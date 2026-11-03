//! Native admission and retained-snapshot accounting, including frozen replay views.
use omochi_protocol::session::Service;
use omochi_ui::{A2uiSnapshot, SnapshotCursor};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

pub const MAX_GATEWAYS: usize = 2;
pub const MAX_SERVICES: usize = 8;
pub const MAX_SERVICE_SNAPSHOT: usize = 16 * 1024 * 1024;
pub const MAX_TOTAL_SNAPSHOT: usize = 32 * 1024 * 1024;
pub const MAX_RENDER_MESSAGE: usize = crate::scoped::MAX_BATCH_BYTES - 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityError {
    DiscoveryPending,
    ServiceLimit,
    SnapshotLimit,
    MessageLimit,
}
impl std::fmt::Display for CapacityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::DiscoveryPending => "waiting for gateway service discovery",
            Self::ServiceLimit => "service capacity reached",
            Self::SnapshotLimit => "snapshot capacity reached",
            Self::MessageLimit => "render message exceeds batch capacity",
        })
    }
}
impl std::error::Error for CapacityError {}

#[derive(Default)]
struct Admission {
    rosters: BTreeMap<String, Option<BTreeMap<String, u64>>>,
    active: BTreeSet<String>,
    disabled: BTreeMap<String, u64>,
}
pub(crate) struct Capacity {
    admission: Mutex<Admission>,
    bytes: Mutex<usize>,
    service_limit: usize,
    byte_limit: usize,
    changed: watch::Sender<u64>,
}
impl Default for Capacity {
    fn default() -> Self {
        Self::new(MAX_SERVICES, MAX_TOTAL_SNAPSHOT)
    }
}
impl Capacity {
    fn new(service_limit: usize, byte_limit: usize) -> Self {
        let (changed, _) = watch::channel(0);
        Self {
            admission: Mutex::default(),
            bytes: Mutex::new(0),
            service_limit,
            byte_limit,
            changed,
        }
    }
    fn changed(&self) {
        self.changed.send_modify(|generation| *generation += 1);
    }
    pub(crate) fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }
    pub(crate) fn gateways(&self, ids: &BTreeSet<String>) {
        let mut state = self.admission.lock().unwrap();
        let old: BTreeSet<_> = state.rosters.keys().cloned().collect();
        if old == *ids {
            return;
        }
        state.rosters.retain(|id, _| ids.contains(id));
        for id in ids {
            state.rosters.entry(id.clone()).or_insert(None);
        }
        drop(state);
        self.changed();
    }
    pub(crate) fn roster(&self, gateway: &str, services: &BTreeMap<String, Service>) {
        let mut state = self.admission.lock().unwrap();
        let next: BTreeMap<_, _> = services
            .iter()
            .map(|(id, s)| (format!("{gateway}~{id}"), s.generation))
            .collect();
        let old = state.rosters.insert(gateway.into(), Some(next.clone()));
        if old.as_ref() == Some(&Some(next.clone())) {
            return;
        }
        state.disabled.retain(|route, generation| {
            !route.starts_with(&format!("{gateway}~")) || next.get(route) == Some(generation)
        });
        drop(state);
        self.changed();
    }
    pub(crate) fn offline(&self, gateway: &str) {
        self.roster(gateway, &BTreeMap::new());
    }
    pub(crate) fn disable(&self, route: &str, generation: u64) {
        let mut state = self.admission.lock().unwrap();
        if !state
            .rosters
            .values()
            .filter_map(Option::as_ref)
            .any(|roster| roster.get(route) == Some(&generation))
        {
            return;
        }
        state.disabled.insert(route.into(), generation);
        drop(state);
        self.changed();
    }
    pub(crate) fn admit(self: &Arc<Self>, route: &str) -> Result<ServiceLease, CapacityError> {
        let mut state = self.admission.lock().unwrap();
        if state.disabled.contains_key(route) {
            return Err(CapacityError::SnapshotLimit);
        }
        let gateway = route.split_once('~').unwrap().0;
        if state
            .rosters
            .iter()
            .any(|(id, roster)| id.as_str() < gateway && roster.is_none())
        {
            return Err(CapacityError::DiscoveryPending);
        }
        let remaining = self.service_limit.saturating_sub(state.active.len());
        // Reserve eligibility for the earliest known IDs even when their task has
        // not yet polled. Existing admissions are never displaced by later arrivals.
        let eligible = state
            .rosters
            .values()
            .filter_map(Option::as_ref)
            .flat_map(|roster| roster.keys())
            .filter(|candidate| {
                !state.active.contains(*candidate) && !state.disabled.contains_key(*candidate)
            })
            .take(remaining)
            .any(|candidate| candidate == route);
        if !eligible || state.active.contains(route) {
            return Err(CapacityError::ServiceLimit);
        }
        state.active.insert(route.into());
        Ok(ServiceLease {
            owner: self.clone(),
            route: route.into(),
        })
    }
    pub(crate) fn snapshot(
        self: &Arc<Self>,
        bytes: usize,
    ) -> Result<Arc<SnapshotLease>, CapacityError> {
        let lease = Arc::new(SnapshotLease {
            owner: self.clone(),
            bytes: Mutex::new(0),
        });
        lease.resize(bytes)?;
        Ok(lease)
    }
}

