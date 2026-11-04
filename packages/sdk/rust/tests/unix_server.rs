use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use omochi_ui::{
    App, Error, ResourceRegistry, Transport, UnixSocketServerOptions, UnixSocketServerTransport,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

struct TempRoot(PathBuf);

impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "omochi-rust-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn read_messages(stream: &mut BufReader<UnixStream>, count: usize) -> Vec<Value> {
    let mut messages = Vec::new();
    while messages.len() < count {
        let mut line = String::new();
        let read = tokio::time::timeout(Duration::from_secs(5), stream.read_line(&mut line))
            .await
            .expect("timed out reading from the service")
            .expect("failed to read from the service");
        assert_ne!(read, 0, "the service closed the connection");
        if !line.trim().is_empty() {
            messages.push(serde_json::from_str(&line).unwrap());
        }
    }
    messages
}

async fn ready_resource_client(
    root: &TempRoot,
) -> (UnixSocketServerTransport, BufReader<UnixStream>, String) {
    let mut transport = UnixSocketServerTransport::bind(
        UnixSocketServerOptions::new("demo").with_runtime_dir(root.path()),
    )
    .await
    .unwrap();
    let resources = ResourceRegistry::default();
    let uri = resources.bytes(b"media", "video/mp4").unwrap();
    transport.set_resources(resources);
    transport
        .send(&json!({ "version": "v0.9", "createSurface": { "surfaceId": "main", "catalogId": "test" } }))
        .await
        .unwrap();
    let mut client = BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
    let replay = read_messages(&mut client, 2).await;
    assert_eq!(
        replay[0],
        json!({ "omochi": "resources/1", "kind": "hello" })
    );
    assert_eq!(replay[1]["createSurface"]["surfaceId"], "main");
    let request = format!(
        "{}\n",
        json!({ "omochi": "resources/1", "kind": "request", "id": "read", "op": "stat", "resource": uri.rsplit('/').next().unwrap() })
    );
    client
        .get_mut()
        .write_all(request.as_bytes())
        .await
        .unwrap();
    assert_eq!(read_messages(&mut client, 1).await[0]["status"], 200);
    (transport, client, request)
}

async fn assert_eof(client: &mut BufReader<UnixStream>) {
    let mut line = String::new();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), client.read_line(&mut line))
            .await
            .expect("shutdown must close the client socket")
            .unwrap(),
        0,
        "shutdown must not leave a resource response or replay writer alive: {line}"
    );
}

#[tokio::test]
async fn runtime_render_failure_releases_service_while_runtime_is_retained() {
    let root = TempRoot::new("runtime-fail");
    let options = UnixSocketServerOptions::new("demo").with_runtime_dir(root.path());
    let transport = UnixSocketServerTransport::bind(options.clone())
        .await
        .unwrap();
    let socket = transport.socket_path().to_owned();
    let directory = transport.directory().to_owned();
    let mut app = App::new(());
    app.surface("main", |_, _| {
        Err(Error::Declaration("initial render failed".into()))
    })
    .unwrap();
    let mut runtime = app.build();
    assert!(matches!(
        runtime.start_with(transport).await,
        Err(Error::Declaration(message)) if message == "initial render failed"
    ));
    assert!(runtime.is_closed());
    assert!(!directory.join("manifest.json").exists());
    assert!(!socket.exists());
    assert!(UnixStream::connect(&socket).await.is_err());

    // The retained Runtime must neither hold the ownership lock nor later remove
    // a successor's publication when shutdown or Drop runs again.
    let mut successor = UnixSocketServerTransport::bind(options).await.unwrap();
    runtime.shutdown().await;
    drop(runtime);
    assert!(directory.join("manifest.json").exists());
    assert!(UnixStream::connect(&socket).await.is_ok());
    successor.close().await;
}

