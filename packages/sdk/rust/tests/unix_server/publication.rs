use super::*;
use serde_json::json;
use std::time::Duration;

fn temp_root() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("om-pub-")
        .tempdir()
        .unwrap()
}

async fn assert_atomic_subscription(publish_first: bool, update_existing: bool) {
    let root = temp_root();
    let mut transport =
        LocalServerTransport::bind(LocalServerOptions::new("demo").with_runtime_dir(root.path()))
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
            stream: LocalStream::from_io(server),
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

#[tokio::test(flavor = "current_thread")]
async fn aborted_supervisor_cannot_release_a_connection_future_publication() {
    let root = temp_root();
    let options = LocalServerOptions::new("demo").with_runtime_dir(root.path());
    let mut owner = LocalServerTransport::bind(options.clone()).await.unwrap();
    let (server, mut client) = UnixStream::pair().unwrap();
    let mut connection = Box::pin(serve_connection(
        ServiceConnection {
            stream: LocalStream::from_io(server),
            _publication: owner.service_publication.upgrade().unwrap(),
        },
        owner.shared.clone(),
        owner.inbound.clone(),
    ));
    assert!(futures::poll!(&mut connection).is_pending());
    owner.accept.as_ref().unwrap().abort();
    while !owner.accept.as_ref().unwrap().is_finished() {
        tokio::task::yield_now().await;
    }
    let mut closing = Box::pin(owner.close());
    assert!(futures::poll!(&mut closing).is_pending());
    drop(closing);
    assert!(LocalServerTransport::bind(options.clone()).await.is_err());
    drop(connection);
    tokio::time::timeout(Duration::from_secs(5), owner.close())
        .await
        .unwrap();
    assert_eq!(client.read(&mut [0]).await.unwrap(), 0);
    let mut successor = LocalServerTransport::bind(options).await.unwrap();
    drop(owner);
    assert!(successor.directory().join("manifest.json").exists());
    successor.close().await;
}
