use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self(std::env::temp_dir().join(format!(
            "omochi-publication-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn close_releases_ownership_with_an_inherited_file_description() {
    let root = TempRoot::new();
    let options = UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0);
    let mut owner = UnixSocketServerTransport::bind(options.clone())
        .await
        .unwrap();
    // dup retains the same open file description as a fork before exec. Hold it
    // through close to reproduce inherited flock ownership without scheduling races.
    let inherited = owner
        .service_publication
        .upgrade()
        .unwrap()
        ._service_lock
        .0
        .try_clone()
        .unwrap();
    owner.close().await;
    let mut successor = UnixSocketServerTransport::bind(options.clone())
        .await
        .unwrap();
    drop(inherited);
    drop(owner);
    assert!(UnixSocketServerTransport::bind(options).await.is_err());
    UnixStream::connect(successor.socket_path()).await.unwrap();
    successor.close().await;
}

async fn ready_owner(
    options: UnixSocketServerOptions,
) -> (UnixSocketServerTransport, BufReader<UnixStream>) {
    let mut owner = UnixSocketServerTransport::bind(options).await.unwrap();
    owner.set_resources(ResourceRegistry::default());
    let mut client = BufReader::new(UnixStream::connect(owner.socket_path()).await.unwrap());
    let mut line = String::new();
    client.read_line(&mut line).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), hello());
    (owner, client)
}

#[tokio::test(flavor = "current_thread")]
async fn dropped_owner_holds_ownership_until_accepted_clients_close() {
    use std::os::unix::fs::MetadataExt;

    for cancel_close in [false, true] {
        let root = TempRoot::new();
        let options = UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0);
        let (mut owner, mut client) = ready_owner(options.clone()).await;
        let lock_inode = fs::metadata(root.0.join(".demo.lock~")).unwrap().ino();
        let mut finished = std::mem::replace(&mut owner.finished, oneshot::channel().1);
        if cancel_close {
            let mut closing = Box::pin(owner.close());
            assert!(futures::poll!(&mut closing).is_pending());
            drop(closing);
        }
        drop(owner);

        // No scheduler yield: the supervisor and its accepted connection are still
        // alive. Drop must leave the lock held until they actually release sockets.
        assert_eq!(
            client.get_ref().try_read(&mut [0]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert!(futures::poll!(&mut finished).is_pending());
        assert!(
            UnixSocketServerTransport::bind(options.clone())
                .await
                .is_err()
        );
        assert!(root.0.join("demo/manifest.json").exists());
        tokio::time::timeout(Duration::from_secs(5), &mut finished)
            .await
            .expect("dropped owner must finish shutdown")
            .unwrap();
        assert_eq!(client.read(&mut [0]).await.unwrap(), 0);
        assert!(!root.0.join("demo").exists());

        let mut successor = UnixSocketServerTransport::bind(options).await.unwrap();
        assert_eq!(
            fs::metadata(root.0.join(".demo.lock~")).unwrap().ino(),
            lock_inode
        );
        UnixStream::connect(successor.socket_path()).await.unwrap();
        successor.close().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn aborted_supervisor_keeps_ownership_until_connection_future_is_dropped() {
    let root = TempRoot::new();
    let options = UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0);
    let mut owner = UnixSocketServerTransport::bind(options.clone())
        .await
        .unwrap();
    let (server, mut client) = UnixStream::pair().unwrap();
    // Hold an accepted connection's future outside the scheduler so cancellation
    // of the supervisor cannot accidentally race ahead of our ownership assertion.
    let mut connection = Box::pin(serve_connection(
        ServiceConnection {
            stream: server,
            _publication: owner.service_publication.upgrade().unwrap(),
        },
        Arc::clone(&owner.shared),
        Arc::clone(&owner.inbound),
    ));
    assert!(futures::poll!(&mut connection).is_pending());
    owner.accept.as_ref().unwrap().abort();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !owner.accept.as_ref().unwrap().is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("supervisor must finish cancellation");
    let mut closing = Box::pin(owner.close());
    assert!(futures::poll!(&mut closing).is_pending());
    // close has joined the supervisor but is still waiting for socket cleanup.
    // Cancelling here must allow a second close to resume that final wait.
    drop(closing);
    assert!(owner.accept.is_none());
    assert!(
        UnixSocketServerTransport::bind(options.clone())
            .await
            .is_err()
    );
    assert!(root.0.join("demo/manifest.json").exists());
    drop(connection);
    owner.close().await;
    assert_eq!(client.read(&mut [0]).await.unwrap(), 0);

    let mut successor = UnixSocketServerTransport::bind(options).await.unwrap();
    owner.close().await;
    drop(owner);
    assert!(successor.directory().join("manifest.json").exists());
    UnixStream::connect(successor.socket_path()).await.unwrap();
    successor.close().await;
}

#[tokio::test(flavor = "current_thread")]
async fn aborted_supervisor_closes_accepted_clients_before_close_completes() {
    let root = TempRoot::new();
    let options = UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0);
    let (mut owner, mut client) = ready_owner(options.clone()).await;
    owner.accept.as_ref().unwrap().abort();
    owner.close().await;
    assert_eq!(client.read(&mut [0]).await.unwrap(), 0);
    let mut successor = UnixSocketServerTransport::bind(options).await.unwrap();
    drop(owner);
    UnixStream::connect(successor.socket_path()).await.unwrap();
    successor.close().await;
}

#[tokio::test]
async fn manifest_failure_cleans_bound_socket_before_releasing_ownership() {
    let root = TempRoot::new();
    let options = UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0);
    fs::create_dir_all(root.0.join("demo/manifest.json")).unwrap();
    // Publishing the manifest cannot rename a file over this directory.
    assert!(
        UnixSocketServerTransport::bind(options.clone())
            .await
            .is_err()
    );
    assert!(!root.0.join("demo").exists());
    let mut successor = UnixSocketServerTransport::bind(options).await.unwrap();
    UnixStream::connect(successor.socket_path()).await.unwrap();
    successor.close().await;
}