pub(crate) struct ServiceLease {
    owner: Arc<Capacity>,
    route: String,
}
impl Drop for ServiceLease {
    fn drop(&mut self) {
        self.owner
            .admission
            .lock()
            .unwrap()
            .active
            .remove(&self.route);
        self.owner.changed();
    }
}
pub(crate) struct SnapshotLease {
    owner: Arc<Capacity>,
    bytes: Mutex<usize>,
}
impl SnapshotLease {
    pub(crate) fn resize(&self, bytes: usize) -> Result<(), CapacityError> {
        if bytes > MAX_SERVICE_SNAPSHOT {
            return Err(CapacityError::SnapshotLimit);
        }
        let mut current = self.bytes.lock().unwrap();
        let mut total = self.owner.bytes.lock().unwrap();
        let next = total.saturating_sub(*current).saturating_add(bytes);
        if next > self.owner.byte_limit {
            return Err(CapacityError::SnapshotLimit);
        }
        *total = next;
        *current = bytes;
        Ok(())
    }
    pub(crate) fn copy(&self) -> Result<Arc<Self>, CapacityError> {
        let bytes = *self.bytes.lock().unwrap();
        self.owner.snapshot(bytes)
    }
}
impl Drop for SnapshotLease {
    fn drop(&mut self) {
        *self.owner.bytes.lock().unwrap() -= *self.bytes.lock().unwrap();
    }
}

pub(crate) fn snapshot_bytes(snapshot: &A2uiSnapshot, native: bool) -> anyhow::Result<usize> {
    // Serialize/count one replay message at a time rather than materializing the
    // whole service into another Vec<Value> just to account it.
    let mut cursor = SnapshotCursor::default();
    let mut total = 2usize;
    let mut first = true;
    while let Some(message) = snapshot.next_message(&mut cursor) {
        let size = serde_json::to_vec(&message)?.len();
        if native && size > MAX_RENDER_MESSAGE {
            return Err(CapacityError::MessageLimit.into());
        }
        if native && let Some(surface) = crate::scoped::surface_id(&message) {
            let route = surface.split_once('/').map_or("", |(route, _)| route);
            let origin = serde_json::json!({"route":route,"surface":surface,"instance":u64::MAX,"generation":u64::MAX});
            // Surface IDs are service data too. Account for escaped origin
            // metadata before accepting this service, so one very long ID cannot
            // later overflow (and invalidate) the entire render subscription.
            if size + serde_json::to_vec(&origin)?.len() + 256 > crate::scoped::MAX_BATCH_BYTES {
                return Err(CapacityError::MessageLimit.into());
            }
        }
        total = total.saturating_add(size + usize::from(!first));
        first = false;
        if total > MAX_SERVICE_SNAPSHOT {
            return Err(CapacityError::SnapshotLimit.into());
        }
    }
    Ok(total)
}

