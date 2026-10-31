//! One shared SDK connection per discovered service, independent of hub transports.
pub mod remote;
pub mod wire;
use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use omochi_local_discovery::{DiscoveredService, scan};
use omochi_protocol::{MAX_FRAME_BYTES, session::Service};
use omochi_ui::A2uiSnapshot;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    net::UnixStream,
    sync::{Mutex, broadcast, mpsc, oneshot},
    task::JoinHandle,
};
use tokio_util::{
    codec::{Framed, LinesCodec},
    sync::CancellationToken,
};

const MAX_SERVICES: usize = 256;
const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);
struct StreamState {
    snapshot: A2uiSnapshot,
    events: omochi_protocol::fanout::Sender,
    resources: bool,
}
struct Running {
    service: Service,
    input: omochi_protocol::queue::Sender<Command>,
    state: Mutex<StreamState>,
}
enum Command {
    Action(Value),
    Resource(Value, oneshot::Sender<Value>),
}
#[derive(Clone)]
pub struct Gateway {
    services: Arc<Mutex<BTreeMap<String, Arc<Running>>>>,
    changes: broadcast::Sender<()>,
    next_generation: Arc<AtomicU64>,
    stop: CancellationToken,
}
pub struct Subscription {
    pub snapshot: Vec<Value>,
    pub events: omochi_protocol::fanout::Receiver,
}
impl Default for Gateway {
    fn default() -> Self {
        Self::new()
    }
}
impl Gateway {
    pub fn new() -> Self {
        let (changes, _) = broadcast::channel(16);
        Self {
            services: Arc::default(),
            changes,
            next_generation: Arc::new(AtomicU64::new(1)),
            stop: CancellationToken::new(),
        }
    }
    pub fn shutdown(&self) {
        self.stop.cancel();
    }
    pub fn changes(&self) -> broadcast::Receiver<()> {
        self.changes.subscribe()
    }
    pub async fn list(&self) -> Vec<Service> {
        self.services
            .lock()
            .await
            .values()
            .map(|r| r.service.clone())
            .collect()
    }
    async fn running(&self, id: &str, generation: u64) -> Result<Arc<Running>> {
        self.services
            .lock()
            .await
            .get(id)
            .filter(|r| r.service.generation == generation)
            .cloned()
            .context("service disconnected or replaced")
    }
    pub async fn subscribe(&self, id: &str, generation: u64) -> Result<Subscription> {
        let service = self.running(id, generation).await?;
        let state = service.state.lock().await;
        // Snapshot and subscription are taken under the same publication lock.
        Ok(Subscription {
            snapshot: state.snapshot.messages(),
            events: state.events.subscribe(),
        })
    }
    pub async fn action(&self, id: &str, generation: u64, message: Value) -> Result<()> {
        if message.get("version").and_then(Value::as_str) != Some("v0.9")
            || !["action", "error"].iter().any(|key| {
                message
                    .get(key)
                    .and_then(|v| v.get("surfaceId"))
                    .and_then(Value::as_str)
                    .is_some()
            })
        {
            bail!("invalid client message")
        }
        let bytes = serde_json::to_vec(&message)?.len();
        if bytes > MAX_FRAME_BYTES {
            bail!("action too large")
        }
        self.running(id, generation)
            .await?
            .input
            .try_send(Command::Action(message), bytes)
            .map_err(|_| anyhow::anyhow!("service busy or disconnected"))
    }
    pub async fn resource(&self, id: &str, generation: u64, mut request: Value) -> Result<Value> {
        let op = request.get("op").and_then(Value::as_str);
        let valid_id = request
            .get("resource")
            .and_then(Value::as_str)
            .is_some_and(|id| {
                id.len() == 32
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            });
        let valid_read = request
            .get("offset")
            .and_then(Value::as_u64)
            .is_some_and(|v| v <= 9_007_199_254_740_991)
            && request
                .get("length")
                .and_then(Value::as_u64)
                .is_some_and(|v| v > 0 && v <= 24 * 1024);
        if !valid_id || !(op == Some("stat") || (op == Some("read") && valid_read)) {
            bail!("invalid resource request")
        }
        let running = self.running(id, generation).await?;
        if !running.state.lock().await.resources {
            bail!("resources unsupported")
        }
        request["omochi"] = "resources/1".into();
        request["kind"] = "request".into();
        request["id"] = format!("r{}", REQUEST_ID.fetch_add(1, Ordering::Relaxed)).into();
        let bytes = serde_json::to_vec(&request)?.len();
        let (tx, rx) = oneshot::channel();
        running
            .input
            .try_send(Command::Resource(request, tx), bytes)
            .map_err(|_| anyhow::anyhow!("service busy"))?;
        Ok(tokio::time::timeout(Duration::from_secs(15), rx).await??)
    }
    pub fn discover(&self, root: PathBuf) -> JoinHandle<Result<()>> {
        let gateway = self.clone();
        tokio::spawn(async move {
            let mut tasks: BTreeMap<
                String,
                (DiscoveredService, CancellationToken, JoinHandle<()>),
            > = BTreeMap::new();
            let changed = Arc::new(tokio::sync::Notify::new());
            let signal = changed.clone();
            let watched_root = root.clone();
            let watcher = tokio::task::spawn_blocking(move || {
                omochi_local_discovery::watch(&watched_root, move || signal.notify_one())
            })
            .await?
            .ok();
            // Notify coalesces bursts into one pending scan instead of accumulating
            // unbounded event data. Polling repairs missed events or a deleted root.
            let period = if watcher.is_some() {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(250)
            };
            let mut tick = tokio::time::interval(period);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = gateway.stop.cancelled() => break,
                    _ = tick.tick() => {},
                    _ = changed.notified() => {},
                }
                let path = root.clone();
                let found = tokio::task::spawn_blocking(move || scan(&path)).await?;
                let Ok(found) = found else { continue };
                let removed: Vec<_> = tasks
                    .iter()
                    .filter(|(id, (previous, _, _))| found.get(*id) != Some(previous))
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in removed {
                    if let Some((_, stop, task)) = tasks.remove(&id) {
                        stop.cancel();
                        let _ = task.await;
                    }
                }
                for (id, service) in found.into_iter().take(MAX_SERVICES) {
                    if tasks.contains_key(&id) {
                        continue;
                    }
                    let stop = gateway.stop.child_token();
                    let child_stop = stop.clone();
                    let child = gateway.clone();
                    let spec = service.clone();
                    let task = tokio::spawn(async move {
                        child.connect_loop(spec, child_stop).await;
                    });
                    tasks.insert(id, (service, stop, task));
                }
            }
            drop(watcher);
            for (_, (_, stop, task)) in tasks {
                stop.cancel();
                let _ = task.await;
            }
            Ok(())
        })
    }
    async fn connect_loop(&self, spec: DiscoveredService, stop: CancellationToken) {
        let mut delay = 100;
        loop {
            let connection = tokio::select! { _ = stop.cancelled() => return, result = UnixStream::connect(&spec.socket_path) => result };
            if let Ok(socket) = connection {
                delay = 100;
                let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
                let (input, receiver) = omochi_protocol::queue::channel(
                    8,
                    omochi_protocol::queue::DEFAULT_BYTES,
                    MAX_FRAME_BYTES,
                );
                let (events, _) =
                    omochi_protocol::fanout::channel(8, omochi_protocol::fanout::DEFAULT_BYTES);
                let running = Arc::new(Running {
                    service: Service {
                        id: spec.id.clone(),
                        name: spec.manifest.name.clone(),
                        generation,
                        icon: spec.manifest.icon.clone(),
                    },
                    input,
                    state: Mutex::new(StreamState {
                        snapshot: A2uiSnapshot::new(),
                        events,
                        resources: false,
                    }),
                });
                self.services
                    .lock()
                    .await
                    .insert(spec.id.clone(), running.clone());
                let _ = self.changes.send(());
                tokio::select! { _ = stop.cancelled() => {}, _ = serve_sdk(socket, running, receiver) => {} }
                self.services.lock().await.remove(&spec.id);
                let _ = self.changes.send(());
            }
            tokio::select! { _ = stop.cancelled() => return, _ = tokio::time::sleep(Duration::from_millis(delay)) => {} }
            delay = (delay * 2).min(5000);
        }
    }
}
async fn serve_sdk(
    socket: UnixStream,
    running: Arc<Running>,
    mut commands: mpsc::Receiver<omochi_protocol::queue::Entry<Command>>,
) -> Result<()> {
    let mut frames = Framed::new(socket, LinesCodec::new_with_max_length(MAX_FRAME_BYTES));
    let mut pending: BTreeMap<String, oneshot::Sender<Value>> = BTreeMap::new();
    loop {
        tokio::select! {
            line = frames.next() => {
                let Some(line) = line else { return Ok(()) }; let line = line?;
                if line.trim().is_empty() { continue }
                let value: Value = serde_json::from_str(&line)?;
                if value.get("omochi").and_then(Value::as_str) == Some("resources/1") {
                    if value.get("kind").and_then(Value::as_str) == Some("hello") { running.state.lock().await.resources = true; }
                    if value.get("kind").and_then(Value::as_str) == Some("response")
                        && let Some(id) = value.get("id").and_then(Value::as_str)
                        && let Some(reply) = pending.remove(id) { let _ = reply.send(value); }
                    continue
                }
                if value.get("version").and_then(Value::as_str) != Some("v0.9") { bail!("invalid A2UI version") }
                let mut state = running.state.lock().await;
                omochi_protocol::data_path::validate(&value)?;
                validate_service_frame(&value)?;
                state.snapshot.apply(&value)?;
                let replay = state.snapshot.messages();
                if serde_json::to_vec(&replay)?.len() > MAX_SNAPSHOT_BYTES { bail!("snapshot exceeds limit") }
                // Small patches can merge into a replay message larger than a wire frame.
                // Retract the service before publishing state that cannot be replayed.
                for message in &replay { validate_service_frame(message)?; }
                let _ = state.events.send(Arc::new(value));
            },
            command = commands.recv() => {
                let Some(command) = command else { return Ok(()) };
                let (command, _charge) = command.into_parts();
                pending.retain(|_, sender| !sender.is_closed());
                let message = match command {
                    Command::Action(value) => value,
                    Command::Resource(value, reply) => {
                        if pending.len() >= 8 { continue }
                        pending.insert(value["id"].as_str().unwrap().into(), reply); value
                    }
                };
                tokio::time::timeout(Duration::from_secs(5), frames.send(serde_json::to_string(&message)?)).await??;
            }
        }
    }
}