async fn assert_atomic_subscription(publish_first: bool, update_existing: bool) {
    let root = TempRoot::new();
    let mut transport = UnixSocketServerTransport::bind(
        UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0),
    )
    .await
    .unwrap();
    let create = json!({
        "version": "v0.9",
        "createSurface": { "surfaceId": "main", "catalogId": "test" }
    });
    let update = json!({
        "version": "v0.9",
        "updateDataModel": { "surfaceId": "main", "path": "/", "value": { "count": 1 } }
    });
    if update_existing {
        transport.send(&create).await.unwrap();
    }
    let message = if update_existing { &update } else { &create };
    let shared = Arc::clone(&transport.shared);
    let (server, client) = UnixStream::pair().unwrap();
    let actions = Arc::new(InboundQueue::new());
    let inbound = actions.clone();
    let mut connection = Box::pin(serve_connection(
        ServiceConnection {
            stream: server,
            _publication: transport.service_publication.upgrade().unwrap(),
        },
        Arc::clone(&shared),
        inbound,
    ));
    let mut publishing = Box::pin(transport.send(message));

    // Poll both operations against a held lock to establish their FIFO order.
    // Splitting either transaction into two lock acquisitions then lets the other
    // operation run in the gap, deterministically exposing duplicates or omissions.
    let publication = shared.publication.lock().await;
    if publish_first {
        assert!(futures::poll!(&mut publishing).is_pending());
        assert!(futures::poll!(&mut connection).is_pending());
    } else {
        assert!(futures::poll!(&mut connection).is_pending());
        assert!(futures::poll!(&mut publishing).is_pending());
    }
    drop(publication);
    let connection = tokio::spawn(connection);
    publishing.await.unwrap();

    // An action is processed only after replay and registration have completed.
    // A following marker bounds the exact output prefix; no quiet-time assertion.
    let mut client = BufReader::new(client);
    let ready = json!({
        "version": "v0.9",
        "action": {
            "name": "ready", "surfaceId": "main", "sourceComponentId": "root",
            "timestamp": "2026-08-12T00:00:00.000Z", "context": {}
        }
    });
    client
        .get_mut()
        .write_all(format!("{ready}\n").as_bytes())
        .await
        .unwrap();
    assert_eq!(
        actions.receive().await.unwrap().unwrap()["action"]["name"],
        "ready"
    );
    let marker = json!({
        "version": "v0.9",
        "updateDataModel": { "surfaceId": "main", "path": "/", "value": "marker" }
    });
    transport.send(&marker).await.unwrap();
    let mut received = Vec::new();
    loop {
        let mut line = String::new();
        assert_ne!(client.read_line(&mut line).await.unwrap(), 0);
        let message: Value = serde_json::from_str(&line).unwrap();
        let finished = message == marker;
        received.push(message);
        if finished {
            break;
        }
    }
    let expected = if update_existing {
        vec![create, update, marker]
    } else {
        vec![create, marker]
    };
    assert_eq!(received, expected);
    let mut snapshot = A2uiSnapshot::new();
    for message in &received {
        snapshot.apply(message).unwrap();
    }
    assert_eq!(
        snapshot.messages(),
        shared.publication.lock().await.snapshot.messages()
    );

    drop(client);
    connection.await.unwrap();
    transport.close().await;
}

