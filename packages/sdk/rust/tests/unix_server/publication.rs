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
