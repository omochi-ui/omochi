//! Nonblocking C ABI for Flutter. Only bounded JSON commands cross this boundary;
//! image bytes use a separate bounded copy API. No Dart callback runs on a Rust thread.
#[cfg(target_os = "android")]
mod android;
use anyhow::{Context, Result, bail};
use omochi_hub::{
    host::HostFuture,
    identity::{CommitOutcome, IdentityError, IdentityStore},
    native::NativeHub,
    scoped::{ScopedSubscription, SurfaceHandle},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    ffi::{CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tokio::{
    runtime::Runtime,
    sync::{Mutex as AsyncMutex, OwnedSemaphorePermit, RwLock, Semaphore, oneshot},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

const MAX_COMMAND: usize = 4 * 1024 * 1024;
const MAX_QUEUE: usize = 16 * 1024 * 1024;
const MAX_COMMAND_BYTES: usize = 8 * 1024 * 1024;
type Sessions = Mutex<BTreeMap<u64, Arc<Session>>>;
static SESSIONS: OnceLock<Sessions> = OnceLock::new();
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("native Hub runtime")
    })
}
fn sessions() -> &'static Sessions {
    SESSIONS.get_or_init(Mutex::default)
}
fn session(id: u64) -> Option<Arc<Session>> {
    sessions().lock().ok()?.get(&id).cloned()
}

struct Reply {
    text: String,
    _slot: OwnedSemaphorePermit,
    _bytes: OwnedSemaphorePermit,
}
#[derive(Default)]
struct Output {
    replies: VecDeque<Reply>,
    bytes: usize,
    terminal: Option<String>,
}

struct CallbackStore {
    next: AtomicU64,
    pending: Mutex<BTreeMap<u64, oneshot::Sender<Value>>>,
    requests: Mutex<VecDeque<String>>,
    stopping: CancellationToken,
}
impl CallbackStore {
    fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
            pending: Mutex::default(),
            requests: Mutex::default(),
            stopping: CancellationToken::new(),
        }
    }
    async fn request(&self, mut request: Value, writing: bool) -> Result<Value, IdentityError> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (reply, response) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            if !pending.is_empty() {
                return Err(IdentityError::Unavailable);
            }
            pending.insert(id, reply);
        }
        request["kind"] = "storage".into();
        request["id"] = id.into();
        self.requests.lock().unwrap().push_back(request.to_string());
        // A write already dispatched to secure storage must settle. Closing the UI
        // does not claim it failed or roll it back. Reads can be cancelled safely.
        let result = if writing {
            response.await.map_err(|_| IdentityError::OutcomeUnknown)
        } else {
            tokio::select! { _=self.stopping.cancelled()=>Err(IdentityError::Unavailable), value=response=>value.map_err(|_| IdentityError::Unavailable) }
        };
        self.pending.lock().unwrap().remove(&id);
        result
    }
    fn reply(&self, id: u64, value: Value) -> bool {
        self.pending
            .lock()
            .unwrap()
            .remove(&id)
            .is_some_and(|reply| reply.send(value).is_ok())
    }
}
impl IdentityStore for CallbackStore {
    fn read(&self) -> HostFuture<'_, Result<Option<String>, IdentityError>> {
        Box::pin(async {
            let result = self.request(json!({"op":"read"}), false).await?;
            if result["ok"] != true {
                return Err(IdentityError::Unavailable);
            }
            match &result["value"] {
                Value::Null => Ok(None),
                Value::String(value) if value.len() <= 1024 * 1024 => Ok(Some(value.clone())),
                _ => Err(IdentityError::Invalid),
            }
        })
    }
    fn compare_exchange(
        &self,
        expected: Option<String>,
        replacement: String,
    ) -> HostFuture<'_, CommitOutcome> {
        Box::pin(async move {
            match self
                .request(
                    json!({"op":"commit","expected":expected,"replacement":replacement}),
                    true,
                )
                .await
            {
                Ok(result) => match result["outcome"].as_str() {
                    Some("committed") => CommitOutcome::Committed,
                    Some("conflict") => CommitOutcome::Conflict,
                    Some("failed") => CommitOutcome::Failed,
                    _ => CommitOutcome::Unknown,
                },
                Err(_) => CommitOutcome::Unknown,
            }
        })
    }
}

