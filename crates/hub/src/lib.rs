//! Hub aggregation independent of browser HTTP and gateway transport.
#[cfg(feature = "browser")]
mod browser_images;
pub mod capacity;
#[cfg(test)]
mod capacity_wire;
#[cfg(feature = "browser")]
pub mod control;
#[cfg(feature = "browser")]
pub mod dashboard_remote;
#[cfg(feature = "browser")]
pub mod dashboards;
#[cfg(all(test, unix, feature = "desktop"))]
mod gateway_ownership;
#[cfg(all(feature = "desktop", feature = "browser"))]
mod gateway_pairing;
mod heartbeat;
pub mod host;
#[cfg(feature = "browser")]
pub mod http;
pub mod identity;
pub mod images;
pub mod native;
#[cfg(feature = "browser")]
pub mod owners;
#[cfg(feature = "browser")]
mod owners_live;
pub mod remote;
#[cfg(feature = "browser")]
mod resource_http;
pub mod routing;
pub mod scoped;
#[cfg(feature = "browser")]
pub mod sharing;
#[cfg(feature = "browser")]
mod sharing_live;
pub mod stdio;
use anyhow::{Context, Result, bail};
use omochi_protocol::{
    OpenStream, VERSION, read_frame,
    session::{Control, Service, ServiceFrame, valid_node_id, valid_service_id},
    write_frame,
};
use omochi_ui::A2uiSnapshot;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::{
    collections::{BTreeMap, BTreeSet},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Mutex, broadcast},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

