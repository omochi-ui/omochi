use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self(std::env::temp_dir().join(format!(
            "omochi-inbound-{}-{}",
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

fn action(sequence: usize) -> Value {
    json!({"version":"v0.9", "action":{"name":"work", "surfaceId":"main",
        "sourceComponentId":"root", "timestamp":"now", "context":{"sequence":sequence}}})
}

fn assert_overflow(result: Result<Option<Value>>) {
    assert!(matches!(result, Err(Error::Transport(message)) if message == RECEIVE_OVERFLOW));
}

async fn read_message(client: &mut BufReader<UnixStream>) -> Value {
    let mut line = String::new();
    assert_ne!(client.read_line(&mut line).await.unwrap(), 0);
    serde_json::from_str(&line).unwrap()
}

async fn send(client: &mut BufReader<UnixStream>, message: &Value) {
    client
        .get_mut()
        .write_all(format!("{message}\n").as_bytes())
        .await
        .unwrap();
}

async fn resource_barrier(client: &mut BufReader<UnixStream>) {
    send(
        client,
        &json!({"omochi":"resources/1", "kind":"request", "id":"barrier",
        "op":"stat", "resource":"00000000000000000000000000000000"}),
    )
    .await;
    let response = read_message(client).await;
    assert_eq!(response["id"], "barrier");
    assert_eq!(response["status"], 404);
}

#[tokio::test]
async fn count_overflow_discards_every_pending_action_and_latches_failure() {
    let queue = InboundQueue::new();
    for sequence in 0..MAX_PENDING_MESSAGES {
        assert!(queue.push(action(sequence)));
    }
    assert!(!queue.push(action(MAX_PENDING_MESSAGES)));
    assert_eq!(queue.state.lock().unwrap().entries.len(), 0);
    assert_eq!(queue.state.lock().unwrap().bytes, 0);
    assert_overflow(queue.receive().await);
    assert!(!queue.push(action(999)));
    queue.close();
    assert_overflow(queue.receive().await);
}

#[tokio::test]
async fn compact_utf8_bytes_are_released_on_receive_and_fail_at_the_exact_limit() {
    let queue = InboundQueue::new();
    let unicode = json!("猫🎤"); // Seven UTF-8 bytes, plus two JSON quotes.
    assert!(queue.push(unicode.clone()));
    assert_eq!(queue.state.lock().unwrap().bytes, 9);
    assert_eq!(queue.receive().await.unwrap(), Some(unicode.clone()));
    assert_eq!(queue.state.lock().unwrap().bytes, 0);
    assert!(queue.push(unicode));
    assert!(queue.push(json!("x".repeat(MAX_PENDING_BYTES - 9 - 2))));
    assert_eq!(queue.state.lock().unwrap().bytes, MAX_PENDING_BYTES);
    assert!(!queue.push(json!(0)));
    assert_overflow(queue.receive().await);

    let queue = InboundQueue::new();
    assert!(!queue.push(json!("x".repeat(MAX_PENDING_BYTES - 1))));
    assert_overflow(queue.receive().await);
}

#[tokio::test]
async fn resources_continue_at_capacity_and_accepted_actions_keep_their_order() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let root = TempRoot::new();
        let mut transport = UnixSocketServerTransport::bind(
            UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0),
        )
        .await
        .unwrap();
        transport.set_resources(ResourceRegistry::default());
        let mut client =
            BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
        assert_eq!(read_message(&mut client).await, hello());
        for sequence in 0..MAX_PENDING_MESSAGES {
            send(&mut client, &action(sequence)).await;
        }
        resource_barrier(&mut client).await;
        assert_eq!(
            transport.inbound.state.lock().unwrap().entries.len(),
            MAX_PENDING_MESSAGES
        );
        for sequence in 0..MAX_PENDING_MESSAGES {
            assert_eq!(transport.receive().await.unwrap(), Some(action(sequence)));
        }
        assert_eq!(transport.inbound.state.lock().unwrap().bytes, 0);
        send(&mut client, &action(999)).await;
        resource_barrier(&mut client).await;
        transport.close().await;
        assert!(
            transport.receive().await.unwrap().is_none(),
            "normal close discards pending actions"
        );
    })
    .await
    .expect("full action queue must not block resource responses");
}

