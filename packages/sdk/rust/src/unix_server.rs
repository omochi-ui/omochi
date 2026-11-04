use crate::resources::{ResourceRegistry, hello, is_resource};
use std::collections::BTreeSet;
use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};

use crate::snapshot::A2uiSnapshot;
use crate::transport::Transport;
use crate::{Error, Result};

const SOCKET_FILE_NAME: &str = "a2ui.sock";

/// Linux caps `sun_path` at 108 bytes and macOS at 104; leave room for the null terminator.
const MAX_SOCKET_PATH_BYTES: usize = 100;

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
    clients: Vec<mpsc::UnboundedSender<String>>,
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
    inbound: mpsc::UnboundedReceiver<Value>,
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

        let (sender, inbound) = mpsc::unbounded_channel();
        let (shutdown, mut stopping) = oneshot::channel();
        let accept_shared = Arc::clone(&shared);
        let accept = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    _ = &mut stopping => break,
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
        let _ = fs::remove_file(self.directory.join("manifest.json"));
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        // Borrow the handle so a cancelled close() can be awaited again.
        let _ = (&mut self.accept).await;
        let _ = fs::remove_dir_all(&self.directory);
        self.inbound.close();
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
        publication
            .clients
            .retain(|client| client.send(line.clone()).is_ok());
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Value>> {
        Ok(self.inbound.recv().await)
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

async fn serve_connection(
    stream: UnixStream,
    shared: Arc<Shared>,
    inbound: mpsc::UnboundedSender<Value>,
) {
    let (reader, mut writer) = stream.into_split();
    let (sender, mut outbound) = mpsc::unbounded_channel::<String>();
    if shared.resources.read().unwrap().is_some() {
        let _ = sender.send(format!("{}\n", hello()));
    }
    {
        // Queue replay and register for live updates without allowing publication
        // between them. Socket writes happen only after releasing this lock.
        let mut publication = shared.publication.lock().await;
        for message in publication.snapshot.messages() {
            let Ok(encoded) = serde_json::to_string(&message) else {
                continue;
            };
            if sender.send(format!("{encoded}\n")).is_err() {
                return;
            }
        }
        publication.clients.push(sender.clone());
    }

    let writing = async move {
        while let Some(line) = outbound.recv().await {
            if writer.write_all(line.as_bytes()).await.is_err() {
                return;
            }
        }
    };

    let reading = async {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(mut value) = serde_json::from_str::<Value>(&line) else {
                break;
            };
            if is_resource(&value) {
                if value["kind"] == "request" {
                    if let Some(resources) = shared.resources.read().unwrap().as_ref() {
                        let _ = sender.send(format!("{}\n", resources.respond(&value)));
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
            if inbound.send(value).is_err() {
                break;
            }
        }
    };
    // Neither half may outlive its connection task, including when cancelled during
    // shutdown or while a slow client has blocked the writer.
    tokio::select! {
        _ = writing => {},
        _ = reading => {},
    }
}

#[cfg(test)]
#[path = "../tests/unix_server/publication.rs"]
mod publication_tests;
