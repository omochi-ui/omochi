use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::AsyncReadExt;

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self(std::env::temp_dir().join(format!(
            "omochi-outbound-{}-{}",
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

async fn read_message(client: &mut BufReader<UnixStream>) -> Value {
    let mut line = String::new();
    assert_ne!(client.read_line(&mut line).await.unwrap(), 0);
    serde_json::from_str(&line).unwrap()
}

async fn wait_closed(sender: &ClientSender) {
    while !sender.queue.is_closed() {
        tokio::task::yield_now().await;
    }
}

fn create(id: &str) -> Value {
    json!({"version":"v0.9", "createSurface":{"surfaceId":id, "catalogId":"test"}})
}

fn update(id: &str, value: Value) -> Value {
    json!({"version":"v0.9", "updateDataModel":{"surfaceId":id, "path":"/", "value":value}})
}

#[tokio::test]
async fn utf8_newlines_and_in_flight_frames_share_the_budget() {
    let (mut sender, mut receiver, mut cancelled) = ClientSender::new();
    sender.budget = Arc::new(Semaphore::new(9));
    let clone = sender.clone();
    assert!(sender.send("🎤\n")); // Five UTF-8 bytes, including the newline.
    let in_flight = receiver.recv().await.unwrap();
    assert_eq!(sender.budget.available_permits(), 4);
    assert!(clone.send("猫\n")); // Four bytes exactly fill the shared budget.
    let second = receiver.recv().await.unwrap();
    assert_eq!(sender.budget.available_permits(), 0);
    drop(in_flight);
    assert_eq!(sender.budget.available_permits(), 5);
    assert!(sender.send("🎤\n"));
    assert!(!clone.send("\n"));
    assert!(cancelled.has_changed().unwrap());
    cancelled.changed().await.unwrap();
    drop(second);
    drop(receiver.recv().await.unwrap());
    assert_eq!(sender.budget.available_permits(), 9);
    assert!(
        !sender.send("\n"),
        "overflow permanently closes this client"
    );
}

#[tokio::test]
async fn oversized_single_frames_cancel_without_queueing() {
    let (sender, mut receiver, cancelled) = ClientSender::new();
    assert!(!sender.send(&format!("{}\n", "x".repeat(MAX_OUTBOUND_BYTES))));
    assert!(cancelled.has_changed().unwrap());
    assert!(sender.budget.is_closed());
    assert_eq!(sender.budget.available_permits(), MAX_OUTBOUND_BYTES);
    assert!(receiver.try_recv().is_err());
}

#[tokio::test]
async fn paused_reader_is_disconnected_without_harming_healthy_clients() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let root = TempRoot::new();
        let options = LocalServerOptions::new("demo").with_runtime_dir(&root.0);
        let mut transport = LocalServerTransport::bind(options.clone()).await.unwrap();
        transport.set_resources(ResourceRegistry::default());
        transport.send(&create("main")).await.unwrap();
        let mut slow = BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
        assert_eq!(read_message(&mut slow).await, hello());
        assert_eq!(read_message(&mut slow).await, create("main"));
        let slow_sender = transport.shared.publication.lock().await.clients[0].clone();
        let mut healthy =
            BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
        assert_eq!(read_message(&mut healthy).await, hello());
        assert_eq!(read_message(&mut healthy).await, create("main"));

        let payload = "🎤".repeat(128 * 1024);
        for sequence in 0..64 {
            let message = update("main", json!({"sequence":sequence, "payload":payload}));
            transport.send(&message).await.unwrap();
            assert_eq!(read_message(&mut healthy).await, message);
            if slow_sender.budget.is_closed() {
                break;
            }
        }
        assert!(
            slow_sender.budget.is_closed(),
            "paused reader must exhaust its budget"
        );
        // Observe task teardown before resuming reads; dropping the queue sender
        // without cancelling a blocked write cannot pass this barrier.
        wait_closed(&slow_sender).await;
        assert_eq!(slow_sender.budget.available_permits(), MAX_OUTBOUND_BYTES);
        assert_eq!(transport.shared.publication.lock().await.clients.len(), 1);
        let mut partial_output = Vec::new();
        slow.read_to_end(&mut partial_output).await.unwrap();
        assert!(!partial_output.is_empty());

        let marker = update("main", json!("reconnected"));
        transport.send(&marker).await.unwrap();
        assert_eq!(read_message(&mut healthy).await, marker);
        let mut reconnect =
            BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
        assert_eq!(read_message(&mut reconnect).await, hello());
        assert_eq!(read_message(&mut reconnect).await, create("main"));
        assert_eq!(read_message(&mut reconnect).await, marker);
        let reconnect_sender = transport.shared.publication.lock().await.clients[1].clone();
        drop(reconnect);
        wait_closed(&reconnect_sender).await;
        assert_eq!(transport.shared.publication.lock().await.clients.len(), 1);

        transport.close().await;
        assert!(transport.shared.publication.lock().await.clients.is_empty());
        let mut successor = LocalServerTransport::bind(options).await.unwrap();
        successor.set_resources(ResourceRegistry::default());
        let mut next = BufReader::new(UnixStream::connect(successor.socket_path()).await.unwrap());
        assert_eq!(read_message(&mut next).await, hello());
        successor.close().await;
    })
    .await
    .expect("outbound overflow must promptly close only the slow connection");
}

