use crate::resources::{ResourceRegistry, hello, is_resource};
use std::collections::{BTreeSet, VecDeque};

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
pub use omochi_local::LocalEndpoint;
use omochi_local::{LocalRuntime, LocalStream, ServicePublication};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, Notify, OwnedSemaphorePermit, Semaphore, mpsc, oneshot, watch};
use tokio::task::{JoinHandle, JoinSet};

use crate::snapshot::{A2uiSnapshot, SnapshotCursor};
use crate::transport::Transport;
use crate::{Error, Result};

/// Encoded UTF-8 bytes, including newlines, queued or being written per client.
const MAX_OUTBOUND_BYTES: usize = 16 * 1024 * 1024;

// Pending actions exclude the action currently executing and resource RPC frames.
const MAX_PENDING_MESSAGES: usize = 256;
const MAX_PENDING_BYTES: usize = 4 * 1024 * 1024;
const MAX_INBOUND_FRAME_BYTES: usize = 4 * 1024 * 1024;
const RECEIVE_OVERFLOW: &str = "pending action queue exceeded 256 messages or 4 MiB";

#[derive(Default)]
struct InboundState {
    entries: VecDeque<(Value, usize)>,
    bytes: usize,
    closed: bool,
    overflow: bool,
}

struct InboundQueue {
    state: std::sync::Mutex<InboundState>,
    ready: Notify,
    failed: watch::Sender<()>,
}

impl InboundQueue {
    fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(InboundState::default()),
            ready: Notify::new(),
            failed: watch::channel(()).0,
        }
    }

    fn push(&self, value: Value) -> bool {
        let bytes = serde_json::to_vec(&value)
            .expect("JSON value is serializable")
            .len();
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return false;
        }
        if state.entries.len() >= MAX_PENDING_MESSAGES || state.bytes + bytes > MAX_PENDING_BYTES {
            state.entries.clear();
            state.bytes = 0;
            state.closed = true;
            state.overflow = true;
            // Fail the whole stream: executing any pending action after losing one
            // would silently continue from an incomplete application history.
            self.failed.send_replace(());
            self.ready.notify_one();
            return false;
        }
        state.entries.push_back((value, bytes));
        state.bytes += bytes;
        self.ready.notify_one();
        true
    }

    async fn receive(&self) -> Result<Option<Value>> {
        loop {
            {
                let mut state = self.state.lock().unwrap();
                if state.overflow {
                    return Err(Error::Transport(RECEIVE_OVERFLOW.into()));
                }
                if let Some((value, bytes)) = state.entries.pop_front() {
                    state.bytes -= bytes;
                    return Ok(Some(value));
                }
                if state.closed {
                    return Ok(None);
                }
            }
            // There is only one receiver. notify_one retains a permit if a push
            // happens between releasing the state lock and awaiting this notification.
            self.ready.notified().await;
        }
    }

    fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.entries.clear();
        state.bytes = 0;
        state.closed = true;
        self.ready.notify_one();
    }
}

struct OutboundFrame {
    line: String,
    // Removing a frame from the queue must not release its budget before write_all.
    _permit: OwnedSemaphorePermit,
}

#[derive(Clone)]
struct ClientSender {
    queue: mpsc::UnboundedSender<OutboundFrame>,
    budget: Arc<Semaphore>,
    cancelled: watch::Sender<()>,
}

impl ClientSender {
    fn new() -> (
        Self,
        mpsc::UnboundedReceiver<OutboundFrame>,
        watch::Receiver<()>,
    ) {
        let (queue, receiver) = mpsc::unbounded_channel();
        let (cancelled, cancellation) = watch::channel(());
        (
            Self {
                queue,
                budget: Arc::new(Semaphore::new(MAX_OUTBOUND_BYTES)),
                cancelled,
            },
            receiver,
            cancellation,
        )
    }