#[tokio::test]
async fn runtime_shutdown_waits_for_owned_connections_to_stop() {
    let root = TempRoot::new("runtime-stop");
    let options = UnixSocketServerOptions::new("demo").with_runtime_dir(root.path());
    let transport = UnixSocketServerTransport::bind(options.clone())
        .await
        .unwrap();
    let socket = transport.socket_path().to_owned();
    let mut client = BufReader::new(UnixStream::connect(&socket).await.unwrap());
    let mut runtime = App::new(()).build();
    // Drive start until the client observes readiness, then cancel the receive
    // loop and explicitly shut down the retained runtime.
    tokio::select! {
        result = runtime.start_with(transport) => panic!("unexpected runtime exit: {result:?}"),
        _ = read_messages(&mut client, 1) => {},
    }
    runtime.shutdown().await;
    assert!(runtime.is_closed());
    assert!(!root.path().join("demo/manifest.json").exists());
    assert!(!socket.exists());
    // The initial hello can be sent both live and on subscription. Drain any
    // queued bytes, requiring EOF from the already accepted connection.
    let mut remaining = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(5),
        tokio::io::AsyncReadExt::read_to_end(&mut client, &mut remaining),
    )
    .await
    .expect("runtime shutdown must close every connection")
    .unwrap();
    let mut successor = UnixSocketServerTransport::bind(options).await.unwrap();
    drop(runtime);
    assert!(socket.exists());
    successor.close().await;
}

#[tokio::test]
async fn close_stops_connections_and_resource_responses() {
    let root = TempRoot::new("close");
    let (mut transport, mut client, request) = ready_resource_client(&root).await;
    transport.close().await;
    // A peer may observe the close on either its write or its read.
    let _ = client.get_mut().write_all(request.as_bytes()).await;
    assert_eof(&mut client).await;
    assert!(transport.receive().await.unwrap().is_none());
    transport.close().await;
    assert!(!root.path().join("demo").exists());
}

#[tokio::test]
async fn drop_stops_connections_and_resource_responses() {
    let root = TempRoot::new("drop");
    let (transport, mut client, request) = ready_resource_client(&root).await;
    drop(transport);
    assert_eof(&mut client).await;
    let _ = client.get_mut().write_all(request.as_bytes()).await;
    assert_eof(&mut client).await;
    assert!(!root.path().join("demo").exists());
}

#[tokio::test(flavor = "current_thread")]
async fn close_can_resume_after_its_future_is_cancelled() {
    let root = TempRoot::new("cancel-close");
    let (mut transport, mut client, request) = ready_resource_client(&root).await;
    let mut closing = Box::pin(transport.close());
    // Poll sends shutdown, but the supervisor cannot run until this task yields.
    assert!(futures::poll!(&mut closing).is_pending());
    drop(closing);
    transport.close().await;
    let _ = client.get_mut().write_all(request.as_bytes()).await;
    assert_eof(&mut client).await;
    assert!(!root.path().join("demo").exists());
}

#[tokio::test(flavor = "current_thread")]
async fn shutdown_rejects_a_connection_queued_before_accept_runs() {
    for use_drop in [false, true] {
        let root = TempRoot::new("queued");
        let mut transport = UnixSocketServerTransport::bind(
            UnixSocketServerOptions::new("demo").with_runtime_dir(root.path()),
        )
        .await
        .unwrap();
        transport.set_resources(ResourceRegistry::default());
        // No await between bind and connect: the spawned accept task has not run on
        // this single-threaded runtime, but the kernel has queued this connection.
        let queued = std::os::unix::net::UnixStream::connect(transport.socket_path()).unwrap();
        queued.set_nonblocking(true).unwrap();
        let mut client = BufReader::new(UnixStream::from_std(queued).unwrap());
        if use_drop {
            drop(transport);
        } else {
            transport.close().await;
        }
        let mut line = String::new();
        let read = tokio::time::timeout(Duration::from_secs(5), client.read_line(&mut line))
            .await
            .expect("a queued connection must not survive shutdown");
        match read {
            Ok(bytes) => assert_eq!(bytes, 0, "must not accept and announce after shutdown"),
            Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset),
        }
    }
}

