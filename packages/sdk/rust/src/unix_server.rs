use crate::resources::{ResourceRegistry, hello, is_resource};
use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, Notify, OwnedSemaphorePermit, Semaphore, mpsc, oneshot, watch};
use tokio::task::{JoinHandle, JoinSet};

use crate::snapshot::{A2uiSnapshot, SnapshotCursor};
use crate::transport::Transport;
use crate::{Error, Result};

const SOCKET_FILE_NAME: &str = "a2ui.sock";

/// Linux caps `sun_path` at 108 bytes and macOS at 104; leave room for the null terminator.
const MAX_SOCKET_PATH_BYTES: usize = 100;

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

/// Mirrors the id constraint the hub applies, so `<serviceId>/<surfaceId>` stays splittable.
pub fn validate_service_id(service_id: &str) -> Result<()> {
    // "." and ".." pass the character check but would collapse or escape the service directory.
    let acceptable = !service_id.is_empty()
        && service_id.len() <= 64
        && service_id != "."
        && service_id != ".."
        && service_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if acceptable {
        Ok(())
    } else {
        Err(Error::Declaration(format!(
            "service id must match [A-Za-z0-9._-]{{1,64}} and cannot be \".\" or \"..\": {service_id:?}"
        )))
    }
}

/// Resolves the directory holding one subdirectory per published service.
///
/// `$XDG_RUNTIME_DIR` is per-user and mode 0700, and that permission is the entire local
/// trust boundary. A world-writable `/tmp/omochi` is never used, because another local
/// user could pre-create a service directory and squat the socket.
pub fn omochi_runtime_root() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(configured) => PathBuf::from(configured).join("omochi"),
        None => {
            let uid = unsafe { libc_getuid() };
            std::env::temp_dir()
                .join(format!("omochi-{uid}"))
                .join("omochi")
        }
    }
}

// Avoids a `libc` dependency for the single value the fallback path needs.
unsafe fn libc_getuid() -> u32 {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}

#[derive(Debug, Clone)]
pub struct UnixSocketServerOptions {
    pub service_id: String,
    /// Human-readable label shown by a hub. Defaults to the service id.
    pub name: Option<String>,
    /// An emoji, or a `data:image/...;base64,` URL a browser can render directly.
    pub icon: Option<String>,
    /// Defaults to `$XDG_RUNTIME_DIR/omochi`.
    pub runtime_dir: Option<PathBuf>,
    pub capabilities: BTreeSet<String>,
}

impl UnixSocketServerOptions {
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

/// Publishes one application on a unix domain socket and advertises it for discovery.
///
/// The transport writes `<runtime_dir>/<service_id>/manifest.json` next to the socket so a
/// hub can find it without configuration. Reaching the socket is the authentication: the
/// runtime directory is mode 0700, so only the owning user can connect.
///
/// One application is fanned out to every connected renderer, and their inbound messages
/// are merged into a single stream for the Runtime.
pub struct UnixSocketServerTransport {
    shared: Arc<Shared>,
    inbound: Arc<InboundQueue>,
    accept: JoinHandle<()>,
    shutdown: Option<oneshot::Sender<()>>,
    capabilities: BTreeSet<String>,
    directory: PathBuf,
    socket_path: PathBuf,
    closed: bool,
    service_lock: Option<ServiceLock>,
}

impl UnixSocketServerTransport {
    pub async fn bind(mut options: UnixSocketServerOptions) -> Result<Self> {
        validate_service_id(&options.service_id)?;
        let root = options
            .runtime_dir
            .take()
            .unwrap_or_else(omochi_runtime_root);
        let directory = root.join(&options.service_id);
        fs::create_dir_all(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        let service_lock = lock_service(&root, &options.service_id)?;
        fs::create_dir_all(&directory)?;
        // create_dir_all's mode is masked by umask and leaves an existing directory untouched.
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;

        match Self::listen(&directory, options).await {
            Ok(mut transport) => {
                transport.service_lock = Some(service_lock);
                Ok(transport)
            }
            Err(error) => {
                // Leave no empty directory behind for a discovery pass to walk into.
                // remove_dir refuses a non-empty directory, so a live service is safe.
                let _ = fs::remove_dir(&directory);
                Err(error)
            }
        }
    }

    async fn listen(directory: &Path, options: UnixSocketServerOptions) -> Result<Self> {
        let socket_path = directory.join(SOCKET_FILE_NAME);
        if socket_path.as_os_str().len() > MAX_SOCKET_PATH_BYTES {
            return Err(Error::Transport(format!(
                "unix socket path is too long for this platform: {}",
                socket_path.display()
            )));
        }
        claim_socket_path(&socket_path).await?;

        let listener = UnixListener::bind(&socket_path)?;
        // Linux enforces unix socket permissions on connect, and bind applies the umask.
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))?;

        let shared = Arc::new(Shared {
            resources: std::sync::RwLock::new(None),
            publication: Mutex::new(Publication::default()),
        });
        // Advertise only once the socket accepts connections, so a readable manifest is
        // always connectable.
        write_manifest(
            directory,
            options.name.as_deref().unwrap_or(&options.service_id),
            options.icon.as_deref(),
        )?;