type Reader = Pin<Box<dyn AsyncRead + Send>>;
type Writer = Pin<Box<dyn AsyncWrite + Send>>;
#[derive(Clone)]
pub enum Connection {
    Local(omochi_local::LocalEndpoint),
    Iroh(omochi_transport::EndpointConnection),
}
impl Connection {
    async fn open(&self, header: OpenStream) -> Result<(Reader, Writer)> {
        let (reader, mut writer): (Reader, Writer) = match self {
            Self::Local(endpoint) => {
                let socket = endpoint.connect().await?;
                let (reader, writer) = socket.into_split();
                (Box::pin(reader), Box::pin(writer))
            }
            Self::Iroh(connection) => {
                let (writer, reader) = connection.open_bi().await?;
                (Box::pin(reader), Box::pin(writer))
            }
        };
        tokio::time::timeout(Duration::from_secs(5), write_frame(&mut writer, &header)).await??;
        Ok((reader, writer))
    }
}
#[derive(Clone, Copy)]
struct GatewaySession<'a> {
    connection: &'a Connection,
    epoch: u64,
    node: &'a str,
    name: &'a str,
    local: bool,
    stop: &'a CancellationToken,
}
struct Channel {
    epoch: u64,
    service: Service,
    node_name: String,
    local: bool,
    snapshot: Arc<A2uiSnapshot>,
    snapshot_charge: Option<Arc<capacity::SnapshotLease>>,
    actions: omochi_protocol::queue::Sender<OutboundAction>,
    connection: Connection,
    instance: u64,
    lifetime: CancellationToken,
    scopes: BTreeMap<String, scoped::Scope>,
}
struct OutboundAction {
    message: Value,
    scope: Option<(CancellationToken, CancellationToken)>,
}
struct State {
    channels: BTreeMap<String, Channel>,
    gateway_owners: BTreeMap<String, u64>,
    events: omochi_protocol::fanout::Sender<scoped::HubEvent>,
    blocked_gateways: BTreeSet<String>,
    scoped: Option<CancellationToken>,
    unavailable: BTreeMap<String, Value>,
}
#[derive(Clone)]
pub struct Hub {
    state: Arc<Mutex<State>>,
    gateway_changes: tokio::sync::watch::Sender<()>,
    changes: broadcast::Sender<()>,
    epoch: Arc<AtomicU64>,
    stop: CancellationToken,
    standalone: Option<stdio::Backend>,
    close_code: Arc<std::sync::atomic::AtomicU16>,
    #[cfg(feature = "browser")]
    browser_tasks: tokio_util::task::TaskTracker,
    #[cfg(feature = "browser")]
    owner_presets: Arc<std::sync::RwLock<Vec<Value>>>,
    #[cfg(feature = "browser")]
    dashboard_host: Arc<std::sync::RwLock<Option<(omochi_transport::Endpoint, String)>>>,
    #[cfg(all(feature = "desktop", feature = "browser"))]
    pairing: Arc<std::sync::RwLock<Option<gateway_pairing::Service>>>,
    diagnostics: remote::Diagnostics,
    peer_names: Arc<Mutex<BTreeMap<String, String>>>,
    images: images::ImageResolver,
    capacity: Option<Arc<capacity::Capacity>>,
}
pub struct Subscription {
    pub snapshot: Vec<Value>,
    pub events: scoped::EventReceiver,
}
impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}
impl Hub {
    pub fn new() -> Self {
        let (events, _) =
            omochi_protocol::fanout::channel(32, omochi_protocol::fanout::DEFAULT_BYTES);
        let (changes, _) = broadcast::channel(16);
        Self {
            state: Arc::new(Mutex::new(State {
                channels: BTreeMap::new(),
                gateway_owners: BTreeMap::new(),
                events,
                blocked_gateways: BTreeSet::new(),
                scoped: None,
                unavailable: BTreeMap::new(),
            })),
            gateway_changes: tokio::sync::watch::channel(()).0,
            changes,
            epoch: Arc::new(AtomicU64::new(1)),
            stop: CancellationToken::new(),
            standalone: None,
            close_code: Arc::new(std::sync::atomic::AtomicU16::new(1001)),
            #[cfg(feature = "browser")]
            browser_tasks: tokio_util::task::TaskTracker::new(),
            #[cfg(feature = "browser")]
            owner_presets: Arc::default(),
            #[cfg(feature = "browser")]
            dashboard_host: Arc::default(),
            #[cfg(all(feature = "desktop", feature = "browser"))]
            pairing: Arc::default(),
            diagnostics: Arc::default(),
            peer_names: Arc::default(),
            images: images::ImageResolver::default(),
            capacity: None,
        }
    }
    pub fn embedded() -> Self {
        let mut hub = Self::new();
        hub.capacity = Some(Arc::new(capacity::Capacity::default()));
        hub
    }
    pub fn attach_stdio(
        input: impl AsyncRead + Unpin + Send + 'static,
        output: impl AsyncWrite + Unpin + Send + 'static,
    ) -> (Self, JoinHandle<Result<()>>) {
        let mut hub = Self::new();
        let (backend, task) = stdio::Backend::attach(input, output, hub.stop.clone());
        hub.standalone = Some(backend);
        (hub, task)
    }
    pub fn shutdown_with_code(&self, code: u16) {
        self.close_code.store(code, Ordering::Release);
        self.shutdown();
    }
    pub fn cancellation_token(&self) -> CancellationToken {
        self.stop.clone()
    }
    pub fn shutdown(&self) {
        self.stop.cancel();
    }
    pub fn changes(&self) -> broadcast::Receiver<()> {
        self.changes.subscribe()
    }
    pub async fn subscribe(&self) -> Subscription {
        if let Some(backend) = &self.standalone {
            return backend.subscribe().await;
        }
        let state = self.state.lock().await;
        Subscription {
            snapshot: state
                .channels
                .values()
                .flat_map(|c| c.snapshot.messages())
                .collect(),
            events: scoped::EventReceiver::scoped(state.events.subscribe()),
        }
    }
    pub async fn connection_status(&self) -> Vec<Value> {
        let statuses = self.diagnostics.lock().await;
        let names = self.peer_names.lock().await;
        statuses
            .iter()
            .map(|(id, status)| json!({"nodeId":id,"name":names.get(id),"status":status}))
            .collect()
    }
    pub async fn services(&self) -> Vec<Value> {
        let state = self.state.lock().await;
        let mut views = state.unavailable.clone();
        views.extend(state.channels.iter().map(|(route, channel)| {
            let mut view = json!({"id":route,"name":channel.service.name,"status":"connected","node":{"id":route.split_once('~').unwrap().0,"name":channel.node_name,"local":channel.local}});
            if let Some(icon) = &channel.service.icon { view["icon"] = icon.clone().into(); } (route.clone(), view)
        }));
        views.into_values().collect()
    }
    pub async fn action(&self, message: Value) -> Result<()> {
        if self.stop.is_cancelled() {
            bail!("Hub stopped")
        }
        if let Some(backend) = &self.standalone {
            return backend.action(message);
        }
        let (route, message) = routing::route_action(message)?;
        let bytes = serde_json::to_vec(&message)?.len();
        let state = self.state.lock().await;
        if state
            .blocked_gateways
            .contains(routing::split_route(&route)?.0)
        {
            bail!("gateway authorization revoked")
        }
        state
            .channels
            .get(&route)
            .context("service unavailable")?
            .actions
            .try_send(
                OutboundAction {
                    message,
                    scope: None,
                },
                bytes,
            )
            .map_err(|_| anyhow::anyhow!("service busy or disconnected"))
    }
    pub async fn resource(&self, route: &str, request: Value) -> Result<Value> {
        if self.stop.is_cancelled() {
            bail!("Hub stopped")
        }
        if let Some(backend) = &self.standalone {
            if route != "_" {
                bail!("unknown resource route");
            }
            return backend.resource(request).await;
        }
        routing::split_route(route)?;
        let (connection, service) = {
            let state = self.state.lock().await;
            if state
                .blocked_gateways
                .contains(routing::split_route(route)?.0)
            {
                bail!("gateway authorization revoked")
            }
            let c = state.channels.get(route).context("service unavailable")?;
            (c.connection.clone(), c.service.clone())
        };
        tokio::time::timeout(Duration::from_secs(20), async {
            let (mut reader, mut writer) = connection
                .open(OpenStream::Resource {
                    service: service.id,
                    generation: service.generation,
                })
                .await?;
            write_frame(&mut writer, &request).await?;
            read_frame(&mut reader)
                .await?
                .context("resource connection closed")
        })
        .await?
    }

