//! Native utility publication using the existing SDK manifest and JSONL contract.
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use omochi_local::{LocalEndpoint, LocalRuntime, LocalStream};
use omochi_protocol::{MAX_FRAME_BYTES, fanout, queue};
use omochi_ui::{A2uiSnapshot, LocalServerOptions};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, Semaphore, mpsc},
    task::{JoinHandle, JoinSet},
};
use tokio_util::{
    codec::{Framed, LinesCodec},
    sync::CancellationToken,
};

struct State {
    snapshot: A2uiSnapshot,
    events: fanout::Sender,
}
pub struct LocalTransport {
    state: Arc<Mutex<State>>,
    input: mpsc::Receiver<queue::Entry<Value>>,
    stop: CancellationToken,
    task: Option<JoinHandle<()>>,
    endpoint: LocalEndpoint,
}
impl LocalTransport {
    pub async fn bind(options: LocalServerOptions) -> Result<Self> {
        omochi_local::validate_service_id(&options.service_id)?;
        let root = LocalRuntime::prepare(options.runtime_dir)?;
        let publication = root
            .publish(
                &options.service_id,
                options.name.as_deref().unwrap_or(&options.service_id),
                options.icon,
            )
            .await?;
        let endpoint = publication.endpoint().clone();
        let (events, _) = fanout::channel(32, fanout::DEFAULT_BYTES);
        let state = Arc::new(Mutex::new(State {
            snapshot: A2uiSnapshot::new(),
            events,
        }));
        let (sender, input) = queue::channel(8, queue::DEFAULT_BYTES, MAX_FRAME_BYTES);
        let stop = CancellationToken::new();
        let shared = state.clone();
        let cancellation = stop.clone();
        let task = tokio::spawn(async move {
            let clients = Arc::new(Semaphore::new(16));
            let mut tasks = JoinSet::new();
            loop {
                tokio::select! {
                    _ = cancellation.cancelled() => break,
                    _ = tasks.join_next(), if !tasks.is_empty() => {},
                    result = publication.accept() => {
                        let Ok(stream) = result else { break; };
                        let Ok(permit) = clients.clone().try_acquire_owned() else { continue; };
                        let state = shared.clone(); let input = sender.clone(); let stop = cancellation.clone();
                        tasks.spawn(async move {
                            let _permit = permit;
                            tokio::select! { _ = stop.cancelled() => {}, _ = connection(stream, state, input) => {} }
                        });
                    }
                }
            }
            publication.stop().await;
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
            drop(publication);
        });
        Ok(Self {
            state,
            input,
            stop,
            task: Some(task),
            endpoint,
        })
    }
    pub fn endpoint(&self) -> &LocalEndpoint {
        &self.endpoint
    }
    pub async fn send(&mut self, message: &Value) -> Result<()> {
        let mut state = self.state.lock().await;
        state.snapshot.apply(message)?;
        state.events.send(Arc::new(message.clone()))?;
        Ok(())
    }
    pub async fn receive(&mut self) -> Result<Option<Value>> {
        Ok(self.input.recv().await.map(|entry| entry.into_parts().0))
    }
    pub async fn close(&mut self) {
        self.stop.cancel();
        // Keep the handle until completion so cancelling close can be retried.
        if let Some(task) = self.task.as_mut() {
            let _ = task.await;
        }
        self.task.take();
        self.input.close();
    }
}
impl Drop for LocalTransport {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
async fn connection(
    stream: LocalStream,
    state: Arc<Mutex<State>>,
    input: queue::Sender<Value>,
) -> Result<()> {
    let (snapshot, mut events) = {
        let state = state.lock().await;
        (state.snapshot.messages(), state.events.subscribe())
    };
    let (mut writer, mut reader) =
        Framed::new(stream, LinesCodec::new_with_max_length(MAX_FRAME_BYTES)).split();
    let write = async {
        for message in snapshot {
            tokio::time::timeout(Duration::from_secs(5), writer.send(message.to_string()))
                .await??;
        }
        loop {
            let message = events.recv().await?;
            tokio::time::timeout(Duration::from_secs(5), writer.send(message.to_string()))
                .await??;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    let read = async {
        while let Some(line) = reader.next().await {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(&line)?;
            if value.get("action").is_some() {
                input.try_send(value, line.len())?;
            }
        }
        Ok::<(), anyhow::Error>(())
    };
    tokio::select! { result = write => result, result = read => result }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use omochi_ui::LocalServerTransport;
    use std::{
        fs,
        os::unix::fs::{MetadataExt, PermissionsExt},
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    fn private_root() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        root
    }

    #[test]
    fn releasing_ownership_does_not_wait_for_inherited_file_descriptions() {
        let root = private_root();
        let owner = ServiceLock::acquire(root.path(), "shared").unwrap();
        // dup and fork both retain the same open file description. Keep that
        // reference alive deterministically, as a child would before exec closes
        // its CLOEXEC descriptors, without relying on subprocess scheduling.
        let inherited = owner.0.file().try_clone().unwrap();
        assert!(ServiceLock::acquire(root.path(), "shared").is_err());
        drop(owner);
        let successor = ServiceLock::acquire(root.path(), "shared").unwrap();
        drop(inherited);
        assert!(ServiceLock::acquire(root.path(), "shared").is_err());
        drop(successor);
        ServiceLock::acquire(root.path(), "shared").unwrap();
    }

    #[tokio::test]
    async fn sdk_and_utility_share_the_persistent_lock_and_allow_successors_after_close() {
        let root = private_root();
        let options = LocalServerOptions::new("shared").with_runtime_dir(root.path());
        let lock_path = root.path().join(".svc-shared.lock~");
        let mut utility = LocalTransport::bind(options.clone()).await.unwrap();
        let lock_metadata = fs::metadata(&lock_path).unwrap();
        assert_eq!(lock_metadata.mode() & 0o777, 0o600);
        let inode = (lock_metadata.dev(), lock_metadata.ino());
        assert!(LocalServerTransport::bind(options.clone()).await.is_err());
        assert!(LocalTransport::bind(options.clone()).await.is_err());
        utility.close().await;

        let mut sdk = LocalServerTransport::bind(options.clone()).await.unwrap();
        utility.close().await;
        drop(utility);
        assert!(LocalTransport::bind(options.clone()).await.is_err());
        UnixStream::connect(sdk.socket_path()).await.unwrap();
        assert!(root.path().join("svc-shared~/manifest.json").exists());
        sdk.close().await;

        let mut successor = LocalTransport::bind(options).await.unwrap();
        drop(sdk);
        UnixStream::connect(successor.socket_path()).await.unwrap();
        successor.close().await;
        let metadata = fs::metadata(lock_path).unwrap();
        assert_eq!((metadata.dev(), metadata.ino()), inode);
    }

    #[tokio::test]
    async fn shared_lock_excludes_sdk_and_utility_before_a_socket_exists() {
        let root = private_root();
        omochi_local::LocalRuntime::prepare(Some(root.path().to_path_buf())).unwrap();
        let lock = ServiceLock::acquire(root.path(), "shared").unwrap();
        let options = LocalServerOptions::new("shared").with_runtime_dir(root.path());
        assert!(LocalTransport::bind(options.clone()).await.is_err());
        assert!(LocalServerTransport::bind(options.clone()).await.is_err());
        assert!(!root.path().join("shared").exists());
        drop(lock);
        let mut transport = LocalTransport::bind(options).await.unwrap();
        transport.close().await;
    }

    #[tokio::test]
    async fn cancelled_close_keeps_ownership_until_listener_and_files_are_cleaned() {
        use std::{future::Future, task::Context};
        let root = private_root();
        let options = LocalServerOptions::new("shared").with_runtime_dir(root.path());
        let mut transport = LocalTransport::bind(options.clone()).await.unwrap();
        let mut closing = Box::pin(transport.close());
        assert!(
            closing
                .as_mut()
                .poll(&mut Context::from_waker(std::task::Waker::noop()))
                .is_pending()
        );
        drop(closing);
        // The current-thread supervisor has not processed cancellation yet.
        assert!(LocalTransport::bind(options.clone()).await.is_err());
        assert!(LocalServerTransport::bind(options.clone()).await.is_err());
        assert!(root.path().join("svc-shared~/manifest.json").exists());
        transport.close().await;
        assert!(!root.path().join("shared").exists());
        let mut successor = LocalTransport::bind(options).await.unwrap();
        drop(transport);
        UnixStream::connect(successor.socket_path()).await.unwrap();
        successor.close().await;
    }

    #[tokio::test]
    async fn aborted_supervisor_retracts_publication_before_releasing_ownership() {
        let root = private_root();
        let options = LocalServerOptions::new("shared").with_runtime_dir(root.path());
        let mut transport = LocalTransport::bind(options.clone()).await.unwrap();
        let task = transport.task.take().unwrap();
        task.abort();
        drop(transport);
        assert!(LocalTransport::bind(options.clone()).await.is_err());
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(!root.path().join("shared").exists());
        let mut successor = LocalServerTransport::bind(options).await.unwrap();
        successor.close().await;
    }

    /// Driven by tests/sdk/socket_ownership.py using stdin/stdout barriers.
    #[tokio::test]
    #[ignore]
    async fn ownership_process_fixture() {
        use std::io::Write;
        let root = std::env::var_os("OMOCHI_OWNER_ROOT");
        let announce = |message| {
            println!("OWNER {message}");
            std::io::stdout().flush().unwrap();
        };
        let command = || {
            let mut line = String::new();
            assert_ne!(std::io::stdin().read_line(&mut line).unwrap(), 0);
        };
        announce("WAITING");
        command();
        let mut options = LocalServerOptions::new("demo");
        if let Some(root) = root {
            options = options.with_runtime_dir(root);
        }
        let Ok(mut transport) = LocalTransport::bind(options).await else {
            announce("REJECTED");
            return;
        };
        announce("READY");
        command();
        transport.close().await;
        announce("CLOSED");
        command();
        transport.close().await;
    }

    #[tokio::test]
    async fn a_slow_reader_is_disconnected_after_exceeding_the_byte_budget() {
        let root = tempfile::tempdir().unwrap();
        let options = LocalServerOptions::new("slow").with_runtime_dir(root.path().join("private"));
        let mut transport = LocalTransport::bind(options).await.unwrap();
        let mut slow = UnixStream::connect(transport.socket_path()).await.unwrap();
        // Receiving an action proves the connection has taken its subscription.
        slow.write_all(b"{\"action\":{\"name\":\"ready\"}}\n")
            .await
            .unwrap();
        transport.receive().await.unwrap().unwrap();
        transport.send(&serde_json::json!({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"}})).await.unwrap();
        let message = serde_json::json!({"version":"v0.9","updateDataModel":{"surfaceId":"main","value":{"text":"x".repeat(1024 * 1024)}}});
        // Fewer than the 32-entry cap: byte pressure alone must cause eviction.
        for _ in 0..20 {
            transport.send(&message).await.unwrap();
        }
        let mut drained = Vec::new();
        tokio::time::timeout(Duration::from_secs(3), slow.read_to_end(&mut drained))
            .await
            .unwrap()
            .unwrap();
        transport.close().await;
    }
    #[tokio::test]
    async fn duplicate_owners_oversized_input_and_idle_clients_are_closed_safely() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let options = LocalServerOptions::new("native").with_runtime_dir(root.path());
        let mut transport = LocalTransport::bind(options.clone()).await.unwrap();
        assert!(LocalTransport::bind(options).await.is_err());
        let mut oversized = UnixStream::connect(transport.socket_path()).await.unwrap();
        let line = vec![b'x'; MAX_FRAME_BYTES + 1];
        let _ = tokio::time::timeout(Duration::from_secs(5), oversized.write_all(&line))
            .await
            .unwrap();
        let mut byte = [0];
        let result = tokio::time::timeout(Duration::from_secs(5), oversized.read(&mut byte))
            .await
            .unwrap();
        assert!(matches!(result, Ok(0)) || result.is_err());
        let mut idle = UnixStream::connect(transport.socket_path()).await.unwrap();
        idle.write_all(b"{\"action\":{\"surfaceId\":\"main\",\"name\":\"click\"}}\n")
            .await
            .unwrap();
        assert_eq!(
            transport.receive().await.unwrap().unwrap()["action"]["name"],
            "click"
        );
        let socket = transport.socket_path().to_owned();
        transport.close().await;
        assert_eq!(idle.read(&mut byte).await.unwrap(), 0);
        assert!(!socket.exists());
        assert!(!root.path().join("svc-native~/manifest.json").exists());
    }
    #[tokio::test]
    async fn refuses_shared_runtime_and_does_not_delete_replaced_manifest() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o777)).unwrap();
        let options = LocalServerOptions::new("native").with_runtime_dir(root.path());
        assert!(LocalTransport::bind(options.clone()).await.is_err());
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let mut transport = LocalTransport::bind(options).await.unwrap();
        let directory = root.path().join("svc-native~");
        fs::write(directory.join("replacement"), "replacement").unwrap();
        fs::rename(
            directory.join("replacement"),
            directory.join("manifest.json"),
        )
        .unwrap();
        transport.close().await;
        assert_eq!(
            fs::read_to_string(directory.join("manifest.json")).unwrap(),
            "replacement"
        );
    }
}

#[cfg(all(test, unix))]
struct ServiceLock(omochi_local::files::FileLock);
#[cfg(all(test, unix))]
impl ServiceLock {
    fn acquire(root: &std::path::Path, id: &str) -> std::io::Result<Self> {
        Ok(Self(omochi_local::files::FileLock::acquire(
            &root.join(format!(".svc-{id}.lock~")),
            true,
        )?))
    }
}
#[cfg(all(test, unix))]
trait SocketPath {
    fn socket_path(&self) -> &std::path::Path;
}
#[cfg(all(test, unix))]
impl SocketPath for LocalTransport {
    fn socket_path(&self) -> &std::path::Path {
        match self.endpoint() {
            LocalEndpoint::Unix { path } => path,
            _ => unreachable!(),
        }
    }
}
#[cfg(all(test, unix))]
impl SocketPath for omochi_ui::LocalServerTransport {
    fn socket_path(&self) -> &std::path::Path {
        match self.endpoint() {
            LocalEndpoint::Unix { path } => path,
            _ => unreachable!(),
        }
    }
}