    fn reserve(&self, bytes: usize) -> Option<OwnedSemaphorePermit> {
        if self.queue.is_closed() {
            return None;
        }
        if bytes <= MAX_OUTBOUND_BYTES {
            if let Ok(permit) = self.budget.clone().try_acquire_many_owned(bytes as u32) {
                return Some(permit);
            }
        }
        // Dropping a sender alone cannot interrupt a blocked socket write. Signal
        // the connection directly and permanently reject any subsequent frames.
        self.budget.close();
        self.cancelled.send_replace(());
        None
    }

    fn send(&self, line: &str) -> bool {
        let Some(permit) = self.reserve(line.len()) else {
            return false;
        };
        self.queue
            .send(OutboundFrame {
                line: line.to_owned(),
                _permit: permit,
            })
            .is_ok()
    }
}

/// Resolve the version 2 runtime directory.
pub fn omochi_runtime_root() -> PathBuf {
    omochi_local::default_runtime_root().expect("could not resolve per-user runtime directory")
}
pub fn prepare_omochi_runtime_root() -> Result<PathBuf> {
    Ok(LocalRuntime::prepare(None)?.root().to_path_buf())
}
pub fn validate_service_id(id: &str) -> Result<()> {
    Ok(omochi_local::validate_service_id(id)?)
}

#[derive(Debug, Clone)]
pub struct LocalServerOptions {
    pub service_id: String,
    /// Human-readable label shown by a hub. Defaults to the service id.
    pub name: Option<String>,
    /// An emoji, or a `data:image/...;base64,` URL a browser can render directly.
    pub icon: Option<String>,
    /// Complete runtime root; defaults to OMOCHI_RUNTIME_DIR or the platform v2 directory.
    pub runtime_dir: Option<PathBuf>,
    pub capabilities: BTreeSet<String>,
}

impl LocalServerOptions {
    pub fn new(service_id: impl Into<String>) -> Self {
        Self {
            service_id: service_id.into(),
            name: None,
            icon: None,
            runtime_dir: None,
            capabilities: BTreeSet::new(),
        }
    }
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }
    pub fn with_runtime_dir(mut self, runtime_dir: impl Into<PathBuf>) -> Self {
        self.runtime_dir = Some(runtime_dir.into());
        self
    }
    pub fn with_capabilities(
        mut self,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.capabilities = capabilities.into_iter().map(Into::into).collect();
        self
    }
}

struct Shared {
    resources: std::sync::RwLock<Option<ResourceRegistry>>,
    publication: Mutex<Publication>,
}

#[derive(Default)]
struct Publication {
    snapshot: A2uiSnapshot,
    clients: Vec<ClientSender>,
}

/// Publishes one application through user-private local IPC for discovery.
///
/// The shared native module writes `<runtime_dir>/svc-<service_id>~/manifest.json`
/// and serves a Unix domain socket or Windows named pipe. Access is restricted
/// to the current OS user through directory permissions or Windows ACLs.
///
/// One application is fanned out to every connected renderer, and their inbound messages
/// are merged into a single stream for the Runtime.
pub struct LocalServerTransport {
    shared: Arc<Shared>,
    inbound: Arc<InboundQueue>,
    accept: Option<JoinHandle<()>>,
    shutdown: Option<oneshot::Sender<()>>,
    finished: watch::Receiver<bool>,
    capabilities: BTreeSet<String>,
    directory: PathBuf,
    endpoint: LocalEndpoint,
    closed: bool,
    #[cfg(all(test, unix))]
    service_publication: std::sync::Weak<ServicePublication>,
}

