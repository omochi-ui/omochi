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
    let inherited = owner.service_lock.as_ref().unwrap().0.try_clone().unwrap();
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
    let (inbound, mut actions) = mpsc::unbounded_channel();
    let mut connection = Box::pin(serve_connection(server, Arc::clone(&shared), inbound));
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
    client
        .get_mut()
        .write_all(b"{\"action\":{\"name\":\"ready\"}}\n")
        .await
        .unwrap();
    assert_eq!(actions.recv().await.unwrap()["action"]["name"], "ready");
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