    /// Deny immediately, before durable removal or supervisor acknowledgement.
    /// The supervisor still owns joining the connection and resource work.
    pub(crate) async fn revoke_gateway(&self, id: &str) {
        let mut state = self.state.lock().await;
        state.blocked_gateways.insert(id.into());
        let routes: Vec<_> = state
            .channels
            .keys()
            .filter(|route| route.split_once('~').is_some_and(|(node, _)| node == id))
            .cloned()
            .collect();
        for route in routes {
            if let Some(channel) = state.channels.remove(&route) {
                for surface in channel.snapshot.surface_ids() {
                    state.emit(
                        json!({"version":"v0.9","deleteSurface":{"surfaceId":surface}}),
                        None,
                    );
                }
            }
        }
        let _ = self.changes.send(());
    }

    pub(crate) async fn allow_gateway(&self, id: &str) {
        self.state.lock().await.blocked_gateways.remove(id);
    }
    pub fn attach_local(&self, path: PathBuf) -> JoinHandle<()> {
        let hub = self.clone();
        tokio::spawn(async move {
            let mut delay = 100;
            loop {
                if hub.stop.is_cancelled() {
                    return;
                }
                if let Ok(endpoint) = omochi_local::LocalRuntime::open(path.clone())
                    .and_then(|runtime| runtime.gateway_endpoint())
                {
                    let _ = hub.run(Connection::Local(endpoint), None, true).await;
                }
                tokio::select! { _ = hub.stop.cancelled() => return, _ = tokio::time::sleep(Duration::from_millis(delay)) => {} }
                delay = (delay * 2).min(5000);
            }
        })
    }
    async fn remove(&self, route: &str, epoch: u64) {
        let mut state = self.state.lock().await;
        if state.channels.get(route).is_some_and(|c| c.epoch == epoch) {
            if let Some(channel) = state.channels.remove(route) {
                for surface in channel.snapshot.surface_ids() {
                    state.emit(
                        json!({"version":"v0.9","deleteSurface":{"surfaceId":surface}}),
                        None,
                    );
                }
            }
            let _ = self.changes.send(());
        }
    }
    pub async fn run(
        &self,
        connection: Connection,
        expected: Option<String>,
        local: bool,
    ) -> Result<()> {
        self.run_until(connection, expected, local, self.stop.child_token())
            .await
    }
    pub(crate) async fn run_until(
        &self,
        connection: Connection,
        expected: Option<String>,
        local: bool,
        stop: CancellationToken,
    ) -> Result<()> {
        let expected = match &connection {
            Connection::Iroh(connection) => {
                let actual = connection.remote_id().to_string();
                if expected
                    .as_ref()
                    .is_some_and(|expected| expected != &actual)
                {
                    bail!("unexpected authenticated gateway")
                }
                Some(actual)
            }
            Connection::Local(_) => expected,
        };
        let epoch = self.epoch.fetch_add(1, Ordering::Relaxed);
        let (mut reader, writer) = tokio::select! {
            _ = stop.cancelled() => return Ok(()),
            result = tokio::time::timeout(Duration::from_secs(5), connection.open(OpenStream::Control { protocol: VERSION })) => result??,
        };
        let hello = tokio::time::timeout(
            Duration::from_secs(5),
            read_frame::<_, Control>(&mut reader),
        )
        .await??
        .context("missing gateway hello")?;
        let Control::Hello {
            node_id,
            name,
            protocol: VERSION,
        } = hello
        else {
            bail!("incompatible gateway")
        };
        if !valid_node_id(&node_id) || expected.is_some_and(|expected| expected != node_id) {
            bail!("gateway identity mismatch")
        }
        let name = self
            .peer_names
            .lock()
            .await
            .get(&node_id)
            .cloned()
            .unwrap_or(name);
        let session_stop = stop.child_token();
        let session = GatewaySession {
            connection: &connection,
            epoch,
            node: &node_id,
            name: &name,
            local,
            stop: &session_stop,
        };
        let (pong, replies) = tokio::sync::watch::channel(0u64);
        let heartbeat = heartbeat::run(writer, replies, Duration::from_secs(10));
        tokio::pin!(heartbeat);
        let mut services = BTreeMap::new();
        let mut roster = BTreeMap::new();
        let mut gateway_changes = self.gateway_changes.subscribe();
        let mut owner = false;
        let mut capacity_changes = self.capacity.as_ref().map(|capacity| capacity.subscribe());
        let first_roster_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let mut roster_received = false;
        let result = async {
            loop {
                // Reading is never cancelled to publish a service update: the only alternative ends this session.
                let read = read_frame::<_, Control>(&mut reader);
                tokio::pin!(read);
                let frame = loop {
                    tokio::select! {
                        _ = stop.cancelled() => return Ok(()),
                        result = &mut heartbeat => return result,
                        _ = tokio::time::sleep_until(first_roster_deadline), if !roster_received => bail!("gateway service discovery timed out"),
                        frame = &mut read => break frame?,
                        _ = gateway_changes.changed(), if !owner && roster_received => {
                            owner = self.claim_gateway(&session).await;
                            if owner {
                                if let Some(capacity) = &self.capacity { capacity.roster(&node_id, &roster); }
                                self.reconcile_services(&session, &roster, &mut services).await;
                            }
                        }
                        _ = async {
                            if let Some(changes) = &mut capacity_changes { let _ = changes.changed().await; }
                            else { std::future::pending::<()>().await; }
                        } => {
                            if owner {
                                self.reconcile_services(&session, &roster, &mut services).await;
                            }
                        }
                    }
                };
                let Some(frame) = frame else { bail!("gateway disconnected") };
                match frame {
                    Control::Services { services: listed } => {
                        roster_received = true;
                        if listed.len() > 256 { bail!("too many services") }
                        let mut next = BTreeMap::new();
                        for spec in listed {
                            if !valid_service_id(&spec.id) || spec.generation == 0 || next.insert(spec.id.clone(), spec).is_some() { bail!("invalid service list") }
                        }
                        roster = next;
                        owner = self.claim_gateway(&session).await;
                        if owner {
                            if let Some(capacity) = &self.capacity { capacity.roster(&node_id, &roster); }
                            self.reconcile_services(&session, &roster, &mut services).await;
                        }
                        if !local {
                            self.diagnostics.lock().await.insert(node_id.clone(), "connected".into());
                        }
                    }
                    Control::Pong { id } => { pong.send_replace(id); },
                    Control::Bye { reason } => bail!("gateway closed: {reason}"),
                    _ => bail!("unexpected control message"),
                }
            }
        }.await;
        session_stop.cancel();
        for (id, (_, _, task)) in services {
            let _ = task.await;
            self.remove(&format!("{node_id}~{id}"), epoch).await;
        }
        if owner {
            if let Some(capacity) = &self.capacity {
                capacity.offline(&node_id);
            }
            let mut state = self.state.lock().await;
            state
                .unavailable
                .retain(|route, _| !route.starts_with(&format!("{node_id}~")));
            state.gateway_owners.remove(&node_id);
            // Transfer only after all old streams have ended and their surfaces
            // have been retracted. Standby control sessions retain fresh rosters.
            self.gateway_changes.send_replace(());
        }
        result
    }