fn validate_service_frame(message: &Value) -> Result<()> {
    // ServiceFrame::Message wraps the original SDK message with these fixed bytes.
    const ENVELOPE: usize = b"{\"kind\":\"message\",\"message\":}".len();
    if serde_json::to_vec(message)?.len() + ENVELOPE > MAX_FRAME_BYTES {
        bail!("service message exceeds framed transport limit")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;
    #[test]
    fn merged_snapshot_must_fit_each_wire_frame() {
        let mut snapshot = A2uiSnapshot::new();
        snapshot
            .apply(
                &json!({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"}}),
            )
            .unwrap();
        for key in ["a", "b"] {
            let patch = json!({"version":"v0.9","updateDataModel":{"surfaceId":"main","path":format!("/{key}"),"value":"x".repeat(MAX_FRAME_BYTES / 2)}});
            validate_service_frame(&patch).unwrap();
            snapshot.apply(&patch).unwrap();
        }
        assert!(
            snapshot
                .messages()
                .iter()
                .any(|message| validate_service_frame(message).is_err())
        );
        let message = json!({"version":"v0.9"});
        let wrapped = omochi_protocol::session::ServiceFrame::Message {
            message: message.clone(),
        };
        assert_eq!(
            serde_json::to_vec(&wrapped).unwrap().len(),
            serde_json::to_vec(&message).unwrap().len()
                + b"{\"kind\":\"message\",\"message\":}".len()
        );
    }
    #[tokio::test]
    async fn shared_sdk_stream_replays_snapshot_routes_actions_and_retracts() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let directory = root.path().join("app");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(
            directory.join("manifest.json"),
            r#"{"name":"App","socket":"a2ui.sock"}"#,
        )
        .unwrap();
        let listener = UnixListener::bind(directory.join("a2ui.sock")).unwrap();
        let (ready_tx, ready_rx) = oneshot::channel();
        let (finish_tx, finish_rx) = oneshot::channel();
        let sdk = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut frames = Framed::new(socket, LinesCodec::new());
            frames.send(json!({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"}}).to_string()).await.unwrap();
            frames
                .send(json!({"omochi":"resources/1","kind":"hello"}).to_string())
                .await
                .unwrap();
            ready_tx.send(()).unwrap();
            let action: Value =
                serde_json::from_str(&frames.next().await.unwrap().unwrap()).unwrap();
            assert_eq!(action["action"]["surfaceId"], "main");
            let request: Value =
                serde_json::from_str(&frames.next().await.unwrap().unwrap()).unwrap();
            frames.send(json!({"omochi":"resources/1","kind":"response","id":request["id"],"status":200,"size":3}).to_string()).await.unwrap();
            let _ = finish_rx.await;
        });
        let gateway = Gateway::new();
        let task = gateway.discover(root.path().to_owned());
        ready_rx.await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(service) = gateway.list().await.first().cloned() {
                    let subscription = gateway.subscribe("app", service.generation).await.unwrap();
                    if !subscription.snapshot.is_empty() {
                        assert_eq!(subscription.snapshot[0]["createSurface"]["surfaceId"], "main");
                        let second = gateway.subscribe("app", service.generation).await.unwrap();
                        assert_eq!(second.snapshot, subscription.snapshot);
                        gateway.action("app", service.generation, json!({"version":"v0.9","action":{"surfaceId":"main","name":"click"}})).await.unwrap();
                        let result = gateway.resource("app", service.generation, json!({"op":"stat","resource":"a".repeat(32)})).await.unwrap();
                        assert_eq!(result["size"], 3);
                        finish_tx.send(()).unwrap(); sdk.await.unwrap();
                        gateway.shutdown(); task.await.unwrap().unwrap();
                        assert!(gateway.list().await.is_empty());
                        assert!(gateway.action("app", service.generation, json!({"version":"v0.9","action":{"surfaceId":"main"}})).await.is_err());
                        break
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.unwrap();
    }
}
