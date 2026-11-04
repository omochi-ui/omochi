//! Pull-based native rendering and opaque, lifetime-bound Surface operations.
use crate::{
    Channel, Hub, OpenStream, State, capacity::CapacityError, read_frame, routing, write_frame,
};
use anyhow::{Context, Result, bail};
use omochi_protocol::fanout;
use omochi_ui::{A2uiSnapshot, SnapshotCursor};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::Duration,
};
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;

pub const MAX_BATCH_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Serialize)]
pub(crate) struct Scope {
    pub route: String,
    pub surface: String,
    pub instance: u64,
    pub generation: u64,
    #[serde(skip)]
    pub stop: CancellationToken,
}

#[derive(Serialize)]
pub(crate) struct HubEvent {
    #[serde(serialize_with = "serialize_message")]
    pub message: Arc<Value>,
    pub scope: Option<Scope>,
}
fn serialize_message<S: serde::Serializer>(
    value: &Arc<Value>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value.as_ref().serialize(serializer)
}

pub struct EventReceiver {
    source: fanout::Receiver<HubEvent>,
}
impl EventReceiver {
    pub(crate) fn scoped(source: fanout::Receiver<HubEvent>) -> Self {
        Self { source }
    }
    pub async fn recv(&mut self) -> Result<Arc<Value>, RecvError> {
        Ok(self.source.recv().await?.message.clone())
    }
}

impl State {
    pub(crate) fn emit(&self, message: Value, scope: Option<Scope>) {
        let _ = self.events.send(Arc::new(HubEvent {
            message: Arc::new(message),
            scope,
        }));
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        self.lifetime.cancel();
    }
}

#[derive(Clone)]
pub struct SurfaceHandle {
    pub(crate) hub: Hub,
    pub(crate) scope: Scope,
    subscription: CancellationToken,
}
impl SurfaceHandle {
    pub async fn image(
        &self,
        uri: &str,
        cancel: CancellationToken,
    ) -> Result<crate::images::ImageData, crate::images::ImageError> {
        self.hub.images.fetch(self, uri, cancel).await
    }
    pub fn surface_id(&self) -> &str {
        &self.scope.surface
    }
    pub fn is_cancelled(&self) -> bool {
        self.hub.stop.is_cancelled()
            || self.scope.stop.is_cancelled()
            || self.subscription.is_cancelled()
    }
    pub(crate) fn validate<'a>(&self, state: &'a State) -> Result<&'a Channel> {
        if self.is_cancelled() {
            bail!("Surface handle expired")
        }
        let channel = state
            .channels
            .get(&self.scope.route)
            .context("service unavailable")?;
        if channel.instance != self.scope.instance
            || channel
                .scopes
                .get(&self.scope.surface)
                .is_none_or(|scope| scope.generation != self.scope.generation)
            || state
                .blocked_gateways
                .contains(routing::split_route(&self.scope.route)?.0)
        {
            bail!("Surface handle expired")
        }
        Ok(channel)
    }

    pub async fn action(&self, mut message: Value) -> Result<()> {
        let kinds: Vec<_> = ["action", "error"]
            .into_iter()
            .filter(|key| message.get(key).is_some())
            .collect();
        if kinds.len() != 1 {
            bail!("invalid client action")
        }
        let body = message[kinds[0]]
            .as_object_mut()
            .context("invalid client action")?;
        // The trusted renderer supplies the handle. Ignore any service-provided destination.
        body.insert("surfaceId".into(), self.scope.surface.clone().into());
        if let Some(backend) = &self.hub.standalone {
            let state = backend.state.lock().await;
            self.validate_stdio(&state)?;
            return backend.enqueue_action(crate::OutboundAction {
                message,
                scope: Some((self.scope.stop.clone(), self.subscription.clone())),
            });
        }
        let (_, message) = routing::route_action(message)?;
        let bytes = serde_json::to_vec(&message)?.len();
        let state = self.hub.state.lock().await;
        let channel = self.validate(&state)?;
        channel
            .actions
            .try_send(
                crate::OutboundAction {
                    message,
                    scope: Some((self.scope.stop.clone(), self.subscription.clone())),
                },
                bytes,
            )
            .map_err(|_| anyhow::anyhow!("service busy or disconnected"))
    }

    pub(crate) fn validate_stdio(&self, state: &crate::stdio::State) -> Result<()> {
        if self.is_cancelled()
            || self.scope.route != "_"
            || state
                .scopes
                .get(&self.scope.surface)
                .is_none_or(|scope| scope.generation != self.scope.generation)
        {
            bail!("Surface handle expired")
        }
        Ok(())
    }
    pub(crate) async fn validate_current(&self) -> Result<()> {
        if let Some(backend) = &self.hub.standalone {
            return self.validate_stdio(&*backend.state.lock().await);
        }
        let state = self.hub.state.lock().await;
        self.validate(&state)?;
        Ok(())
    }

    /// Used by the common image resolver; the UI never chooses a resource route.
    pub async fn resource(&self, request: Value) -> Result<Value> {
        if let Some(backend) = &self.hub.standalone {
            self.validate_current().await?;
            let response = tokio::select! {
                _ = self.cancelled() => bail!("Surface handle expired"),
                result = backend.resource(request) => result?,
            };
            self.validate_current().await?;
            return Ok(response);
        }
        let (connection, service) = {
            let state = self.hub.state.lock().await;
            let channel = self.validate(&state)?;
            (channel.connection.clone(), channel.service.clone())
        };
        let response = tokio::select! {
            _ = self.cancelled() => bail!("Surface handle expired"),
            result = tokio::time::timeout(Duration::from_secs(20), async {
                let (mut reader, mut writer) = connection.open(OpenStream::Resource {
                    service: service.id, generation: service.generation,
                }).await?;
                write_frame(&mut writer, &request).await?;
                read_frame(&mut reader).await?.context("resource connection closed")
            }) => result??,
        };
        self.validate_current().await?;
        Ok(response)
    }

    pub(crate) async fn cancelled(&self) {
        tokio::select! {
            _ = self.subscription.cancelled() => {},
            _ = self.scope.stop.cancelled() => {},
            _ = self.hub.stop.cancelled() => {},
        }
    }
}