#[tokio::test]
async fn large_bootstrap_streams_before_live_updates_and_resource_responses() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let root = TempRoot::new();
        let mut transport = LocalServerTransport::bind(
            LocalServerOptions::new("demo").with_runtime_dir(&root.0),
        ).await.unwrap();
        let resources = ResourceRegistry::default();
        let uri = resources.bytes(b"media", "video/mp4").unwrap();
        transport.set_resources(resources);
        let payload = "x".repeat(2 * 1024 * 1024);
        // Aggregate replay exceeds the queue limit, while each frame fits.
        for index in 0..9 {
            let id = format!("surface-{index}");
            transport.send(&create(&id)).await.unwrap();
            transport.send(&update(&id, json!(payload))).await.unwrap();
        }
        let mut client = BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
        assert_eq!(read_message(&mut client).await, hello());
        let sender = transport.shared.publication.lock().await.clients[0].clone();
        // A 2 MiB replay frame cannot fit the Unix socket's send buffer. Wait for
        // its reservation so subsequent traffic is certainly queued during replay.
        while sender.budget.available_permits() == MAX_OUTBOUND_BYTES {
            tokio::task::yield_now().await;
        }
        let marker = update("surface-0", json!("live"));
        transport.send(&marker).await.unwrap();
        let request = json!({"omochi":"resources/1", "kind":"request", "id":"stat", "op":"stat", "resource":uri.rsplit('/').next().unwrap()});
        let ready = json!({"version":"v0.9", "action":{"name":"ready", "surfaceId":"surface-0", "sourceComponentId":"root", "timestamp":"now", "context":{}}});
        client.get_mut().write_all(format!("{request}\n{ready}\n").as_bytes()).await.unwrap();
        assert_eq!(transport.receive().await.unwrap().unwrap(), ready);

        for index in 0..9 {
            let id = format!("surface-{index}");
            assert_eq!(read_message(&mut client).await, create(&id));
            assert_eq!(read_message(&mut client).await, update(&id, json!(payload)));
        }
        assert_eq!(read_message(&mut client).await, marker);
        let response = read_message(&mut client).await;
        assert_eq!(response["id"], "stat");
        assert_eq!(response["status"], 200);
        assert!(!sender.budget.is_closed());
        transport.close().await;
    }).await.expect("large snapshot must stream in order without overflowing the client");
}

