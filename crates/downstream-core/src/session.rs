//! Transport-independent application, acknowledgement and lease state.
use crate::protocol::{AppliedFrame, Fault, Mode, Permissions, Phase, RenderFrame, Rpc};
use std::collections::{BTreeMap, BTreeSet};

/// Accept validates a frame; acknowledge is called only after the renderer applied it.
/// Rejected frames leave all state intact. The adapter owns cancellation for returned leases.
pub struct SessionState {
    mode: Mode,
    permissions: Permissions,
    phase: Phase,
    previous: u64,
    pending: Option<String>,
    leases: BTreeMap<String, String>,
    snapshot: omochi_ui::A2uiSnapshot,
}
impl SessionState {
    pub fn new(mode: Mode, permissions: Permissions) -> Result<Self, Fault> {
        if mode == Mode::Public && (permissions.interact || permissions.edit_layout) {
            return Err(Fault::ProtocolError);
        }
        Ok(Self {
            mode,
            permissions,
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
                if nodes.len() > 2 || services.len() > 8 {
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
                if let Some(old) = self.leases.get(surface) {
                    if kind == "deleteSurface"
                        || kind == "createSurface"
                        || Some(old) != effect.lease.as_ref()
                    {
                        effect.invalidated_leases.push(old.clone());
                    }
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
