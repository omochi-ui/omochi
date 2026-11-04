//! One shared SDK connection per discovered service, independent of hub transports.
mod dispatch;
pub mod remote;
#[cfg(test)]
mod stream_capacity_tests;
pub mod wire;
use anyhow::{Context, Result, bail};
use dispatch::CommandOrigin;
use futures_util::{SinkExt, StreamExt};
use omochi_local_discovery::{DiscoveredService, scan};
use omochi_protocol::{
    MAX_FRAME_BYTES,
    session::{Control, Service},
};
use omochi_ui::A2uiSnapshot;
use serde_json::{Value, json};
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
type DiscoveryTasks = BTreeMap<String, (DiscoveredService, CancellationToken, JoinHandle<()>)>;

fn roster_entry_budget(spec: &DiscoveredService) -> usize {
    // Use the wire serializer for escaping and UTF-8, reserving every possible
    // generation digit so reconnects cannot outgrow the discovery reservation.
    serde_json::to_vec(&Service {
        id: spec.id.clone(),
        name: spec.manifest.name.clone(),
        generation: u64::MAX,
        icon: spec.manifest.icon.clone(),
    })
    .expect("service metadata is serializable")
    .len()
        + 1 // Reserve a comma even for the first entry.
}
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
    Action(Value, CommandOrigin),
    Resource(Value, oneshot::Sender<Value>, CommandOrigin),
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
        self.action_from(id, generation, message, CommandOrigin::Local)
            .await
    }
    async fn action_from(
        &self,
        id: &str,
        generation: u64,
        message: Value,
        origin: CommandOrigin,
    ) -> Result<()> {
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
            .try_send(Command::Action(message, origin), bytes)
            .map_err(|_| anyhow::anyhow!("service busy or disconnected"))
    }
    pub async fn resource(&self, id: &str, generation: u64, request: Value) -> Result<Value> {
        self.resource_from(id, generation, request, CommandOrigin::Local)
            .await
    }
    async fn resource_from(
        &self,
        id: &str,
        generation: u64,
        mut request: Value,
        origin: CommandOrigin,
    ) -> Result<Value> {
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
        request["omochi"] = "resources/1".into();
        request["kind"] = "request".into();
        request["id"] = format!("r{}", REQUEST_ID.fetch_add(1, Ordering::Relaxed)).into();
        let bytes = serde_json::to_vec(&request)?.len();
        let (tx, rx) = oneshot::channel();
        running
            .input
            .try_send(Command::Resource(request, tx, origin), bytes)
            .map_err(|_| anyhow::anyhow!("service busy"))?;
        Ok(tokio::time::timeout(Duration::from_secs(15), rx).await??)
    }
    pub fn discover(&self, root: PathBuf) -> JoinHandle<Result<()>> {
        let gateway = self.clone();
        tokio::spawn(async move {
            let mut tasks = DiscoveryTasks::new();
            let changed = Arc::new(tokio::sync::Notify::new());
            let signal = changed.clone();
            let watched_root = root.clone();
            let watcher = omochi_transport::blocking::run(move || {
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
                let found = omochi_transport::blocking::run(move || scan(&path)).await?;
                let Ok(found) = found else { continue };
                gateway.reconcile_discovery(&mut tasks, found).await;
            }
            drop(watcher);
            for (_, (_, stop, task)) in tasks {
                stop.cancel();
                let _ = task.await;
            }
            Ok(())
        })
    }
    async fn reconcile_discovery(
        &self,
        tasks: &mut DiscoveryTasks,
        found: BTreeMap<String, DiscoveredService>,
    ) {
        // Protect unchanged services from oversized additions or metadata updates.
        // Changed existing IDs still have priority over newcomers when they fit.
        let mut candidates: Vec<_> = found.into_iter().collect();
        candidates.sort_by_key(|(id, service)| match tasks.get(id) {
            Some((previous, _, _)) if previous == service => 0,
            Some(_) => 1,
            None => 2,
        });
        let mut available = MAX_FRAME_BYTES
            - serde_json::to_vec(&Control::Services { services: vec![] })
                .expect("service roster is serializable")
                .len();
        let mut found = BTreeMap::new();
        for (id, service) in candidates {
            let bytes = roster_entry_budget(&service);
            if found.len() < MAX_SERVICES && bytes <= available {
                available -= bytes;
                found.insert(id, service);
            }
        }
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
        for (id, service) in found {
            if tasks.contains_key(&id) {
                continue;
            }
            let stop = self.stop.child_token();
            let child_stop = stop.clone();
            let child = self.clone();
            let spec = service.clone();
            let task = tokio::spawn(async move {
                child.connect_loop(spec, child_stop).await;
            });
            tasks.insert(id, (service, stop, task));
        }
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
async fn serve_sdk<S>(
    socket: S,
    running: Arc<Running>,
    mut commands: mpsc::Receiver<omochi_protocol::queue::Entry<Command>>,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
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
                let (message, reply, origin) = match command {
                    Command::Action(value, origin) => (value, None, origin),
                    Command::Resource(value, reply, origin) => {
                        if reply.is_closed() || pending.len() >= 8 { continue }
                        (value, Some(reply), origin)
                    }
                };
                let Some(_dispatch) = origin.enter().await else { continue };
                if let Some(reply) = reply {
                    if reply.is_closed() { continue }
                    if !running.state.lock().await.resources {
                        let _ = reply.send(json!({"omochi":"resources/1","kind":"response","id":message["id"],"status":501}));
                        continue;
                    }
                    pending.insert(message["id"].as_str().unwrap().into(), reply);
                }
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
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;

    #[test]
    fn roster_budget_covers_json_escaping_unicode_and_generation_growth() {
        let spec = DiscoveredService {
            id: "app".into(),
            manifest: omochi_local_discovery::Manifest {
                name: "\0\n\"\\🍡日本語".into(),
                socket: "a2ui.sock".into(),
                icon: Some("🍡\"\\".into()),
            },
            socket_path: PathBuf::new(),
        };
        let envelope = serde_json::to_vec(&Control::Services { services: vec![] })
            .unwrap()
            .len();
        for generation in [1, 9, 10, 99, 100, u64::MAX] {
            let service = Service {
                id: spec.id.clone(),
                name: spec.manifest.name.clone(),
                generation,
                icon: spec.manifest.icon.clone(),
            };
            for count in [1, MAX_SERVICES] {
                let bytes = serde_json::to_vec(&Control::Services {
                    services: vec![service.clone(); count],
                })
                .unwrap()
                .len();
                let reserved = envelope + count * roster_entry_budget(&spec);
                assert!(bytes < reserved);
                if generation == u64::MAX {
                    assert_eq!(bytes + 1, reserved);
                }
            }
        }
    }

    #[tokio::test]
    async fn large_valid_manifests_keep_control_alive_and_release_roster_capacity() {
        use omochi_protocol::{OpenStream, VERSION, read_frame, write_frame};
        async fn connected(gateway: &Gateway, tasks: &DiscoveryTasks) -> Vec<Service> {
            let mut changes = gateway.changes();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let services = gateway.list().await;
                    if services.len() == tasks.len()
                        && services.iter().all(|service| {
                            let spec = &tasks[&service.id].0;
                            service.name == spec.manifest.name && service.icon == spec.manifest.icon
                        })
                    {
                        return services;
                    }
                    let _ = changes.recv().await;
                }
            })
            .await
            .unwrap()
        }
        async fn roster_and_ping(client: &mut tokio::io::DuplexStream, expected: &[Service]) {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let frame = read_frame::<_, Control>(client).await.unwrap().unwrap();
                    assert!(serde_json::to_vec(&frame).unwrap().len() <= MAX_FRAME_BYTES);
                    if let Control::Services { services } = frame
                        && services == expected
                    {
                        break;
                    }
                }
                write_frame(client, &Control::Ping { id: 42 })
                    .await
                    .unwrap();
                loop {
                    match read_frame::<_, Control>(client).await.unwrap().unwrap() {
                        Control::Pong { id: 42 } => break,
                        Control::Services { services } => assert_eq!(services, expected),
                        frame => panic!("unexpected frame: {frame:?}"),
                    }
                }
            })
            .await
            .unwrap();
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let icon = format!("data:image/png;base64,{}", "A".repeat(65_499));
        let mut sdk_tasks = Vec::new();
        for index in 0..65 {
            let id = format!("z{index:03}");
            let directory = root.path().join(&id);
            std::fs::create_dir(&directory).unwrap();
            std::fs::write(
                directory.join("manifest.json"),
                json!({"name":id, "socket":"a2ui.sock", "icon":icon}).to_string(),
            )
            .unwrap();
            let listener = UnixListener::bind(directory.join("a2ui.sock")).unwrap();
            sdk_tasks.push(tokio::spawn(async move {
                let mut sockets = Vec::new();
                loop {
                    sockets.push(listener.accept().await.unwrap().0);
                }
            }));
        }
        let mut found = scan(root.path()).unwrap();
        assert_eq!(found.len(), 65);
        let gateway = Gateway::new();
        // Exercise the 19-to-20 digit generation transition during admission.
        gateway
            .next_generation
            .store(9_999_999_999_999_999_990, Ordering::Relaxed);
        let mut tasks = DiscoveryTasks::new();
        gateway.reconcile_discovery(&mut tasks, found.clone()).await;
        assert_eq!(tasks.len(), 63);
        let original = connected(&gateway, &tasks).await;
        let (mut client, server) = tokio::io::duplex(64 * 1024);
        let child = gateway.clone();
        let control = tokio::spawn(async move {
            let (reader, writer) = tokio::io::split(server);
            child
                .serve_stream(reader, writer, &"a".repeat(64), "test")
                .await
        });
        write_frame(&mut client, &OpenStream::Control { protocol: VERSION })
            .await
            .unwrap();
        roster_and_ping(&mut client, &original).await;

        // This valid metadata update cannot fit; only that service loses its slot.
        let manifest = &mut found.get_mut("z000").unwrap().manifest;
        manifest.name = "\0\"\\🍡".repeat(4500);
        let bytes = serde_json::to_vec(manifest).unwrap();
        assert!(bytes.len() < 128 * 1024);
        std::fs::write(root.path().join("z000/manifest.json"), bytes).unwrap();
        found = scan(root.path()).unwrap();
        assert_eq!(found.len(), 65);
        gateway.reconcile_discovery(&mut tasks, found.clone()).await;
        assert!(!tasks.contains_key("z000"));
        assert!(tasks.contains_key("z063"));
        let updated = connected(&gateway, &tasks).await;
        for service in original.iter().filter(|service| service.id != "z000") {
            assert!(updated.contains(service));
        }
        roster_and_ping(&mut client, &updated).await;

        // A smaller replacement remains pending until an existing slot is freed.
        found.get_mut("z000").unwrap().manifest.name = "Updated".into();
        gateway.reconcile_discovery(&mut tasks, found.clone()).await;
        assert!(!tasks.contains_key("z000"));
        found.remove("z001");
        gateway.reconcile_discovery(&mut tasks, found.clone()).await;
        assert!(tasks.contains_key("z000"));
        let replaced = connected(&gateway, &tasks).await;
        roster_and_ping(&mut client, &replaced).await;

        // Shrinking metadata also releases bytes and admits the last waiting ID.
        found.get_mut("z002").unwrap().manifest.icon = None;
        gateway.reconcile_discovery(&mut tasks, found).await;
        assert_eq!(tasks.len(), 64);
        assert!(tasks.contains_key("z064"));
        let shrunk = connected(&gateway, &tasks).await;
        roster_and_ping(&mut client, &shrunk).await;
        gateway.shutdown();
        control.await.unwrap().unwrap();
        for (_, (_, _, task)) in tasks {
            task.await.unwrap();
        }
        for task in sdk_tasks {
            task.abort();
            let _ = task.await;
        }
    }

    #[tokio::test]
    async fn discovery_caps_total_services_preserves_slots_and_admits_after_removal() {
        async fn connected(gateway: &Gateway) -> BTreeMap<String, Service> {
            let mut changes = gateway.changes();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let services = gateway.list().await;
                    assert!(services.len() <= MAX_SERVICES);
                    if services.len() == MAX_SERVICES {
                        return services.into_iter().map(|s| (s.id.clone(), s)).collect();
                    }
                    let _ = changes.recv().await;
                }
            })
            .await
            .unwrap()
        }

        let root = tempfile::tempdir().unwrap();
        let socket_path = root.path().join("sdk.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        // Keep every accepted SDK connection alive until gateway shutdown.
        let sdk = tokio::spawn(async move {
            let mut sockets = Vec::new();
            loop {
                sockets.push(listener.accept().await.unwrap().0);
            }
        });
        let service = |id: String| DiscoveredService {
            manifest: omochi_local_discovery::Manifest {
                name: id.clone(),
                socket: "sdk.sock".into(),
                icon: None,
            },
            id,
            socket_path: socket_path.clone(),
        };
        let mut found: BTreeMap<_, _> = (0..MAX_SERVICES)
            .map(|i| {
                let id = format!("z{i:03}");
                (id.clone(), service(id))
            })
            .collect();
        let gateway = Gateway::new();
        let mut tasks = DiscoveryTasks::new();
        gateway.reconcile_discovery(&mut tasks, found.clone()).await;
        let original = connected(&gateway).await;

        // An earlier-sorting newcomer used to raise the total to 257 because the
        // scan's first 256 entries were added without counting retained tasks.
        found.insert("a000".into(), service("a000".into()));
        gateway.reconcile_discovery(&mut tasks, found.clone()).await;
        // Reconciliation completion is the barrier proving no new task was admitted.
        assert_eq!(tasks.len(), MAX_SERVICES);
        assert!(!tasks.contains_key("a000"));
        for current in gateway.list().await {
            assert_eq!(current.generation, original[&current.id].generation);
        }

        // A changed existing manifest must reconnect in its reserved slot, even
        // while an earlier-sorting newcomer waits for capacity.
        found.get_mut("z255").unwrap().manifest.name = "Updated".into();
        gateway.reconcile_discovery(&mut tasks, found.clone()).await;
        assert_eq!(tasks.len(), MAX_SERVICES);
        assert!(!tasks.contains_key("a000"));
        let updated = connected(&gateway).await;
        assert_eq!(updated["z255"].name, "Updated");
        assert_ne!(updated["z255"].generation, original["z255"].generation);
        for (id, current) in &updated {
            if id != "z255" {
                assert_eq!(current.generation, original[id].generation);
            }
        }

        found.remove("z000");
        gateway.reconcile_discovery(&mut tasks, found).await;
        assert_eq!(tasks.len(), MAX_SERVICES);
        assert!(tasks.contains_key("a000"));
        assert!(!tasks.contains_key("z000"));
        let replaced = connected(&gateway).await;
        assert!(replaced.contains_key("a000"));
        assert!(!replaced.contains_key("z000"));
        for (id, current) in &replaced {
            if id != "a000" {
                assert_eq!(current.generation, updated[id].generation);
            }
        }

        gateway.shutdown();
        for (_, (_, _, task)) in tasks {
            task.await.unwrap();
        }
        assert!(gateway.list().await.is_empty());
        sdk.abort();
        let _ = sdk.await;
    }

    #[tokio::test]
    async fn blocked_sdk_write_drains_before_barrier_and_revoked_queued_work_is_skipped() {
        use crate::dispatch::AuthorizationEpoch;
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

        // A tiny bounded socket deterministically blocks a partially written SDK frame.
        let (socket, mut sdk) = tokio::io::duplex(64);
        let (input, receiver) =
            omochi_protocol::queue::channel(8, 8 * 1024 * 1024, MAX_FRAME_BYTES);
        let (events, _) = omochi_protocol::fanout::channel(8, 1024);
        let running = Arc::new(Running {
            service: Service {
                id: "app".into(),
                name: "App".into(),
                generation: 1,
                icon: None,
            },
            input: input.clone(),
            state: Mutex::new(StreamState {
                snapshot: A2uiSnapshot::new(),
                events,
                resources: true,
            }),
        });
        let epoch = AuthorizationEpoch::new();
        let session = CancellationToken::new();
        let origin = CommandOrigin::Remote {
            epoch: epoch.clone(),
            session: session.clone(),
        };
        let task = tokio::spawn(serve_sdk(socket, running, receiver));
        let first = json!({"version":"v0.9","action":{"surfaceId":"main","name":"started","payload":"x".repeat(16384)}});
        input
            .try_send(Command::Action(first.clone(), origin.clone()), 17000)
            .unwrap();
        let mut first_byte = [0u8; 1];
        tokio::time::timeout(Duration::from_secs(1), sdk.read_exact(&mut first_byte))
            .await
            .unwrap()
            .unwrap();
        let cancelled = json!({"version":"v0.9","action":{"surfaceId":"main","name":"revoked"}});
        input
            .try_send(Command::Action(cancelled, origin.clone()), 100)
            .unwrap();
        let (reply, response) = oneshot::channel();
        input
            .try_send(
                Command::Resource(json!({"id":"r1","op":"stat"}), reply, origin),
                100,
            )
            .unwrap();
        let local = json!({"version":"v0.9","action":{"surfaceId":"main","name":"local"}});
        input
            .try_send(Command::Action(local.clone(), CommandOrigin::Local), 100)
            .unwrap();
        epoch.cancel();
        session.cancel();
        let draining = epoch.drain();
        tokio::pin!(draining);
        assert!(
            futures_util::poll!(&mut draining).is_pending(),
            "an already dispatched partial write must finish"
        );
        let mut reader = BufReader::new(sdk);
        let mut line = vec![first_byte[0]];
        tokio::time::timeout(Duration::from_secs(1), reader.read_until(b'\n', &mut line))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&line).unwrap(), first);
        tokio::time::timeout(Duration::from_secs(1), draining)
            .await
            .unwrap();
        line.clear();
        tokio::time::timeout(Duration::from_secs(1), reader.read_until(b'\n', &mut line))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&line).unwrap(),
            local,
            "Local work survives while queued Remote action/resource are skipped"
        );
        assert!(response.await.is_err());
        task.abort();
        let _ = task.await;
    }

    #[tokio::test]
    async fn abandoned_resource_reply_is_discarded_before_sdk_dispatch() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let (socket, sdk) = tokio::io::duplex(256);
        let (input, receiver) = omochi_protocol::queue::channel(8, 1024, 1024);
        let (events, _) = omochi_protocol::fanout::channel(8, 1024);
        let running = Arc::new(Running {
            service: Service {
                id: "app".into(),
                name: "App".into(),
                generation: 1,
                icon: None,
            },
            input: input.clone(),
            state: Mutex::new(StreamState {
                snapshot: A2uiSnapshot::new(),
                events,
                resources: true,
            }),
        });
        let (reply, response) = oneshot::channel();
        drop(response);
        input
            .try_send(
                Command::Resource(json!({"id":"r1"}), reply, CommandOrigin::Local),
                100,
            )
            .unwrap();
        input
            .try_send(
                Command::Action(json!({"marker":"local"}), CommandOrigin::Local),
                100,
            )
            .unwrap();
        let task = tokio::spawn(serve_sdk(socket, running, receiver));
        let mut reader = BufReader::new(sdk);
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(1), reader.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap(),
            json!({"marker":"local"})
        );
        task.abort();
        let _ = task.await;
    }
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