struct SnapshotView {
    snapshot: Arc<A2uiSnapshot>,
    scopes: BTreeMap<String, Scope>,
    cursor: SnapshotCursor,
    _charge: Option<Arc<crate::capacity::SnapshotLease>>,
}

/// One message per batch bounds copies even for a full initial snapshot.
/// Acknowledgement means the renderer has applied the batch, not merely queued it.
pub struct ScopedBatch {
    pub epoch: u64,
    pub sequence: u64,
    pub snapshot: bool,
    pub snapshot_end: bool,
    pub message: Option<Arc<Value>>,
    pub handle: Option<SurfaceHandle>,
}
pub struct ScopedSubscription {
    hub: Hub,
    epoch: u64,
    sequence: u64,
    snapshot_end: bool,
    views: VecDeque<SnapshotView>,
    events: fanout::Receiver<HubEvent>,
    stop: CancellationToken,
    in_flight: Option<Arc<ScopedBatch>>,
}
impl Drop for ScopedSubscription {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
impl ScopedSubscription {
    /// A host may immediately invalidate all operations from this subscription
    /// before awaiting a replacement or lifecycle command.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.stop.clone()
    }
    pub fn cancel(&mut self) {
        self.stop.cancel();
        self.views.clear();
        self.in_flight = None;
    }
    pub fn acknowledge(&mut self, sequence: u64) -> Result<()> {
        if self.events.has_gap() {
            self.cancel();
            bail!("subscription overflow; discard old UI and resubscribe")
        }
        if self.stop.is_cancelled() {
            bail!("subscription ended")
        }
        if self
            .in_flight
            .as_ref()
            .is_none_or(|batch| batch.sequence != sequence)
        {
            bail!("incorrect batch acknowledgement")
        }
        self.in_flight = None;
        Ok(())
    }
    pub async fn next_batch(&mut self) -> Result<Arc<ScopedBatch>> {
        if self.stop.is_cancelled() {
            bail!("subscription ended")
        }
        if self.events.has_gap() {
            self.cancel();
            bail!("subscription overflow; discard old UI and resubscribe")
        }
        if self.in_flight.is_some() {
            bail!("apply and acknowledge the previous batch first")
        }
        let snapshot = !self.snapshot_end;
        let mut event = None;
        while let Some(view) = self.views.front_mut() {
            if let Some(message) = view.snapshot.next_message(&mut view.cursor) {
                let scope = surface_id(&message)
                    .and_then(|id| view.scopes.get(id))
                    .cloned();
                event = Some(Arc::new(HubEvent {
                    message: Arc::new(message),
                    scope,
                }));
                break;
            }
            self.views.pop_front();
        }
        let snapshot_end = snapshot && event.is_none();
        if snapshot_end {
            self.snapshot_end = true;
        }
        if !snapshot {
            let result = tokio::select! {
                _ = self.stop.cancelled() => bail!("subscription ended"),
                result = self.events.recv() => result,
            };
            match result {
                Ok(update) => event = Some(update),
                Err(_) => {
                    self.cancel();
                    bail!("subscription ended or overflowed; discard old UI and resubscribe")
                }
            }
        }
        if let Some(event) = &event {
            // Include origin metadata and reserve space for the binding envelope.
            if serde_json::to_vec(event.as_ref())?.len() + 256 > MAX_BATCH_BYTES {
                self.cancel();
                return Err(CapacityError::MessageLimit.into());
            }
        }
        self.sequence += 1;
        let batch = Arc::new(ScopedBatch {
            epoch: self.epoch,
            sequence: self.sequence,
            snapshot,
            snapshot_end,
            handle: event
                .as_ref()
                .and_then(|e| e.scope.as_ref())
                .map(|scope| SurfaceHandle {
                    hub: self.hub.clone(),
                    scope: scope.clone(),
                    subscription: self.stop.clone(),
                }),
            message: event.map(|e| e.message.clone()),
        });
        self.in_flight = Some(batch.clone());
        Ok(batch)
    }
}

