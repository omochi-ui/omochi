//! Hub aggregation independent of browser HTTP and gateway transport.
pub mod dashboards;
mod heartbeat;
pub mod http;
pub mod remote;
mod resource_http;
pub mod routing;
pub mod stdio;
use anyhow::{Context, Result, bail};
use omochi_protocol::{
    OpenStream, VERSION, read_frame,
    session::{Control, Service, ServiceFrame, valid_node_id, valid_service_id},
    write_frame,
};
use omochi_ui::A2uiSnapshot;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::UnixStream,
    sync::{Mutex, broadcast},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

type Reader = Pin<Box<dyn AsyncRead + Send>>;
type Writer = Pin<Box<dyn AsyncWrite + Send>>;
#[derive(Clone)]
pub enum Connection {
    Unix(PathBuf),
    Iroh(omochi_transport::EndpointConnection),
}
impl Connection {
    async fn open(&self, header: OpenStream) -> Result<(Reader, Writer)> {
        let (reader, mut writer): (Reader, Writer) = match self {
            Self::Unix(path) => {
                let socket = UnixStream::connect(path).await?;
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
struct Channel {
    epoch: u64,
    service: Service,
    node_name: String,
    local: bool,
    snapshot: A2uiSnapshot,
    actions: omochi_protocol::queue::Sender<Value>,
    connection: Connection,
}
struct State {
    channels: BTreeMap<String, Channel>,
    events: omochi_protocol::fanout::Sender,
}
#[derive(Clone)]
pub struct Hub {
    state: Arc<Mutex<State>>,
    changes: broadcast::Sender<()>,
    epoch: Arc<AtomicU64>,
    stop: CancellationToken,
    standalone: Option<stdio::Backend>,
    close_code: Arc<std::sync::atomic::AtomicU16>,
    browser_tasks: tokio_util::task::TaskTracker,
    diagnostics: remote::Diagnostics,
    peer_names: Arc<Mutex<BTreeMap<String, String>>>,
}
pub struct Subscription {
    pub snapshot: Vec<Value>,
    pub events: omochi_protocol::fanout::Receiver,
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
                events,
            })),
            changes,
            epoch: Arc::new(AtomicU64::new(1)),
            stop: CancellationToken::new(),
            standalone: None,
            close_code: Arc::new(std::sync::atomic::AtomicU16::new(1001)),
            browser_tasks: tokio_util::task::TaskTracker::new(),
            diagnostics: Arc::default(),
            peer_names: Arc::default(),
        }
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
            events: state.events.subscribe(),
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
        self.state.lock().await.channels.iter().map(|(route, channel)| {
            let mut view = json!({"id":route,"name":channel.service.name,"status":"connected","node":{"id":route.split_once('~').unwrap().0,"name":channel.node_name,"local":channel.local}});
            if let Some(icon) = &channel.service.icon { view["icon"] = icon.clone().into(); } view
        }).collect()
    }
    pub async fn action(&self, message: Value) -> Result<()> {
        if let Some(backend) = &self.standalone {
            return backend.action(message);
        }
        let (route, message) = routing::route_action(message)?;
        let bytes = serde_json::to_vec(&message)?.len();
        let state = self.state.lock().await;
        state
            .channels
            .get(&route)
            .context("service unavailable")?
            .actions
            .try_send(message, bytes)
            .map_err(|_| anyhow::anyhow!("service busy or disconnected"))
    }
    pub async fn resource(&self, route: &str, request: Value) -> Result<Value> {
        if let Some(backend) = &self.standalone {
            if route != "_" {
                bail!("unknown resource route");
            }
            return backend.resource(request).await;
        }
        routing::split_route(route)?;
        let (connection, service) = {
            let state = self.state.lock().await;
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
    pub fn attach_unix(&self, path: PathBuf) -> JoinHandle<()> {
        let hub = self.clone();
        tokio::spawn(async move {
            let mut delay = 100;
            loop {
                if hub.stop.is_cancelled() {
                    return;
                }
                let _ = hub.run(Connection::Unix(path.clone()), None, true).await;
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
                    let _ = state.events.send(Arc::new(
                        json!({"version":"v0.9","deleteSurface":{"surfaceId":surface}}),
                    ));
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
            Connection::Unix(_) => expected,
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
        let (pong, replies) = tokio::sync::watch::channel(0u64);
        let heartbeat = heartbeat::run(writer, replies, Duration::from_secs(10));
        tokio::pin!(heartbeat);
        let mut services: BTreeMap<String, (Service, CancellationToken, JoinHandle<()>)> =
            BTreeMap::new();
        let result = async {
            loop {
                // Reading is never cancelled to publish a service update: the only alternative ends this session.
                let frame = tokio::select! {
                    _ = stop.cancelled() => return Ok(()),
                    result = &mut heartbeat => return result,
                    frame = read_frame::<_, Control>(&mut reader) => frame?,
                };
                let Some(frame) = frame else { bail!("gateway disconnected") };
                match frame {
                    Control::Services { services: listed } => {
                        if listed.len() > 256 { bail!("too many services") }
                        let mut next = BTreeMap::new();
                        for spec in listed {
                            if !valid_service_id(&spec.id) || spec.generation == 0 || next.insert(spec.id.clone(), spec).is_some() { bail!("invalid service list") }
                        }
                        let removed: Vec<_> = services.iter().filter(|(id, (spec, _, _))| next.get(*id) != Some(spec)).map(|(id, _)| id.clone()).collect();
                        for id in removed { let (_, stop, task) = services.remove(&id).unwrap(); stop.cancel(); let _ = task.await; self.remove(&format!("{node_id}~{id}"), epoch).await; }
                        for (id, spec) in next {
                            if services.contains_key(&id) { continue }
                            let stop = session_stop.child_token(); let token = stop.clone(); let hub = self.clone(); let conn = connection.clone(); let node = node_id.clone(); let node_name = name.clone(); let service = spec.clone();
                            let task = tokio::spawn(async move {
                                loop {
                                    tokio::select! { _ = token.cancelled() => break, _ = hub.service_stream(conn.clone(), epoch, &node, &node_name, local, service.clone()) => {} }
                                    hub.remove(&format!("{node}~{}", service.id), epoch).await;
                                    tokio::select! { _ = token.cancelled() => break, _ = tokio::time::sleep(Duration::from_millis(250)) => {} }
                                }
                            });
                            services.insert(id, (spec, stop, task));
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
        result
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
        if !matches!(
            read_frame::<_, ServiceFrame>(&mut reader).await?,
            Some(ServiceFrame::SnapshotStart)
        ) {
            bail!("missing snapshot start")
        }
        loop {
            match read_frame::<_, ServiceFrame>(&mut reader).await? {
                Some(ServiceFrame::Message { message }) => {
                    apply_snapshot(&mut snapshot, &routing::prefix(&route, message)?)?
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
        {
            let mut state = self.state.lock().await;
            for message in snapshot.messages() {
                let _ = state.events.send(Arc::new(message));
            }
            state.channels.insert(
                route.clone(),
                Channel {
                    epoch,
                    service,
                    node_name: name.into(),
                    local,
                    snapshot,
                    actions,
                    connection,
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
                apply_snapshot(&mut channel.snapshot, &message)?;
                let _ = state.events.send(Arc::new(message));
            }
            Ok::<_, anyhow::Error>(())
        };
        let write = async {
            while let Some(action) = rx.recv().await {
                let (action, _charge) = action.into_parts();
                tokio::time::timeout(Duration::from_secs(5), write_frame(&mut writer, &action))
                    .await??;
            }
            Ok::<_, anyhow::Error>(())
        };
        tokio::select! { result = read => result, result = write => result }
    }
}

fn apply_snapshot(snapshot: &mut A2uiSnapshot, message: &Value) -> Result<()> {
    omochi_protocol::data_path::validate(message)?;
    snapshot.apply(message)?;
    if serde_json::to_vec(&snapshot.messages())?.len() > 16 * 1024 * 1024 {
        bail!("snapshot exceeds limit")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use std::os::unix::fs::PermissionsExt;
    use tokio_util::codec::{Framed, LinesCodec};
    #[tokio::test]
    async fn unix_gateway_to_hub_preserves_browser_routes_and_action_isolation() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let app = root.path().join("app");
        std::fs::create_dir(&app).unwrap();
        std::fs::write(
            app.join("manifest.json"),
            r#"{"name":"App","socket":"a2ui.sock"}"#,
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
            .listen_unix(socket.clone(), node.clone(), "workstation".into())
            .unwrap();
        let hub = Hub::new();
        let attached = hub.attach_unix(socket);
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