#[tokio::test]
async fn byte_overflow_closes_all_clients_and_listener_before_receive_resumes() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let root = TempRoot::new();
        let options = UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0);
        let mut transport = UnixSocketServerTransport::bind(options.clone())
            .await
            .unwrap();
        transport.set_resources(ResourceRegistry::default());
        let mut clients = Vec::new();
        for _ in 0..2 {
            let mut client =
                BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
            assert_eq!(read_message(&mut client).await, hello());
            clients.push(client);
        }
        let mut large = action(0);
        large["action"]["context"]["payload"] = json!("猫".repeat(350_000));
        for _ in 0..3 {
            send(&mut clients[0], &large).await;
        }
        resource_barrier(&mut clients[0]).await;
        assert_eq!(transport.inbound.state.lock().unwrap().entries.len(), 3);
        // All clients share the application's pending budget.
        send(&mut clients[1], &large).await;
        while !transport.accept.as_ref().unwrap().is_finished() {
            tokio::task::yield_now().await;
        }
        for client in &mut clients {
            let mut tail = Vec::new();
            client.read_to_end(&mut tail).await.unwrap();
            assert!(tail.is_empty());
        }
        assert!(UnixStream::connect(transport.socket_path()).await.is_err());
        assert!(!transport.directory().join("manifest.json").exists());
        assert!(transport.shared.publication.lock().await.clients.is_empty());
        assert_overflow(transport.receive().await);
        transport.close().await;
        assert_overflow(transport.receive().await);
        let mut successor = UnixSocketServerTransport::bind(options).await.unwrap();
        drop(transport);
        assert!(successor.socket_path().exists());
        successor.close().await;
    })
    .await
    .expect("byte overflow must tear down clients without waiting for receive");
}

#[tokio::test]
async fn runtime_finishes_only_the_running_action_after_overflow() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let root = TempRoot::new();
        let options = UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0);
        let transport = UnixSocketServerTransport::bind(options.clone())
            .await
            .unwrap();
        let socket = transport.socket_path().to_owned();
        let inbound = transport.inbound.clone();
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let executions = Arc::new(AtomicUsize::new(0));
        let mut app = crate::App::new(());
        let (e, r, count) = (entered.clone(), release.clone(), executions.clone());
        app.action::<(), _, _>("work", move |_, _| {
            let (e, r, count) = (e.clone(), r.clone(), count.clone());
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                e.notify_one();
                r.notified().await;
                Ok(())
            }
        })
        .unwrap();
        let mut runtime = app.build();
        let observer = async {
            let mut client = BufReader::new(UnixStream::connect(&socket).await.unwrap());
            assert_eq!(read_message(&mut client).await, hello());
            send(&mut client, &action(0)).await;
            entered.notified().await;
            for sequence in 1..=MAX_PENDING_MESSAGES {
                send(&mut client, &action(sequence)).await;
            }
            // A full pending queue excludes the running action and still serves RPCs.
            resource_barrier(&mut client).await;
            send(&mut client, &action(MAX_PENDING_MESSAGES + 1)).await;
            while !inbound.state.lock().unwrap().overflow {
                tokio::task::yield_now().await;
            }
            release.notify_one();
        };
        let (result, ()) = tokio::join!(runtime.start_with(transport), observer);
        assert!(matches!(result, Err(Error::Transport(message)) if message == RECEIVE_OVERFLOW));
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert!(runtime.is_closed());
        assert!(!socket.exists());
        let mut successor = UnixSocketServerTransport::bind(options).await.unwrap();
        drop(runtime);
        assert!(socket.exists());
        successor.close().await;
    })
    .await
    .expect("overflow must abort pending actions after the running handler returns");
}

#[tokio::test]
async fn oversized_unterminated_frame_disconnects_only_its_sender() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let root = TempRoot::new();
        let mut transport = UnixSocketServerTransport::bind(
            UnixSocketServerOptions::new("demo").with_runtime_dir(&root.0),
        )
        .await
        .unwrap();
        transport.set_resources(ResourceRegistry::default());
        let mut bad = BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
        assert_eq!(read_message(&mut bad).await, hello());
        bad.get_mut()
            .write_all(&vec![b' '; MAX_INBOUND_FRAME_BYTES + 1])
            .await
            .unwrap();
        let mut tail = Vec::new();
        bad.read_to_end(&mut tail).await.unwrap();
        assert!(tail.is_empty());
        let mut healthy =
            BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
        assert_eq!(read_message(&mut healthy).await, hello());
        send(&mut healthy, &action(0)).await;
        resource_barrier(&mut healthy).await;
        assert_eq!(transport.receive().await.unwrap(), Some(action(0)));
        transport.close().await;
    })
    .await
    .expect("partial oversized frame must have a finite input buffer");
}