#[derive(Clone)]
struct Handle {
    surface: SurfaceHandle,
    render: CancellationToken,
}
impl Handle {
    fn expired(&self) -> bool {
        self.render.is_cancelled() || self.surface.is_cancelled()
    }
}
struct Blob {
    bytes: Vec<u8>,
    handle: Handle,
    _permit: OwnedSemaphorePermit,
}
struct Session {
    native: RwLock<Option<NativeHub>>,
    store: Arc<CallbackStore>,
    subscription: AsyncMutex<Option<ScopedSubscription>>,
    render_cancel: Mutex<CancellationToken>,
    handles: Mutex<BTreeMap<u64, Handle>>,
    blobs: Mutex<BTreeMap<u64, Blob>>,
    image_slots: Arc<Semaphore>,
    commands: Mutex<BTreeMap<u64, CancellationToken>>,
    slots: Arc<Semaphore>,
    close_slot: Arc<Semaphore>,
    command_bytes: Arc<Semaphore>,
    next: AtomicU64,
    output: Mutex<Output>,
    tasks: TaskTracker,
    closing: AtomicBool,
    closed: AtomicBool,
}
impl Session {
    fn new() -> Self {
        Self {
            native: RwLock::new(None),
            store: Arc::new(CallbackStore::new()),
            subscription: AsyncMutex::new(None),
            render_cancel: Mutex::new(CancellationToken::new()),
            handles: Mutex::default(),
            blobs: Mutex::default(),
            image_slots: Arc::new(Semaphore::new(2)),
            commands: Mutex::default(),
            slots: Arc::new(Semaphore::new(8)),
            close_slot: Arc::new(Semaphore::new(1)),
            command_bytes: Arc::new(Semaphore::new(MAX_COMMAND_BYTES)),
            next: AtomicU64::new(1),
            output: Mutex::default(),
            tasks: TaskTracker::new(),
            closing: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        }
    }
    fn next_id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
    async fn native(&self) -> Result<NativeHub> {
        self.native
            .read()
            .await
            .clone()
            .context("Hub has not started")
    }
    fn cancel_render(&self) {
        let mut cancel = self.render_cancel.lock().unwrap();
        cancel.cancel();
        *cancel = CancellationToken::new();
        self.handles.lock().unwrap().clear();
        self.blobs.lock().unwrap().clear();
    }
    fn handle(&self, id: u64) -> Result<Handle> {
        self.handles
            .lock()
            .unwrap()
            .get(&id)
            .filter(|handle| !handle.expired())
            .cloned()
            .context("Surface handle expired")
    }
    fn register_handle(&self, surface: SurfaceHandle, render: CancellationToken) -> u64 {
        let handle = Handle { surface, render };
        let mut handles = self.handles.lock().unwrap();
        handles.retain(|_, h| !h.expired());
        if let Some((id, _)) = handles
            .iter()
            .find(|(_, h)| h.surface.surface_id() == handle.surface.surface_id())
        {
            return *id;
        }
        let id = self.next_id();
        handles.insert(id, handle);
        id
    }
    fn enqueue(self: &Arc<Self>, reply: Reply) {
        let mut output = self.output.lock().unwrap();
        if output.bytes.saturating_add(reply.text.len()) > MAX_QUEUE {
            output.replies.clear();
            output.bytes = 0;
            output.terminal = Some("native result queue overflow; stop and reopen the Hub".into());
            self.closing.store(true, Ordering::Release);
            drop(output);
            self.cancel_render();
            // Losing host results is terminal for this bridge. Reserve shutdown
            // credit so the core cannot keep networking behind an unusable UI.
            // Already dispatched writes still settle through storage callbacks.
            self.submit(br#"{"op":"close"}"#);
            return;
        }
        output.bytes += reply.text.len();
        output.replies.push_back(reply);
    }
    fn poll(&self) -> Option<String> {
        // Storage is a separate control path. A slow renderer cannot bury a save
        // callback behind render data and deadlock native administration.
        if let Some(request) = self.store.requests.lock().unwrap().pop_front() {
            return Some(request);
        }
        let mut output = self.output.lock().unwrap();
        if let Some(error) = output.terminal.take() {
            return Some(json!({"kind":"terminal","error":error}).to_string());
        }
        let reply = output.replies.pop_front()?;
        output.bytes -= reply.text.len();
        Some(reply.text)
    }
    async fn execute(
        self: &Arc<Self>,
        command: Command,
        cancel: CancellationToken,
    ) -> Result<Value> {
        match command {
            Command::Start { online } => {
                let mut native = self.native.write().await;
                if self.closing.load(Ordering::Acquire) {
                    bail!("Hub is closing")
                }
                if native.is_some() {
                    bail!("Hub already started")
                }
                *native = Some(NativeHub::open(self.store.clone(), online).await?);
                Ok(json!({"identity":native.as_ref().unwrap().identity().await?}))
            }
            Command::State => {
                let native = self.native().await?;
                let hub = native.hub().await;
                let (services, connections) = if let Ok(hub) = hub {
                    (hub.services().await, hub.connection_status().await)
                } else {
                    (vec![], vec![])
                };
                Ok(
                    json!({"identity":native.identity().await?,"peers":native.peers().await?,"services":services,"connections":connections}),
                )
            }
            Command::Pair { invitation } => {
                self.native().await?.pair(invitation, cancel).await?;
                Ok(Value::Null)
            }
            Command::Forget { id } => {
                self.native().await?.forget(id.parse()?).await?;
                Ok(Value::Null)
            }
            Command::Retry { id } => {
                self.native().await?.retry(id.parse()?).await?;
                Ok(Value::Null)
            }
            Command::Reload => {
                self.cancel_render();
                self.native().await?.reload_authorization().await?;
                Ok(Value::Null)
            }
            Command::Suspend => {
                self.cancel_render();
                self.native().await?.suspend().await?;
                *self.subscription.lock().await = None;
                Ok(Value::Null)
            }
            Command::Resume { online } => {
                self.cancel_render();
                self.native().await?.resume(online).await?;
                *self.subscription.lock().await = None;
                Ok(Value::Null)
            }
            Command::Subscribe => {
                self.cancel_render();
                let mut current = self.subscription.lock().await;
                let subscription = self.native().await?.hub().await?.subscribe_scoped().await?;
                *self.render_cancel.lock().unwrap() = subscription.cancellation_token();
                *current = Some(subscription);
                Ok(Value::Null)
            }
            Command::Next { ack } => {
                let render_cancel = self.render_cancel.lock().unwrap().clone();
                let mut subscription = self.subscription.lock().await;
                let subscription = subscription.as_mut().context("no render subscription")?;
                if let Some(ack) = ack {
                    subscription.acknowledge(ack)?;
                }
                let batch = tokio::select! {
                    _=cancel.cancelled()=>bail!("render request cancelled"),
                    _=render_cancel.cancelled()=>bail!("render subscription replaced"),
                    batch=subscription.next_batch()=>batch?,
                };
                let handle = batch
                    .handle
                    .clone()
                    .map(|handle| self.register_handle(handle, render_cancel.clone()));
                Ok(
                    json!({"epoch":batch.epoch,"sequence":batch.sequence,"snapshot":batch.snapshot,"snapshotEnd":batch.snapshot_end,"message":batch.message.as_deref(),"handle":handle}),
                )
            }
            Command::Action { handle, message } => {
                self.handle(handle)?.surface.action(message).await?;
                Ok(Value::Null)
            }
            Command::Image { handle, uri } => {
                let permit = self
                    .image_slots
                    .clone()
                    .try_acquire_owned()
                    .context("image reader is busy; retry later")?;
                let handle = self.handle(handle)?;
                let image = tokio::select! {
                    _=handle.render.cancelled()=>bail!("Surface handle expired"),
                    image=handle.surface.image(&uri,cancel)=>image?,
                };
                if handle.expired() {
                    bail!("Surface handle expired")
                }
                let id = self.next_id();
                let result = json!({"blob":id,"length":image.bytes.len(),"mime":image.mime,"width":image.width,"height":image.height});
                self.blobs.lock().unwrap().insert(
                    id,
                    Blob {
                        bytes: image.bytes,
                        handle,
                        _permit: permit,
                    },
                );
                Ok(result)
            }
            Command::Close => {
                self.cancel_render();
                self.store.stopping.cancel();
                // Start may still be settling a dispatched identity write. The write
                // lock waits while host polling continues servicing storage requests.
                let mut native = self.native.write().await;
                if let Some(hub) = native.as_ref() {
                    hub.close().await?;
                }
                *native = None;
                *self.subscription.lock().await = None;
                self.closed.store(true, Ordering::Release);
                Ok(Value::Null)
            }
        }
    }
    fn submit(self: &Arc<Self>, bytes: &[u8]) -> u64 {
        let Ok(command) = serde_json::from_slice::<Command>(bytes) else {
            return 0;
        };
        let close = matches!(command, Command::Close);
        let mut commands = self.commands.lock().unwrap();
        if self.closing.load(Ordering::Acquire) && !close {
            return 0;
        }
        let pool = if close { &self.close_slot } else { &self.slots };
        let Ok(slot) = pool.clone().try_acquire_owned() else {
            return 0;
        };
        let charge_size = if close { 0 } else { bytes.len().max(1) as u32 };
        let Ok(charge) = self
            .command_bytes
            .clone()
            .try_acquire_many_owned(charge_size)
        else {
            return 0;
        };
        if close {
            self.closing.store(true, Ordering::Release);
            for cancel in commands.values() {
                cancel.cancel();
            }
        }
        let id = self.next_id();
        let cancel = CancellationToken::new();
        commands.insert(id, cancel.clone());
        drop(commands);
        let session = self.clone();
        let _runtime = runtime().enter();
        self.tasks.spawn(async move {
            let result = session.execute(command, cancel).await;
            let reply = match result {
                Ok(value) => json!({"kind":"result","id":id,"ok":true,"value":value}),
                Err(error) => json!({"kind":"result","id":id,"ok":false,"error":error.to_string()}),
            };
            session.commands.lock().unwrap().remove(&id);
            session.enqueue(Reply {
                text: reply.to_string(),
                _slot: slot,
                _bytes: charge,
            });
        });
        id
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Start { online: bool },
    State,
    Pair { invitation: String },
    Forget { id: String },
    Retry { id: String },
    Reload,
    Suspend,
    Resume { online: bool },
    Subscribe,
    Next { ack: Option<u64> },
    Action { handle: u64, message: Value },
    Image { handle: u64, uri: String },
    Close,
}

#[unsafe(no_mangle)]
pub extern "C" fn omochi_hub_abi_version() -> u32 {
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn omochi_hub_create() -> u64 {
    catch_unwind(|| {
        let mut sessions = sessions().lock().unwrap();
        if sessions.len() >= 4 {
            return 0;
        }
        let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        sessions.insert(id, Arc::new(Session::new()));
        id
    })
    .unwrap_or(0)
}
/// # Safety
/// `data` must reference `length` readable bytes for this call. No pointer is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omochi_hub_submit(id: u64, data: *const u8, length: usize) -> u64 {
    catch_unwind(AssertUnwindSafe(|| {
        if data.is_null() || length == 0 || length > MAX_COMMAND {
            return 0;
        }
        let Some(session) = session(id) else {
            return 0;
        };
        session.submit(unsafe { std::slice::from_raw_parts(data, length) })
    }))
    .unwrap_or(0)
}
#[unsafe(no_mangle)]
pub extern "C" fn omochi_hub_poll(id: u64) -> *mut c_char {
    catch_unwind(|| {
        session(id)
            .and_then(|s| s.poll())
            .and_then(|s| CString::new(s).ok())
            .map_or(std::ptr::null_mut(), CString::into_raw)
    })
    .unwrap_or(std::ptr::null_mut())
}
/// # Safety
/// `value` must be null or an unreleased pointer returned by `omochi_hub_poll`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omochi_hub_string_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}
/// # Safety
/// `data` must reference `length` readable bytes for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omochi_hub_storage_reply(
    id: u64,
    request: u64,
    data: *const u8,
    length: usize,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if data.is_null() || length > MAX_COMMAND {
            return false;
        }
        let Some(session) = session(id) else {
            return false;
        };
        let Ok(value) = serde_json::from_slice(unsafe { std::slice::from_raw_parts(data, length) })
        else {
            return false;
        };
        session.store.reply(request, value)
    }))
    .unwrap_or(false)
}
#[unsafe(no_mangle)]
pub extern "C" fn omochi_hub_cancel(id: u64, request: u64) {
    let _ = catch_unwind(|| {
        if let Some(session) = session(id)
            && let Some(token) = session.commands.lock().unwrap().get(&request)
        {
            token.cancel();
        }
    });
}
/// # Safety
/// `destination` must reference `length` writable bytes. Copies at most 64 KiB.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omochi_hub_blob_read(
    id: u64,
    blob: u64,
    offset: usize,
    destination: *mut u8,
    length: usize,
) -> isize {
    catch_unwind(AssertUnwindSafe(|| {
        if destination.is_null() || length > 64 * 1024 {
            return -1;
        }
        let Some(session) = session(id) else {
            return -1;
        };
        let blobs = session.blobs.lock().unwrap();
        let Some(blob) = blobs.get(&blob).filter(|blob| !blob.handle.expired()) else {
            return -1;
        };
        if offset > blob.bytes.len() {
            return -1;
        }
        let size = length.min(blob.bytes.len() - offset);
        unsafe { std::ptr::copy_nonoverlapping(blob.bytes[offset..].as_ptr(), destination, size) };
        size as isize
    }))
    .unwrap_or(-1)
}
#[unsafe(no_mangle)]
pub extern "C" fn omochi_hub_blob_free(id: u64, blob: u64) {
    let _ = catch_unwind(|| {
        if let Some(session) = session(id) {
            session.blobs.lock().unwrap().remove(&blob);
        }
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn omochi_hub_release(id: u64) -> bool {
    catch_unwind(|| {
        let mut sessions = sessions().lock().unwrap();
        let Some(session) = sessions.get(&id) else {
            return true;
        };
        if !session.closed.load(Ordering::Acquire)
            || !session.tasks.is_empty()
            || !session.store.pending.lock().unwrap().is_empty()
        {
            return false;
        }
        sessions.remove(&id);
        true
    })
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn submit(session: &Arc<Session>, value: Value) -> u64 {
        let id = session.submit(value.to_string().as_bytes());
        assert_ne!(id, 0);
        id
    }
    async fn poll(session: &Session) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(value) = session.poll() {
                    break serde_json::from_str(&value).unwrap();
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("bridge reply")
    }
    async fn stopped(session: &Session) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !session.tasks.is_empty() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn storage_callbacks_are_async_and_close_waits_for_dispatched_save() {
        let session = Arc::new(Session::new());
        let start = submit(&session, json!({"op":"start","online":false}));
        let read = poll(&session).await;
        assert_eq!(read["op"], "read");
        assert!(!session.tasks.is_empty());
        session.store.reply(
            read["id"].as_u64().unwrap(),
            json!({"ok":true,"value":null}),
        );
        let write = poll(&session).await;
        assert_eq!(write["op"], "commit");
        assert!(write["expected"].is_null());
        let document: Value = serde_json::from_str(write["replacement"].as_str().unwrap()).unwrap();
        assert!(document["secret"].is_string());
        let close = submit(&session, json!({"op":"close"}));
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!session.closed.load(Ordering::Acquire));
        assert!(session.poll().is_none());
        session.store.reply(
            write["id"].as_u64().unwrap(),
            json!({"outcome":"committed"}),
        );
        let first = poll(&session).await;
        let second = poll(&session).await;
        assert!(first["ok"] == true && second["ok"] == true);
        assert_eq!(
            BTreeMap::from([
                (first["id"].as_u64().unwrap(), ()),
                (second["id"].as_u64().unwrap(), ())
            ]),
            BTreeMap::from([(start, ()), (close, ())])
        );
        stopped(&session).await;
        assert!(session.closed.load(Ordering::Acquire));
        assert!(session.native.read().await.is_none());
    }

    #[tokio::test]
    async fn corrupt_existing_identity_does_not_issue_a_write_or_start_networking() {
        for document in ["{}", "not JSON", r#"{"secret":"bad","peers":[]}"#] {
            let session = Arc::new(Session::new());
            submit(&session, json!({"op":"start","online":false}));
            let read = poll(&session).await;
            session.store.reply(
                read["id"].as_u64().unwrap(),
                json!({"ok":true,"value":document}),
            );
            let result = poll(&session).await;
            assert_eq!(result["ok"], false);
            assert!(result["error"].as_str().unwrap().contains("refusing"));
            assert!(session.native.read().await.is_none());
            assert!(session.store.requests.lock().unwrap().is_empty());
            submit(&session, json!({"op":"close"}));
            poll(&session).await;
            stopped(&session).await;
        }
    }

    #[tokio::test]
    async fn close_has_reserved_credit_and_cancelled_read_cannot_restart_session() {
        let session = Arc::new(Session::new());
        submit(&session, json!({"op":"start","online":false}));
        let read = poll(&session).await;
        for _ in 0..7 {
            submit(&session, json!({"op":"state"}));
        }
        assert_eq!(session.submit(br#"{"op":"state"}"#), 0);
        let close = submit(&session, json!({"op":"close"}));
        let mut replies = Vec::new();
        for _ in 0..9 {
            replies.push(poll(&session).await);
        }
        assert!(replies.iter().any(|r| r["id"] == close && r["ok"] == true));
        assert!(!session.store.reply(
            read["id"].as_u64().unwrap(),
            json!({"ok":true,"value":null})
        ));
        assert_eq!(session.submit(br#"{"op":"start","online":false}"#), 0);
        stopped(&session).await;
        assert!(session.native.read().await.is_none());
    }

    #[tokio::test]
    async fn result_overflow_invalidates_rendering_and_stops_the_core() {
        let session = Arc::new(Session::new());
        let old_render = session.render_cancel.lock().unwrap().clone();
        session.enqueue(Reply {
            text: "x".repeat(MAX_QUEUE + 1),
            _slot: session.slots.clone().try_acquire_owned().unwrap(),
            _bytes: session.command_bytes.clone().try_acquire_owned().unwrap(),
        });
        assert!(old_render.is_cancelled());
        assert!(session.closing.load(Ordering::Acquire));
        assert_eq!(poll(&session).await["kind"], "terminal");
        assert_eq!(poll(&session).await["ok"], true);
        stopped(&session).await;
        assert!(session.closed.load(Ordering::Acquire));
        assert_eq!(session.output.lock().unwrap().bytes, 0);
    }

    #[tokio::test]
    async fn unknown_commit_is_never_reported_as_started() {
        let session = Arc::new(Session::new());
        submit(&session, json!({"op":"start","online":false}));
        let read = poll(&session).await;
        session.store.reply(
            read["id"].as_u64().unwrap(),
            json!({"ok":true,"value":null}),
        );
        let write = poll(&session).await;
        session
            .store
            .reply(write["id"].as_u64().unwrap(), json!({"outcome":"unknown"}));
        let result = poll(&session).await;
        assert_eq!(result["ok"], false);
        assert!(session.native.read().await.is_none());
        submit(&session, json!({"op":"close"}));
        poll(&session).await;
        stopped(&session).await;
    }
}