pub(crate) fn surface_id(message: &Value) -> Option<&str> {
    [
        "createSurface",
        "updateComponents",
        "updateDataModel",
        "deleteSurface",
    ]
    .into_iter()
    .find_map(|kind| message.get(kind)?.get("surfaceId")?.as_str())
}

impl Hub {
    pub async fn subscribe_scoped(&self) -> Result<ScopedSubscription> {
        self.scoped_subscription(true).await
    }
    #[cfg(feature = "browser")]
    pub(crate) async fn subscribe_browser(&self) -> Result<ScopedSubscription> {
        self.scoped_subscription(false).await
    }
    async fn scoped_subscription(&self, exclusive: bool) -> Result<ScopedSubscription> {
        if self.stop.is_cancelled() {
            bail!("Hub stopped")
        }
        let stop = self.stop.child_token();
        let (views, events) = if let Some(backend) = &self.standalone {
            let state = backend.state.lock().await;
            (
                VecDeque::from([SnapshotView {
                    snapshot: state.snapshot.clone(),
                    scopes: state.scopes.clone(),
                    cursor: SnapshotCursor::default(),
                    _charge: None,
                }]),
                state.events.subscribe(),
            )
        } else {
            let mut state = self.state.lock().await;
            if exclusive && let Some(previous) = state.scoped.replace(stop.clone()) {
                previous.cancel();
            }
            // Freeze snapshots at the event cursor. Browser subscriptions are independent;
            // the native binding deliberately permits only one active renderer.
            let views = state
                .channels
                .values()
                .map(|channel| SnapshotView {
                    snapshot: channel.snapshot.clone(),
                    scopes: channel.scopes.clone(),
                    cursor: SnapshotCursor::default(),
                    _charge: channel.snapshot_charge.clone(),
                })
                .collect();
            (views, state.events.subscribe())
        };
        let lost = stop.clone();
        events.on_gap(move || lost.cancel());
        Ok(ScopedSubscription {
            hub: self.clone(),
            epoch: self
                .epoch
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            sequence: 0,
            snapshot_end: false,
            views,
            events,
            stop,
            in_flight: None,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{Connection, OutboundAction};
    use omochi_protocol::session::Service;
    use serde_json::json;
    use std::path::PathBuf;

    pub(crate) async fn install(
        hub: &Hub,
        path: PathBuf,
    ) -> (
        String,
        tokio::sync::mpsc::Receiver<omochi_protocol::queue::Entry<OutboundAction>>,
    ) {
        let route = format!("{}~app", "a".repeat(64));
        let surface = format!("{route}/main");
        let mut snapshot = A2uiSnapshot::new();
        snapshot
            .apply(
                &json!({"version":"v0.9","createSurface":{"surfaceId":surface,"catalogId":"test"}}),
            )
            .unwrap();
        snapshot.apply(&json!({"version":"v0.9","updateDataModel":{"surfaceId":surface,"path":"/","value":{"value":1}}})).unwrap();
        let lifetime = hub.stop.child_token();
        let instance = hub.epoch.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let scope = Scope {
            route: route.clone(),
            surface: surface.clone(),
            instance,
            generation: 1,
            stop: lifetime.child_token(),
        };
        let (actions, receive) = omochi_protocol::queue::channel(8, 8192, 4096);
        hub.state.lock().await.channels.insert(
            route,
            Channel {
                epoch: 1,
                service: Service {
                    id: "app".into(),
                    name: "App".into(),
                    generation: 1,
                    icon: None,
                },
                node_name: "gateway".into(),
                local: true,
                snapshot: Arc::new(snapshot),
                snapshot_charge: None,
                actions,
                connection: Connection::Unix(path),
                instance,
                lifetime,
                scopes: [(surface.clone(), scope)].into(),
            },
        );
        (surface, receive)
    }

    #[tokio::test]
    async fn snapshot_is_frozen_ordered_and_requires_application_acknowledgement() {
        let hub = Hub::new();
        let (surface, _) = install(&hub, PathBuf::new()).await;
        let mut sub = hub.subscribe_scoped().await.unwrap();
        let update = json!({"version":"v0.9","updateDataModel":{"surfaceId":surface,"path":"/value","value":2}});
        {
            let mut state = hub.state.lock().await;
            let channel = state.channels.values_mut().next().unwrap();
            Arc::make_mut(&mut channel.snapshot).apply(&update).unwrap();
            let scope = channel.scopes[&surface].clone();
            state.emit(update.clone(), Some(scope));
        }
        let create = sub.next_batch().await.unwrap();
        assert!(create.snapshot);
        assert!(create.handle.is_some());
        assert!(sub.next_batch().await.is_err());
        assert!(sub.acknowledge(create.sequence + 1).is_err());
        sub.acknowledge(create.sequence).unwrap();
        let data = sub.next_batch().await.unwrap();
        assert_eq!(
            data.message.as_ref().unwrap()["updateDataModel"]["value"]["value"],
            1
        );
        sub.acknowledge(data.sequence).unwrap();
        let end = sub.next_batch().await.unwrap();
        assert!(end.snapshot_end);
        sub.acknowledge(end.sequence).unwrap();
        let delta = sub.next_batch().await.unwrap();
        assert!(!delta.snapshot);
        assert_eq!(delta.message.as_deref().unwrap(), &update);
    }

    #[tokio::test]
    async fn handle_scopes_forged_destinations_and_expires_on_resubscribe_or_same_generation_reconnect()
     {
        let hub = Hub::new();
        let (surface, mut actions) = install(&hub, PathBuf::new()).await;
        let mut sub = hub.subscribe_scoped().await.unwrap();
        let batch = sub.next_batch().await.unwrap();
        let handle = batch.handle.clone().unwrap();
        let other = format!("{}~other/main", "b".repeat(64));
        handle.action(json!({"version":"v0.9","action":{"surfaceId":other,"name":"click"},"metadata":{"a2uiClientDataModel":{"surfaces":{surface:{"mine":1},other:{"secret":2}}}}})).await.unwrap();
        let (sent, _charge) = actions.recv().await.unwrap().into_parts();
        assert_eq!(sent.message["action"]["surfaceId"], "main");
        assert_eq!(
            sent.message["metadata"]["a2uiClientDataModel"]["surfaces"],
            json!({"main":{"mine":1}})
        );
        let mut new = hub.subscribe_scoped().await.unwrap();
        assert!(handle.is_cancelled());
        assert!(
            handle
                .action(json!({"version":"v0.9","action":{"name":"click"}}))
                .await
                .is_err()
        );
        let fresh = new.next_batch().await.unwrap().handle.clone().unwrap();
        install(&hub, PathBuf::new()).await;
        assert!(fresh.is_cancelled());
        assert!(
            fresh
                .action(json!({"version":"v0.9","action":{"name":"click"}}))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn queue_overflow_invalidates_handles_even_during_snapshot_delivery() {
        let hub = Hub::new();
        let (surface, _) = install(&hub, PathBuf::new()).await;
        let mut sub = hub.subscribe_scoped().await.unwrap();
        let create = sub.next_batch().await.unwrap();
        let handle = create.handle.clone().unwrap();
        {
            let state = hub.state.lock().await;
            for value in 0..33 {
                state.emit(json!({"version":"v0.9","updateDataModel":{"surfaceId":surface,"path":"/value","value":value}}), None);
            }
        }
        // Expiry happens while the renderer is stalled, before it can ACK or
        // request another batch. Even a retained button callback must be denied.
        assert!(handle.is_cancelled());
        assert!(
            handle
                .action(json!({"version":"v0.9","action":{"name":"click"}}))
                .await
                .is_err()
        );
        assert!(sub.acknowledge(create.sequence).is_err());
        assert!(sub.next_batch().await.is_err());
    }

    #[tokio::test]
    async fn revocation_cancels_in_flight_resource_and_rejects_old_results() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("resource.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let hub = Hub::new();
        install(&hub, path).await;
        let mut sub = hub.subscribe_scoped().await.unwrap();
        let handle = sub.next_batch().await.unwrap().handle.clone().unwrap();
        let (started, seen) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let header: OpenStream = read_frame(&mut socket).await.unwrap().unwrap();
            assert!(matches!(header, OpenStream::Resource { generation: 1, .. }));
            let _: Value = read_frame(&mut socket).await.unwrap().unwrap();
            started.send(()).unwrap();
            assert!(read_frame::<_, Value>(&mut socket).await.unwrap().is_none());
        });
        let request = tokio::spawn(async move {
            handle
                .resource(json!({"op":"stat","resource":"a".repeat(32)}))
                .await
        });
        seen.await.unwrap();
        hub.revoke_gateway(&"a".repeat(64)).await;
        assert!(
            tokio::time::timeout(Duration::from_secs(1), request)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        server.await.unwrap();
    }
}