    async fn claim_gateway(&self, session: &GatewaySession<'_>) -> bool {
        let mut state = self.state.lock().await;
        // Unix and authenticated iroh can reach the same node. Keep its current
        // session until it ends instead of letting their service streams replace
        // one another's snapshots, action queues and scope generations.
        *state
            .gateway_owners
            .entry(session.node.into())
            .or_insert(session.epoch)
            == session.epoch
    }

    async fn service_unavailable(
        &self,
        route: &str,
        service: &Service,
        node_name: &str,
        local: bool,
        error: capacity::CapacityError,
    ) {
        self.state.lock().await.unavailable.insert(
            route.into(),
            json!({
                "id":route,"name":service.name,"status":"error","detail":error.to_string(),
                "node":{"id":route.split_once('~').unwrap().0,"name":node_name,"local":local}
            }),
        );
        let _ = self.changes.send(());
    }

    async fn reconcile_services(
        &self,
        session: &GatewaySession<'_>,
        next: &BTreeMap<String, Service>,
        services: &mut BTreeMap<String, (Service, CancellationToken, JoinHandle<()>)>,
    ) {
        let GatewaySession {
            connection,
            epoch,
            node,
            name,
            local,
            stop: session_stop,
        } = *session;
        let removed: Vec<_> = services
            .iter()
            .filter(|(id, (spec, _, _))| next.get(*id) != Some(spec))
            .map(|(id, _)| id.clone())
            .collect();
        for id in removed {
            let (_, stop, task) = services.remove(&id).unwrap();
            stop.cancel();
            let _ = task.await;
            self.remove(&format!("{node}~{id}"), epoch).await;
        }
        self.state.lock().await.unavailable.retain(|route, _| {
            route
                .split_once('~')
                .is_none_or(|(gateway, id)| gateway != node || next.contains_key(id))
        });
        for (id, spec) in next {
            if services.contains_key(id) {
                continue;
            }
            let route = format!("{node}~{id}");
            let permit = match &self.capacity {
                Some(capacity) => match capacity.admit(&route) {
                    Ok(permit) => Some(permit),
                    Err(error) => {
                        self.service_unavailable(&route, spec, name, local, error)
                            .await;
                        continue;
                    }
                },
                None => None,
            };
            self.state.lock().await.unavailable.remove(&route);
            let stop = session_stop.child_token();
            let token = stop.clone();
            let hub = self.clone();
            let conn = connection.clone();
            let node = node.to_owned();
            let name = name.to_owned();
            let service = spec.clone();
            let task = tokio::spawn(async move {
                let _permit = permit;
                loop {
                    let result = tokio::select! {
                        _=token.cancelled()=>break,
                        result=hub.service_stream(conn.clone(),epoch,&node,&name,local,service.clone())=>result,
                    };
                    hub.remove(&route, epoch).await;
                    if let Err(error) = result
                        && let Some(error) = error.downcast_ref::<capacity::CapacityError>()
                    {
                        if let Some(capacity) = &hub.capacity {
                            capacity.disable(&route, service.generation);
                        }
                        hub.service_unavailable(&route, &service, &name, local, *error)
                            .await;
                        break;
                    }
                    tokio::select! { _=token.cancelled()=>break, _=tokio::time::sleep(Duration::from_millis(250))=>{} }
                }
            });
            services.insert(id.clone(), (spec.clone(), stop, task));
        }
    }
    async fn service_stream(
        &self,
        connection: Connection,
        epoch: u64,
        node: &str,
        name: &str,
        local: bool,
        service: Service,
    ) -> Result<()> {
        let route = format!("{node}~{}", service.id);
        let (mut reader, mut writer) = connection
            .open(OpenStream::Service {
                service: service.id.clone(),
                generation: service.generation,
            })
            .await?;
        let mut snapshot = A2uiSnapshot::new();
        let snapshot_charge = self
            .capacity
            .as_ref()
            .map(|capacity| capacity.snapshot(0))
            .transpose()?;
        if !matches!(
            read_frame::<_, ServiceFrame>(&mut reader).await?,
            Some(ServiceFrame::SnapshotStart)
        ) {
            bail!("missing snapshot start")
        }
        loop {
            match read_frame::<_, ServiceFrame>(&mut reader).await? {
                Some(ServiceFrame::Message { message }) => {
                    apply_snapshot(&mut snapshot, &routing::prefix(&route, message)?)?;
                    if let Some(charge) = &snapshot_charge {
                        charge.resize(capacity::snapshot_bytes(&snapshot, true)?)?;
                    }
                }
                Some(ServiceFrame::SnapshotEnd) => break,
                _ => bail!("incomplete snapshot"),
            }
        }
        let (actions, mut rx) = omochi_protocol::queue::channel(
            8,
            omochi_protocol::queue::DEFAULT_BYTES,
            omochi_protocol::MAX_FRAME_BYTES,
        );
        let instance = self.epoch.fetch_add(1, Ordering::Relaxed);
        let lifetime = self.stop.child_token();
        let scopes: BTreeMap<_, _> = snapshot
            .surface_ids()
            .into_iter()
            .map(|surface| {
                let scope = scoped::Scope {
                    route: route.clone(),
                    surface: surface.clone(),
                    instance,
                    generation: self.epoch.fetch_add(1, Ordering::Relaxed),
                    stop: lifetime.child_token(),
                };
                (surface, scope)
            })
            .collect();
        {
            let mut state = self.state.lock().await;
            if state.blocked_gateways.contains(node) {
                bail!("gateway authorization revoked")
            }
            for message in snapshot.messages() {
                let scope = scoped::surface_id(&message)
                    .and_then(|id| scopes.get(id))
                    .cloned();
                state.emit(message, scope);
            }
            state.channels.insert(
                route.clone(),
                Channel {
                    epoch,
                    service,
                    node_name: name.into(),
                    local,
                    snapshot: Arc::new(snapshot),
                    snapshot_charge,
                    actions,
                    connection,
                    instance,
                    lifetime,
                    scopes,
                },
            );
            let _ = self.changes.send(());
        }
        let read = async {
            while let Some(ServiceFrame::Message { message }) =
                read_frame::<_, ServiceFrame>(&mut reader).await?
            {
                let message = routing::prefix(&route, message)?;
                let mut state = self.state.lock().await;
                let channel = state
                    .channels
                    .get_mut(&route)
                    .filter(|c| c.epoch == epoch)
                    .context("superseded service")?;
                capacity::apply_retained(
                    &mut channel.snapshot,
                    &mut channel.snapshot_charge,
                    &message,
                )?;
                let surface = scoped::surface_id(&message).context("missing surface")?;
                let scope = if message.get("deleteSurface").is_some() {
                    if let Some(scope) = channel.scopes.remove(surface) {
                        scope.stop.cancel();
                    }
                    None
                } else if message.get("createSurface").is_some() {
                    let scope = scoped::Scope {
                        route: route.clone(),
                        surface: surface.into(),
                        instance: channel.instance,
                        generation: self.epoch.fetch_add(1, Ordering::Relaxed),
                        stop: channel.lifetime.child_token(),
                    };
                    if let Some(old) = channel.scopes.insert(surface.into(), scope.clone()) {
                        old.stop.cancel();
                    }
                    Some(scope)
                } else {
                    channel.scopes.get(surface).cloned()
                };
                state.emit(message, scope);
            }
            Ok::<_, anyhow::Error>(())
        };
        let write = async {
            while let Some(action) = rx.recv().await {
                let (action, _charge) = action.into_parts();
                if let Some((surface, subscription)) = action.scope {
                    if surface.is_cancelled() || subscription.is_cancelled() {
                        continue;
                    }
                    tokio::select! {
                        _ = surface.cancelled() => bail!("Surface expired during action write"),
                        _ = subscription.cancelled() => bail!("subscription ended during action write"),
                        result = tokio::time::timeout(Duration::from_secs(5), write_frame(&mut writer, &action.message)) => result??,
                    }
                } else {
                    tokio::time::timeout(
                        Duration::from_secs(5),
                        write_frame(&mut writer, &action.message),
                    )
                    .await??;
                }
            }
            Ok::<_, anyhow::Error>(())
        };
        tokio::select! { result = read => result, result = write => result }
    }
}