        let inbound = Arc::new(InboundQueue::new());
        let sender = Arc::clone(&inbound);
        let mut failed = inbound.failed.subscribe();
        let failed_directory = directory.to_owned();
        let (shutdown, mut stopping) = oneshot::channel();
        let accept_shared = Arc::clone(&shared);
        let accept = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    _ = &mut stopping => break,
                    _ = failed.changed() => {
                        let _ = fs::remove_file(failed_directory.join("manifest.json"));
                        break;
                    },
                    // Reap finished connections without keeping their task allocations.
                    _ = connections.join_next(), if !connections.is_empty() => {},
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { break };
                        connections.spawn(serve_connection(
                            stream,
                            Arc::clone(&accept_shared),
                            sender.clone(),
                        ));
                    }
                }
            }
            drop(listener);
            // Each connection owns both socket halves, so joining their cancellation
            // also waits for every reader and writer to release its resources.
            connections.shutdown().await;
            accept_shared.publication.lock().await.clients.clear();
            sender.close();
        });

        Ok(Self {
            shared,
            inbound,
            accept,
            shutdown: Some(shutdown),
            capabilities: options.capabilities,
            directory: directory.to_path_buf(),
            socket_path,
            closed: false,
            service_lock: None,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Retracts the advertisement and waits for all connections to stop before removing
    /// the service directory.
    pub async fn close(&mut self) {
        if self.closed {
            return;
        }
        self.inbound.close();
        let _ = fs::remove_file(self.directory.join("manifest.json"));
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        // Borrow the handle so a cancelled close() can be awaited again.
        let _ = (&mut self.accept).await;
        let _ = fs::remove_dir_all(&self.directory);
        self.closed = true;
        self.service_lock.take();
    }
}

impl Drop for UnixSocketServerTransport {
    fn drop(&mut self) {
        // Last-resort cleanup for a transport dropped without close().
        // The supervisor owns every connection and completes their cancellation even
        // when the caller cannot await close().
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if !self.closed {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
}

#[async_trait]
impl Transport for UnixSocketServerTransport {
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

struct ServiceLock(fs::File);

impl Drop for ServiceLock {
    fn drop(&mut self) {
        // A concurrent fork can keep this open file description alive until exec,
        // despite CLOEXEC. Release ownership before closing our descriptor so a
        // successor does not depend on an unrelated child's scheduling.
        unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

/// Keep this inode forever: unlinking a locked file would allow a second owner.
/// `~` cannot occur in service ids, and service cleanup never touches the lock file.
fn lock_service(root: &Path, service_id: &str) -> Result<ServiceLock> {
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(root.join(format!(".{service_id}.lock~")))?;
    // flock locks open file descriptions, so distinct transports in one process also
    // contend, and the kernel releases ownership if the process crashes.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::WouldBlock {
            return Err(Error::Transport(format!(
                "another omochi service already owns {}",
                root.join(service_id).display()
            )));
        }
        return Err(error.into());
    }
    Ok(ServiceLock(file))
}

/// Takes ownership of the socket path, removing one left behind by a crash.
///
/// A path that still accepts connections means another instance owns this service id;
/// the caller holds the lifetime lock through probing, binding and cleanup.
async fn claim_socket_path(socket_path: &Path) -> Result<()> {
    if !socket_path.exists() {
        return Ok(());
    }
    match UnixStream::connect(socket_path).await {
        Ok(_) => {
            return Err(Error::Transport(format!(
                "another omochi service already owns {}",
                socket_path.display()
            )));
        }
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) => {}
        Err(error) => return Err(error.into()),
    }
    fs::remove_file(socket_path)?;
    Ok(())
}

/// Publishes the manifest atomically so a discovery pass never reads a half-written file.
fn write_manifest(directory: &Path, name: &str, icon: Option<&str>) -> Result<()> {
    let mut manifest = Map::new();
    manifest.insert("name".into(), Value::String(name.to_owned()));
    if let Some(icon) = icon {
        manifest.insert("icon".into(), Value::String(icon.to_owned()));
    }
    manifest.insert("socket".into(), Value::String(SOCKET_FILE_NAME.to_owned()));
    let temporary = directory.join(format!(".manifest.{}.tmp", std::process::id()));
    fs::write(
        &temporary,
        format!("{}\n", serde_json::to_string(&Value::Object(manifest))?),
    )?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    fs::rename(&temporary, directory.join("manifest.json"))?;
    Ok(())
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

async fn serve_connection(stream: UnixStream, shared: Arc<Shared>, inbound: Arc<InboundQueue>) {
    let (reader, mut writer) = stream.into_split();
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

#[cfg(test)]
#[path = "../tests/unix_server/publication.rs"]
mod publication_tests;

#[cfg(test)]
#[path = "../tests/unix_server/outbound.rs"]
mod outbound_tests;

#[cfg(test)]
#[path = "../tests/unix_server/inbound.rs"]
mod inbound_tests;