#[tokio::test]
async fn publishes_a_discoverable_service_and_fans_it_out() {
    let root = TempRoot::new("publish");
    let mut transport = UnixSocketServerTransport::bind(
        UnixSocketServerOptions::new("demo")
            .with_name("Demo")
            .with_icon("🎤")
            .with_runtime_dir(root.path()),
    )
    .await
    .unwrap();

    let directory = transport.directory().to_path_buf();
    let socket_path = transport.socket_path().to_path_buf();
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(directory.join("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(
        manifest,
        json!({ "name": "Demo", "icon": "🎤", "socket": "a2ui.sock" })
    );
    let mode = std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);

    // The message sequence a Runtime produces for one surface.
    for message in [
        json!({ "version": "v0.9", "createSurface": { "surfaceId": "main", "catalogId": "test", "sendDataModel": true } }),
        json!({ "version": "v0.9", "updateComponents": { "surfaceId": "main", "components": [{ "id": "count", "component": "Text", "text": "Count: 0" }] } }),
        json!({ "version": "v0.9", "updateDataModel": { "surfaceId": "main", "path": "/", "value": { "count": 0 } } }),
    ] {
        transport.send(&message).await.unwrap();
    }

    let mut client = BufReader::new(UnixStream::connect(&socket_path).await.unwrap());
    let initial = read_messages(&mut client, 3).await;
    assert_eq!(initial[0]["createSurface"]["surfaceId"], json!("main"));

    client
        .get_mut()
        .write_all(
            format!(
                "{}\n",
                json!({
                    "version": "v0.9",
                    "action": {
                        "name": "increment",
                        "surfaceId": "main",
                        "sourceComponentId": "action:increment",
                        "timestamp": "2026-08-12T00:00:00.000Z",
                        "context": {},
                    },
                })
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let received = transport.receive().await.unwrap().unwrap();
    assert_eq!(received["action"]["name"], json!("increment"));

    for message in [
        json!({ "version": "v0.9", "updateComponents": { "surfaceId": "main", "components": [{ "id": "count", "component": "Text", "text": "Count: 1" }] } }),
        json!({ "version": "v0.9", "updateDataModel": { "surfaceId": "main", "path": "/count", "value": 1 } }),
    ] {
        transport.send(&message).await.unwrap();
    }
    let updates = read_messages(&mut client, 2).await;
    assert_eq!(
        updates[0]["updateComponents"]["components"][0]["text"],
        json!("Count: 1")
    );

    // A renderer joining later is caught up from the snapshot rather than the history.
    let mut late = BufReader::new(UnixStream::connect(&socket_path).await.unwrap());
    let replay = read_messages(&mut late, 3).await;
    assert_eq!(
        replay[1]["updateComponents"]["components"][0]["text"],
        json!("Count: 1")
    );
    assert_eq!(replay[2]["updateDataModel"]["value"], json!({ "count": 1 }));

    transport.close().await;
    assert!(!directory.exists());
}

#[tokio::test]
async fn refuses_a_live_duplicate_and_recovers_a_stale_socket() {
    let root = TempRoot::new("duplicate");
    let mut first = UnixSocketServerTransport::bind(
        UnixSocketServerOptions::new("demo").with_runtime_dir(root.path()),
    )
    .await
    .unwrap();

    let duplicate = UnixSocketServerTransport::bind(
        UnixSocketServerOptions::new("demo").with_runtime_dir(root.path()),
    )
    .await;
    assert!(duplicate.is_err(), "a second instance must be refused");

    first.close().await;
    assert!(!root.path().join("demo").exists());

    // A socket file left behind by a crash must not block the next start.
    std::fs::create_dir_all(root.path().join("demo")).unwrap();
    std::fs::write(root.path().join("demo/a2ui.sock"), b"").unwrap();
    let mut restarted = UnixSocketServerTransport::bind(
        UnixSocketServerOptions::new("demo").with_runtime_dir(root.path()),
    )
    .await
    .unwrap();
    assert!(restarted.socket_path().exists());
    restarted.close().await;
}

#[tokio::test]
async fn rejects_a_service_id_that_would_escape_the_runtime_directory() {
    let root = TempRoot::new("ids");
    for service_id in [".", "..", "a/b", "", &"x".repeat(65)] {
        let result = UnixSocketServerTransport::bind(
            UnixSocketServerOptions::new(service_id).with_runtime_dir(root.path()),
        )
        .await;
        assert!(result.is_err(), "{service_id:?} must be rejected");
    }
}

#[tokio::test]
async fn drops_client_messages_that_carry_no_action() {
    let root = TempRoot::new("errors");
    let mut transport = UnixSocketServerTransport::bind(
        UnixSocketServerOptions::new("demo").with_runtime_dir(root.path()),
    )
    .await
    .unwrap();
    transport
        .send(&json!({ "version": "v0.9", "createSurface": { "surfaceId": "main", "catalogId": "test" } }))
        .await
        .unwrap();

    let mut client = BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
    read_messages(&mut client, 1).await;
    let error = json!({ "version": "v0.9", "error": { "code": "VALIDATION_FAILED", "surfaceId": "main", "path": "/x", "message": "bad" } });
    let action = json!({ "version": "v0.9", "action": { "name": "run", "surfaceId": "main", "sourceComponentId": "root", "timestamp": "2026-08-12T00:00:00.000Z", "context": {} } });
    client
        .get_mut()
        .write_all(format!("{error}\n{action}\n").as_bytes())
        .await
        .unwrap();

    // The client error is dropped, so the Runtime sees the action next.
    let received = transport.receive().await.unwrap().unwrap();
    assert_eq!(received["action"]["name"], json!("run"));
    transport.close().await;
}

/// Driven by tests/sdk/socket_ownership.py, never run without its stdin barriers.
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
    let mut options = UnixSocketServerOptions::new("demo");
    if let Some(root) = root {
        options = options.with_runtime_dir(root);
    }
    let Ok(mut transport) = UnixSocketServerTransport::bind(options).await else {
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
async fn isolates_malformed_client_frames() {
    let actions: Value = serde_json::from_str(include_str!(
        "../../../../spec/fixtures/client-actions.json"
    ))
    .unwrap();
    let errors: Value =
        serde_json::from_str(include_str!("../../../../spec/fixtures/client-errors.json")).unwrap();
    let root = TempRoot::new("frames");
    let (mut transport, mut good, request) = ready_resource_client(&root).await;

    async fn verify(
        transport: &mut UnixSocketServerTransport,
        good: &mut BufReader<UnixStream>,
        request: &str,
        action: &Value,
    ) {
        good.get_mut()
            .write_all(format!("{action}\n").as_bytes())
            .await
            .unwrap();
        let mut expected = action.clone();
        expected.as_object_mut().unwrap().remove("type");
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), transport.receive())
                .await
                .unwrap()
                .unwrap(),
            Some(expected)
        );
        good.get_mut().write_all(request.as_bytes()).await.unwrap();
        assert_eq!(read_messages(good, 1).await[0]["status"], 200);
    }

    for message in actions["invalid"]
        .as_array()
        .unwrap()
        .iter()
        .chain(errors["invalid"].as_array().unwrap())
    {
        let mut bad = BufReader::new(UnixStream::connect(transport.socket_path()).await.unwrap());
        read_messages(&mut bad, 2).await;
        let mut tail = actions["actions"][0].clone();
        tail["action"]["name"] = json!("disconnected-peer");
        bad.get_mut()
            .write_all(format!("{message}\n{tail}\n").as_bytes())
            .await
            .unwrap();
        assert_eof(&mut bad).await;
        verify(&mut transport, &mut good, &request, &actions["actions"][0]).await;
    }
    for report in errors["reports"].as_array().unwrap() {
        good.get_mut()
            .write_all(format!("{report}\n").as_bytes())
            .await
            .unwrap();
        verify(&mut transport, &mut good, &request, &actions["actions"][0]).await;
    }
    for action in actions["actions"].as_array().unwrap() {
        verify(&mut transport, &mut good, &request, action).await;
    }
    transport.close().await;
}