impl LocalServerTransport {
    pub async fn bind(mut options: LocalServerOptions) -> Result<Self> {
        validate_service_id(&options.service_id)?;
        let root = LocalRuntime::prepare(options.runtime_dir.take())?;
        let publication = root
            .publish(
                &options.service_id,
                options.name.as_deref().unwrap_or(&options.service_id),
                options.icon.clone(),
            )
            .await?;
        let directory = publication.directory().to_path_buf();
        let endpoint = publication.endpoint().clone();
        let finished = publication.finished();
        let listener = Arc::new(publication);
        #[cfg(all(test, unix))]
        let service_publication = Arc::downgrade(&listener);
        let shared = Arc::new(Shared {
            resources: std::sync::RwLock::new(None),
            publication: Mutex::new(Publication::default()),
        });
        let inbound = Arc::new(InboundQueue::new());
        let sender = Arc::clone(&inbound);
        let mut failed = inbound.failed.subscribe();
        let (shutdown, mut stopping) = oneshot::channel();
        let accept_shared = Arc::clone(&shared);
        let accept = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    _ = &mut stopping => break,
                    _ = failed.changed() => break,
                    // Reap finished connections without keeping their task allocations.
                    _ = connections.join_next(), if !connections.is_empty() => {},
                    accepted = listener.accept() => {
                        let Ok(stream) = accepted else { break };
                        connections.spawn(serve_connection(
                            ServiceConnection {
                                stream,
                                _publication: Arc::clone(&listener),
                            },
                            Arc::clone(&accept_shared),
                            sender.clone(),
                        ));
                    }
                }
            }
            // Each connection owns both socket halves, so joining their cancellation
            // also waits for every reader and writer to release its resources.
            listener.stop().await;
            connections.shutdown().await;
            accept_shared.publication.lock().await.clients.clear();
            sender.close();
            drop(listener);
        });

        Ok(Self {
            shared,
            inbound,
            accept: Some(accept),
            shutdown: Some(shutdown),
            finished,
            capabilities: options.capabilities,
            directory: directory.to_path_buf(),
            endpoint,
            closed: false,
            #[cfg(all(test, unix))]
            service_publication,
        })
    }

    pub fn endpoint(&self) -> &LocalEndpoint {
        &self.endpoint
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Waits for listener and connection shutdown, publication cleanup, and release
    /// of the service's ownership lock.
    pub async fn close(&mut self) {
        if self.closed {
            return;
        }
        self.inbound.close();
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        // Borrow the handle so a cancelled close() can be awaited again.
        if let Some(accept) = self.accept.as_mut() {
            let _ = accept.await;
        }
        self.accept.take();
        // A panicked/aborted supervisor aborts its JoinSet without joining it.
        // Wait for the final socket's guard to finish cleanup and unlock as well.
        let _ = self.finished.wait_for(|finished| *finished).await;
        self.closed = true;
    }
}