pub(crate) fn apply_retained(
    snapshot: &mut Arc<A2uiSnapshot>,
    charge: &mut Option<Arc<SnapshotLease>>,
    message: &Value,
) -> anyhow::Result<()> {
    if Arc::strong_count(snapshot) > 1
        && let Some(previous) = charge
    {
        // Charge the frozen version and replacement independently before cloning.
        let replacement = previous.copy()?;
        *charge = Some(replacement);
    }
    omochi_protocol::data_path::validate(message)?;
    Arc::make_mut(snapshot).apply(message)?;
    let bytes = snapshot_bytes(snapshot, charge.is_some())?;
    if let Some(charge) = charge {
        charge.resize(bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_accounting_includes_origin_and_exact_array_separators() {
        let mut snapshot = A2uiSnapshot::new();
        snapshot.apply(&serde_json::json!({"version":"v0.9","createSurface":{"surfaceId":"node~app/main","catalogId":"test"}})).unwrap();
        assert_eq!(
            snapshot_bytes(&snapshot, false).unwrap(),
            serde_json::to_vec(&snapshot.messages()).unwrap().len()
        );
        let mut oversized = A2uiSnapshot::new();
        oversized.apply(&serde_json::json!({"version":"v0.9","createSurface":{"surfaceId":format!("node~app/{}", "x".repeat(2 * 1024 * 1024)),"catalogId":"test"}})).unwrap();
        assert!(snapshot_bytes(&oversized, false).is_ok());
        assert_eq!(
            snapshot_bytes(&oversized, true)
                .unwrap_err()
                .downcast_ref::<CapacityError>(),
            Some(&CapacityError::MessageLimit)
        );
    }

    fn roster(ids: &[&str]) -> BTreeMap<String, Service> {
        ids.iter()
            .map(|id| {
                (
                    id.to_string(),
                    Service {
                        id: id.to_string(),
                        name: id.to_string(),
                        generation: 1,
                        icon: None,
                    },
                )
            })
            .collect()
    }
    #[test]
    fn discovery_order_does_not_change_initial_admission_and_existing_services_are_kept() {
        let limits = Arc::new(Capacity::new(2, 1024));
        limits.gateways(&["a".into(), "b".into()].into());
        limits.roster("b", &roster(&["two", "one"]));
        assert!(matches!(
            limits.admit("b~one"),
            Err(CapacityError::DiscoveryPending)
        ));
        limits.roster("a", &roster(&["first"]));
        assert!(matches!(
            limits.admit("b~two"),
            Err(CapacityError::ServiceLimit)
        ));
        let b = limits.admit("b~one").unwrap();
        let a = limits.admit("a~first").unwrap();
        limits.roster("a", &roster(&["earlier", "first"]));
        assert!(matches!(
            limits.admit("a~earlier"),
            Err(CapacityError::ServiceLimit)
        ));
        drop(b);
        let earlier = limits.admit("a~earlier").unwrap();
        drop(a);
        drop(earlier);
    }
    #[test]
    fn failed_initial_gateway_unblocks_others_and_failed_service_does_not_retry_forever() {
        let limits = Arc::new(Capacity::new(1, 1024));
        limits.gateways(&["a".into(), "b".into()].into());
        limits.roster("b", &roster(&["one", "two"]));
        limits.offline("a");
        let first = limits.admit("b~one").unwrap();
        limits.disable("b~one", 1);
        drop(first);
        let _next = limits.admit("b~two").unwrap();
        assert!(matches!(
            limits.admit("b~one"),
            Err(CapacityError::SnapshotLimit)
        ));
    }
    #[test]
    fn frozen_snapshots_keep_their_byte_charge_until_last_view_is_released() {
        let limits = Arc::new(Capacity::new(8, 10));
        let snapshot = limits.snapshot(6).unwrap();
        let view = snapshot.clone();
        assert!(snapshot.copy().is_err());
        drop(snapshot);
        assert!(limits.snapshot(5).is_err());
        drop(view);
        let another = limits.snapshot(5).unwrap();
        assert!(another.resize(11).is_err());
        assert_eq!(*limits.bytes.lock().unwrap(), 5);
        drop(another);
        assert_eq!(*limits.bytes.lock().unwrap(), 0);
    }
}
