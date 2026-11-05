//! Transport-independent application, acknowledgement and lease state.
use crate::protocol::{AppliedFrame, Fault, Mode, Permissions, Phase, RenderFrame, Rpc};
use std::collections::{BTreeMap, BTreeSet};

/// Adapter-specific source admission limits, independent of frame, lease and byte budgets.
/// `None` allows any source count within those shared resource budgets.
#[derive(Clone, Copy, Debug)]
pub struct SourceLimits {
    pub gateways: Option<usize>,
    pub services: Option<usize>,
}
impl Default for SourceLimits {
    fn default() -> Self {
        // Preserve the native mobile admission contract unless an adapter opts out.
        Self {
            gateways: Some(2),
            services: Some(8),
        }
    }
}

/// Accept validates a frame; acknowledge is called only after the renderer applied it.
/// Rejected frames leave all state intact. The adapter owns cancellation for returned leases.
pub struct SessionState {
    mode: Mode,
    permissions: Permissions,
    source_limits: SourceLimits,
    phase: Phase,
    previous: u64,
    pending: Option<String>,
    leases: BTreeMap<String, String>,
    snapshot: omochi_ui::A2uiSnapshot,
}
impl SessionState {
    pub fn new(mode: Mode, permissions: Permissions) -> Result<Self, Fault> {
        Self::with_source_limits(mode, permissions, SourceLimits::default())
    }
    pub fn with_source_limits(
        mode: Mode,
        permissions: Permissions,
        source_limits: SourceLimits,
    ) -> Result<Self, Fault> {
        Ok(Self {
            mode,
            permissions,
            source_limits,
            phase: Phase::Snapshot,
            previous: 0,
            pending: None,
            leases: BTreeMap::new(),
            snapshot: omochi_ui::A2uiSnapshot::new(),
        })
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn pending_ack(&self) -> Option<&str> {
        self.pending.as_deref()
    }
    pub fn check_ack(&self, ack: Option<&str>) -> Result<(), Fault> {
        if self.phase == Phase::Closed {
            return Err(Fault::ScopeExpired);
        }
        if self.pending.as_deref() != ack {
            return Err(Fault::ProtocolError);
        }
        Ok(())
    }
    pub fn acknowledge(&mut self, ack: &str) -> Result<(), Fault> {
        self.check_ack(Some(ack))?;
        self.pending = None;
        Ok(())
    }
    pub fn accept(&mut self, frame: &RenderFrame) -> Result<AppliedFrame, Fault> {
        if self.phase == Phase::Closed {
            return Err(Fault::ScopeExpired);
        }
        if self.pending.is_some() {
            return Err(Fault::ProtocolError);
        }
        let sequence = frame.sequence();
        let number: u64 = sequence.parse().map_err(|_| Fault::ProtocolError)?;
        if number <= self.previous || number.to_string() != sequence {
            return Err(Fault::ProtocolError);
        }
        let mut effect = AppliedFrame {
            sequence: sequence.into(),
            surface: None,
            lease: None,
            invalidated_leases: vec![],
            snapshot_end: false,
        };
        match frame {
            RenderFrame::SnapshotEnd { .. } => {
                if self.phase != Phase::Snapshot {
                    return Err(Fault::ProtocolError);
                }
                effect.snapshot_end = true;
            }
            RenderFrame::Message { message, lease, .. } => {
                if message["version"] != "v0.9" {
                    return Err(Fault::ProtocolError);
                }
                let keys = [
                    "createSurface",
                    "updateComponents",
                    "updateDataModel",
                    "deleteSurface",
                ];
                let bodies = keys
                    .iter()
                    .filter_map(|key| message.get(key).map(|v| (*key, v)))
                    .collect::<Vec<_>>();
                if bodies.len() != 1 {
                    return Err(Fault::ProtocolError);
                }
                let (kind, body) = bodies[0];
                let surface = body["surfaceId"].as_str().ok_or(Fault::ProtocolError)?;
                if kind != "deleteSurface" {
                    let lease = lease
                        .as_deref()
                        .filter(|v| !v.is_empty() && v.len() <= 100)
                        .ok_or(Fault::ProtocolError)?;
                    if self.leases.iter().any(|(s, l)| s != surface && l == lease) {
                        return Err(Fault::ProtocolError);
                    }
                    if !self.leases.contains_key(surface) && self.leases.len() >= 1024 {
                        return Err(Fault::CapacityExceeded);
                    }
                    effect.lease = Some(lease.into());
                }
                // Reject sparse expansion before materializing an untrusted patch.
                omochi_ui::data_path::validate(message).map_err(|_| Fault::ProtocolError)?;
                // Materialize a candidate so validation and limits never partially commit.
                let mut snapshot = self.snapshot.clone();
                snapshot.apply(message).map_err(|_| Fault::ProtocolError)?;
                let surfaces = snapshot.surface_ids();
                let services = surfaces
                    .iter()
                    .filter_map(|id| id.split_once('/').map(|v| v.0))
                    .collect::<BTreeSet<_>>();
                let nodes = services
                    .iter()
                    .filter_map(|id| id.split_once('~').map(|v| v.0))
                    .collect::<BTreeSet<_>>();
                if self
                    .source_limits
                    .gateways
                    .is_some_and(|limit| nodes.len() > limit)
                    || self
                        .source_limits
                        .services
                        .is_some_and(|limit| services.len() > limit)
                {
                    return Err(Fault::CapacityExceeded);
                }
                let bytes = snapshot
                    .messages()
                    .iter()
                    .map(|v| serde_json::to_vec(v).map(|b| b.len()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| Fault::ProtocolError)?
                    .into_iter()
                    .sum::<usize>();
                if bytes > 32 * 1024 * 1024 {
                    return Err(Fault::CapacityExceeded);
                }
                if let Some(old) = self.leases.get(surface)
                    && (kind == "deleteSurface"
                        || kind == "createSurface"
                        || Some(old) != effect.lease.as_ref())
                {
                    effect.invalidated_leases.push(old.clone());
                }
                if let Some(lease) = &effect.lease {
                    self.leases.insert(surface.into(), lease.clone());
                } else {
                    self.leases.remove(surface);
                }
                self.snapshot = snapshot;
                effect.surface = Some(surface.into());
            }
        }
        if effect.snapshot_end {
            self.phase = Phase::Live;
        }
        self.previous = number;
        self.pending = Some(sequence.into());
        Ok(effect)
    }
    pub fn authorize(&self, rpc: &Rpc) -> Result<(), Fault> {
        if self.phase == Phase::Closed {
            return Err(Fault::ScopeExpired);
        }
        match rpc {
            Rpc::Ping {} => Ok(()),
            Rpc::Control { .. } if self.mode == Mode::Owner => Ok(()),
            Rpc::Layout { .. } if self.permissions.edit_layout => Ok(()),
            Rpc::Action { lease, .. } | Rpc::Image { lease, .. } => {
                if !self.leases.values().any(|v| v == lease) {
                    return Err(Fault::ScopeExpired);
                }
                if matches!(rpc, Rpc::Action { .. }) && !self.permissions.interact {
                    return Err(Fault::NotAuthorized);
                }
                Ok(())
            }
            _ => Err(Fault::NotAuthorized),
        }
    }
    pub fn close(&mut self) -> Vec<String> {
        self.phase = Phase::Closed;
        self.pending = None;
        self.snapshot = omochi_ui::A2uiSnapshot::new();
        std::mem::take(&mut self.leases).into_values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn default_admission_preserves_two_gateways_and_eight_services() {
        let mut state = SessionState::new(Mode::Owner, Permissions::default()).unwrap();
        let create = |sequence: usize, surface: &str| RenderFrame::Message {
            sequence: sequence.to_string(),
            message: json!({"version": "v0.9", "createSurface": {"surfaceId": surface}}),
            lease: Some(format!("lease{sequence}")),
        };
        // The third Gateway is rejected even while the service budget has room.
        for index in 0..2 {
            state
                .accept(&create(index + 1, &format!("node{index}~app{index}/main")))
                .unwrap();
            state.acknowledge(&(index + 1).to_string()).unwrap();
        }
        assert_eq!(
            state.accept(&create(3, "node2~app2/main")).unwrap_err(),
            Fault::CapacityExceeded
        );
        for index in 2..8 {
            state
                .accept(&create(
                    index + 1,
                    &format!("node{}~app{index}/main", index % 2),
                ))
                .unwrap();
            state.acknowledge(&(index + 1).to_string()).unwrap();
        }
        let before = state.snapshot.messages();
        assert_eq!(
            state.accept(&create(9, "node0~app8/main")).unwrap_err(),
            Fault::CapacityExceeded
        );
        assert_eq!(state.snapshot.messages(), before);
        assert_eq!(state.previous, 8);
        assert_eq!(state.pending_ack(), None);
        assert_eq!(state.close().len(), 8);
    }

    fn data_frame(sequence: &str, path: Value, lease: &str) -> RenderFrame {
        RenderFrame::Message {
            sequence: sequence.into(),
            message: json!({
                "version": "v0.9",
                "updateDataModel": {"surfaceId": "node~app/main", "path": path, "value": "new"}
            }),
            lease: Some(lease.into()),
        }
    }

    #[test]
    fn unsafe_data_paths_are_rejected_before_materialization_without_committing_state() {
        let mut state = SessionState::new(Mode::Owner, Permissions::default()).unwrap();
        state.accept(&RenderFrame::Message {
            sequence: "1".into(),
            message: json!({"version": "v0.9", "createSurface": {"surfaceId": "node~app/main"}}),
            lease: Some("old".into()),
        }).unwrap();
        state.acknowledge("1").unwrap();
        state
            .accept(&data_frame("2", json!("/kept"), "old"))
            .unwrap();
        state.acknowledge("2").unwrap();
        let before = state.snapshot.messages();

        for path in [
            json!("/999999999"),
            json!("/60000/60000"),
            json!("/9999999999999999999999999"),
            json!("/x".repeat(128)),
            json!("/constructor/x"),
            json!("/x/__proto__"),
            json!("/prototype"),
            json!(42),
        ] {
            assert_eq!(
                state
                    .accept(&data_frame("3", path.clone(), "replacement"))
                    .unwrap_err(),
                Fault::ProtocolError,
                "{path}"
            );
            assert_eq!(state.snapshot.messages(), before);
            assert_eq!(state.previous, 2);
            assert_eq!(state.pending_ack(), None);
            assert_eq!(state.phase(), Phase::Snapshot);
            assert_eq!(
                state.leases.get("node~app/main").map(String::as_str),
                Some("old")
            );
        }

        // The rejected frames consume neither their sequence nor replacement lease.
        let applied = state
            .accept(&data_frame("3", json!("/accepted"), "replacement"))
            .unwrap();
        assert_eq!(applied.invalidated_leases, ["old"]);
        assert_eq!(state.pending_ack(), Some("3"));
    }

    #[test]
    fn data_paths_at_the_sparse_allocation_budget_remain_accepted() {
        for path in ["/100000", "/50000/50000"] {
            let mut state = SessionState::new(Mode::Owner, Permissions::default()).unwrap();
            state.accept(&RenderFrame::Message {
                sequence: "1".into(),
                message: json!({"version": "v0.9", "createSurface": {"surfaceId": "node~app/main"}}),
                lease: Some("lease".into()),
            }).unwrap();
            state.acknowledge("1").unwrap();
            state
                .accept(&data_frame("2", json!(path), "lease"))
                .unwrap();
            let messages = state.snapshot.messages();
            assert_eq!(
                messages[1]["updateDataModel"]["value"].pointer(path),
                Some(&json!("new"))
            );
        }
    }
}