#[tokio::test]
async fn active_replay_and_queued_live_frames_share_the_limit() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let root = TempRoot::new();
        let mut transport =
            LocalServerTransport::bind(LocalServerOptions::new("demo").with_runtime_dir(&root.0))
                .await
                .unwrap();
        transport.set_resources(ResourceRegistry::default());
        transport.send(&create("main")).await.unwrap();
        let replay = update("main", json!("x".repeat(9 * 1024 * 1024)));
        let replay_bytes = format!("{replay}\n").len();
        transport.send(&replay).await.unwrap();
        let mut client =
            BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
        assert_eq!(read_message(&mut client).await, hello());
        let sender = transport.shared.publication.lock().await.clients[0].clone();
        while sender.budget.available_permits() != MAX_OUTBOUND_BYTES - replay_bytes {
            tokio::task::yield_now().await;
        }
        let live = update("main", json!("y".repeat(8 * 1024 * 1024)));
        transport.send(&live).await.unwrap();
        assert!(sender.budget.is_closed());
        wait_closed(&sender).await;
        assert_eq!(sender.budget.available_permits(), MAX_OUTBOUND_BYTES);
        assert!(transport.shared.publication.lock().await.clients.is_empty());
        transport.close().await;
    })
    .await
    .expect("queued live data must count against a blocked replay frame");
}

#[tokio::test]
async fn resource_responses_share_the_budget_with_a_blocked_live_write() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let root = TempRoot::new();
        let mut transport =
            LocalServerTransport::bind(LocalServerOptions::new("demo").with_runtime_dir(&root.0))
                .await
                .unwrap();
        let resources = ResourceRegistry::default();
        let uri = resources
            .bytes(&vec![0; crate::resources::CHUNK as usize], "video/mp4")
            .unwrap();
        transport.set_resources(resources);
        let mut slow = BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
        assert_eq!(read_message(&mut slow).await, hello());
        let sender = transport.shared.publication.lock().await.clients[0].clone();
        // Resource envelopes sent by the runtime use the same fanout queue. This
        // large live frame stays in write_all while request responses accumulate.
        let live =
            json!({"omochi":"resources/1", "kind":"response", "data":"x".repeat(9 * 1024 * 1024)});
        transport.send(&live).await.unwrap();
        let request = json!({
            "omochi":"resources/1", "kind":"request", "id":"read", "op":"read",
            "resource":uri.rsplit('/').next().unwrap(), "offset":0, "length":crate::resources::CHUNK
        });
        slow.get_mut()
            .write_all(format!("{request}\n").repeat(256).as_bytes())
            .await
            .unwrap();
        wait_closed(&sender).await;
        assert!(sender.budget.is_closed());
        assert_eq!(sender.budget.available_permits(), MAX_OUTBOUND_BYTES);
        assert!(transport.shared.publication.lock().await.clients.is_empty());

        let mut healthy =
            BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
        assert_eq!(read_message(&mut healthy).await, hello());
        healthy
            .get_mut()
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        assert_eq!(read_message(&mut healthy).await["status"], 200);
        transport.close().await;
    })
    .await
    .expect("resource responses must cancel a full client without waiting for its writer");
}

#[tokio::test]
async fn oversized_bootstrap_frame_disconnects_and_releases_registration() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let root = TempRoot::new();
        let mut transport =
            LocalServerTransport::bind(LocalServerOptions::new("demo").with_runtime_dir(&root.0))
                .await
                .unwrap();
        transport.set_resources(ResourceRegistry::default());
        transport.send(&create("main")).await.unwrap();
        transport
            .send(&update("main", json!("x".repeat(MAX_OUTBOUND_BYTES))))
            .await
            .unwrap();
        let mut client =
            BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
        assert_eq!(read_message(&mut client).await, hello());
        assert_eq!(read_message(&mut client).await, create("main"));
        let mut tail = Vec::new();
        client.read_to_end(&mut tail).await.unwrap();
        assert!(
            tail.is_empty(),
            "oversized bootstrap must not be partially written"
        );
        assert!(transport.shared.publication.lock().await.clients.is_empty());
        transport.close().await;
    })
    .await
    .expect("an oversized bootstrap frame must close its connection");
}