#[tokio::test]
async fn publication_before_subscription_delivers_each_message_once() {
    tokio::time::timeout(Duration::from_secs(5), async {
        for update_existing in [false, true] {
            assert_atomic_subscription(true, update_existing).await;
        }
    })
    .await
    .expect("publication and subscription must complete");
}

#[tokio::test]
async fn subscription_before_publication_delivers_each_message_once() {
    tokio::time::timeout(Duration::from_secs(5), async {
        for update_existing in [false, true] {
            assert_atomic_subscription(false, update_existing).await;
        }
    })
    .await
    .expect("subscription and publication must complete");
}

#[tokio::test]
async fn ownership_covers_probe_bind_and_cleanup() {
    let root = TempRoot::new();
    fs::create_dir_all(&root.0).unwrap();
    let lock = lock_service(&root.0, "demo").unwrap();
    let directory = root.0.join("demo");
    fs::create_dir(&directory).unwrap();
    let path = directory.join(SOCKET_FILE_NAME);
    // Explicit transition barriers: contenders finish while the owner is parked at
    // each boundary, so no scheduling delay can make this test accidentally pass.
    for phase in 0..3 {
        assert!(
            UnixSocketServerTransport::bind(
                UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0)
            )
            .await
            .is_err()
        );
        assert!(directory.exists());
        if phase == 0 {
            claim_socket_path(&path).await.unwrap();
        } else if phase == 1 {
            let listener = UnixListener::bind(&path).unwrap();
            assert!(
                UnixSocketServerTransport::bind(
                    UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0)
                )
                .await
                .is_err()
            );
            assert!(UnixStream::connect(&path).await.is_ok());
            drop(listener);
        }
    }
    fs::remove_dir_all(&directory).unwrap();
    assert!(
        UnixSocketServerTransport::bind(
            UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0)
        )
        .await
        .is_err()
    );
    drop(lock);
    let mut successor = UnixSocketServerTransport::bind(
        UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0),
    )
    .await
    .unwrap();
    successor.close().await;
    let mut next = UnixSocketServerTransport::bind(
        UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0),
    )
    .await
    .unwrap();
    successor.close().await;
    drop(successor);
    assert!(UnixStream::connect(next.socket_path()).await.is_ok());
    next.close().await;
}