impl Drop for LocalServerTransport {
    fn drop(&mut self) {
        // Last-resort cleanup for a transport dropped without close().
        // The supervisor owns every connection and completes their cancellation even
        // when the caller cannot await close().
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

#[async_trait]
impl Transport for LocalServerTransport {
    fn set_resources(&mut self, resources: ResourceRegistry) {
        *self.shared.resources.write().unwrap() = Some(resources);
    }
    fn capabilities(&self) -> BTreeSet<String> {
        self.capabilities.clone()
    }

    async fn send(&mut self, message: &Value) -> Result<()> {
        let line = format!("{}\n", serde_json::to_string(message)?);
        // Snapshot updates and live fanout share the same boundary as subscription:
        // a new client receives each update either in replay or live, exactly once.
        let mut publication = self.shared.publication.lock().await;
        if !is_resource(message) {
            publication.snapshot.apply(message)?;
        }
        publication.clients.retain(|client| client.send(&line));
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Value>> {
        self.inbound.receive().await
    }

    async fn stop_receiving(&mut self) {
        self.close().await;
    }
}

struct ServiceConnection {
    stream: LocalStream,
    _publication: Arc<ServicePublication>,
}

/// Validate the v0.9 envelope before it can reach the shared Runtime.
fn is_client_message(value: &Value) -> bool {
    let Some(message) = value.as_object() else {
        return false;
    };
    if message.get("version").and_then(Value::as_str) != Some("v0.9")
        || message.contains_key("action") == message.contains_key("error")
    {
        return false;
    }
    if let Some(action) = message.get("action") {
        return action.as_object().is_some_and(|action| {
            ["name", "surfaceId", "sourceComponentId", "timestamp"]
                .iter()
                .all(|key| action.get(*key).is_some_and(Value::is_string))
                && action.get("context").is_some_and(Value::is_object)
        });
    }
    message["error"].as_object().is_some_and(|error| {
        error.contains_key("code")
            && error.get("surfaceId").is_some_and(Value::is_string)
            && error.get("message").is_some_and(Value::is_string)
            && (error.get("code").and_then(Value::as_str) != Some("VALIDATION_FAILED")
                || (error.len() == 4 && error.get("path").is_some_and(Value::is_string)))
    })
}

async fn serve_connection(
    connection: ServiceConnection,
    shared: Arc<Shared>,
    inbound: Arc<InboundQueue>,
) {
    // Borrow the halves so the connection keeps both the stream and publication
    // alive until all reader/writer futures have been dropped.
    let (reader, mut writer) = connection.stream.into_split();
    let (sender, mut outbound, mut cancelled) = ClientSender::new();
    let announce_resources = shared.resources.read().unwrap().is_some();
    let snapshot = {
        // Capture replay and register for live updates atomically. Encode and write
        // one replay frame at a time so healthy snapshots may exceed the budget.
        let mut publication = shared.publication.lock().await;
        let snapshot = publication.snapshot.clone();
        publication.clients.push(sender.clone());
        snapshot
    };

    let writing = async {
        let mut cursor = SnapshotCursor::default();
        let mut announcement = announce_resources.then(hello);
        while let Some(message) = announcement
            .take()
            .or_else(|| snapshot.next_message(&mut cursor))
        {
            let line = format!("{message}\n");
            let Some(_permit) = sender.reserve(line.len()) else {
                return;
            };
            if writer.write_all(line.as_bytes()).await.is_err() {
                return;
            }
        }
        // Release the captured snapshot once replay completes, even for clients
        // that remain connected indefinitely.
        drop(snapshot);
        while let Some(frame) = outbound.recv().await {
            if writer.write_all(frame.line.as_bytes()).await.is_err() {
                return;
            }
        }
    };

    let reading = async {
        let mut reader = BufReader::new(reader);
        loop {
            // Cap partial frames as well as complete JSON. A peer that never sends
            // a newline must not allocate unbounded memory before queue admission.
            let mut line = Vec::new();
            let read = (&mut reader)
                .take((MAX_INBOUND_FRAME_BYTES + 1) as u64)
                .read_until(b'\n', &mut line)
                .await;
            if !matches!(read, Ok(bytes) if bytes > 0) {
                break;
            }
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.len() > MAX_INBOUND_FRAME_BYTES {
                break;
            }
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let Ok(mut value) = serde_json::from_slice::<Value>(&line) else {
                break;
            };
            if is_resource(&value) {
                if value["kind"] == "request" {
                    if let Some(resources) = shared.resources.read().unwrap().as_ref() {
                        if !sender.send(&format!("{}\n", resources.respond(&value))) {
                            break;
                        }
                    }
                }
                continue;
            }
            if !is_client_message(&value) {
                break;
            }
            // Valid client error reports carry no action; keep the connection open.
            if value.get("error").is_some() {
                continue;
            }
            // A2UI extensions must not select the Runtime's legacy command API.
            value.as_object_mut().unwrap().remove("type");
            if !inbound.push(value) {
                break;
            }
        }
    };
    // Neither half may outlive its connection task, including when cancelled during
    // shutdown or while a slow client has blocked the writer.
    tokio::select! {
        biased;
        _ = cancelled.changed() => {},
        _ = writing => {},
        _ = reading => {},
    }
    shared
        .publication
        .lock()
        .await
        .clients
        .retain(|client| !client.queue.same_channel(&sender.queue));
}

#[cfg(all(test, unix))]
use std::fs;
#[cfg(all(test, unix))]
use tokio::net::UnixStream;
#[cfg(all(test, unix))]
impl LocalServerTransport {
    fn socket_path(&self) -> &Path {
        match &self.endpoint {
            LocalEndpoint::Unix { path } => path,
            _ => unreachable!(),
        }
    }
}
#[cfg(all(test, unix))]
#[path = "../tests/unix_server/inbound.rs"]
mod inbound_tests;
#[cfg(all(test, unix))]
#[path = "../tests/unix_server/outbound.rs"]
mod outbound_tests;

#[cfg(all(test, unix))]
#[path = "../tests/unix_server/publication.rs"]
mod publication_tests;