fn apply_snapshot(snapshot: &mut A2uiSnapshot, message: &Value) -> Result<()> {
    omochi_protocol::data_path::validate(message)?;
    snapshot.apply(message)?;
    capacity::snapshot_bytes(snapshot, false)?;
    Ok(())
}

#[cfg(all(test, feature = "browser"))]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use std::os::unix::fs::PermissionsExt;
    use tokio_util::codec::{Framed, LinesCodec};
    #[tokio::test]
    async fn unix_gateway_to_hub_preserves_browser_routes_and_action_isolation() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        omochi_local::LocalRuntime::prepare(Some(root.path().to_path_buf())).unwrap();
        let app = root.path().join("svc-app~");
        omochi_local::files::private_directory(&app).unwrap();
        std::fs::write(
            app.join("manifest.json"),
            r#"{"version":2,"serviceId":"app","name":"App","endpoint":{"kind":"unix","path":"a2ui.sock"}}"#,
        )
        .unwrap();
        let listener = tokio::net::UnixListener::bind(app.join("a2ui.sock")).unwrap();
        let (action_tx, action_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
        let sdk = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut frames = Framed::new(socket, LinesCodec::new());
            frames.send(json!({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"}}).to_string()).await.unwrap();
            let action: Value =
                serde_json::from_str(&frames.next().await.unwrap().unwrap()).unwrap();
            action_tx.send(action).unwrap();
            let _ = finish_rx.await;
        });
        let node = "a".repeat(64);
        let route = format!("{node}~app");
        let gateway = omochi_gateway::Gateway::new();
        let discovery = gateway.discover(root.path().to_owned());
        let socket = root.path().join("gateway.sock");
        let server = gateway
            .listen_local(
                (socket.clone()).parent().unwrap().to_path_buf(),
                node.clone(),
                "workstation".into(),
            )
            .await
            .unwrap();
        let hub = Hub::new();
        let attached = hub.attach_local(socket.parent().unwrap().to_path_buf());
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let subscription = hub.subscribe().await;
                if !subscription.snapshot.is_empty() {
                    assert_eq!(
                        subscription.snapshot[0]["createSurface"]["surfaceId"],
                        format!("{route}/main")
                    );
                    assert_eq!(hub.services().await[0]["node"]["id"], node);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let (address, http) = http::serve(
            http::HttpState {
                hub: hub.clone(),
                dashboards: dashboards::DashboardStore::open(root.path().join("state")).unwrap(),
                allow_origins: vec![],
                assets: None,
                presets: vec![],
            },
            "127.0.0.1:0".parse().unwrap(),
        )
        .await
        .unwrap();
        let (mut browser, _) = tokio_tungstenite::connect_async(format!("ws://{address}/ws"))
            .await
            .unwrap();
        let first = browser.next().await.unwrap().unwrap().into_text().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&first).unwrap()["createSurface"]["surfaceId"],
            format!("{route}/main")
        );
        browser.send(tokio_tungstenite::tungstenite::Message::Text(json!({"version":"v0.9","action":{"surfaceId":format!("{route}/main"),"name":"click"},"metadata":{"a2uiClientDataModel":{"surfaces":{format!("{route}/main"): {"value":"local"},format!("{node}~other/main"): {"value":"secret"}}}}}).to_string().into())).await.unwrap();
        let action = tokio::time::timeout(Duration::from_secs(3), action_rx)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(action["action"]["surfaceId"], "main");
        assert_eq!(
            action
                .pointer("/metadata/a2uiClientDataModel/surfaces")
                .unwrap(),
            &json!({"main":{"value":"local"}})
        );
        browser.close(None).await.unwrap();
        drop(browser);
        hub.shutdown();
        attached.await.unwrap();
        http.await.unwrap().unwrap();
        assert!(hub.services().await.is_empty());
        finish_tx.send(()).unwrap();
        sdk.await.unwrap();
        gateway.shutdown();
        server.await.unwrap().unwrap();
        discovery.await.unwrap().unwrap();
    }
}
