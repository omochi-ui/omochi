use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_omochi-ui"));
    command
        .arg("--config-dir")
        .arg(root.join("config"))
        .arg("--runtime-dir")
        .arg(root.join("runtime"));
    command.env("PATH", "/nonexistent").kill_on_drop(true);
    command
}
#[tokio::test]
async fn gateway_invalid_relay_allows_restart_in_the_same_runtime() {
    let root = tempfile::tempdir().unwrap();
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        command(root.path())
            .args(["gateway", "--remote", "--relay", "invalid"])
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("Failed to parse relay URL"), "{error}");
    assert!(!root.path().join("runtime/gateway.sock").exists());
    local_gateway_at(root.path()).await;
}

#[tokio::test]
async fn gateway_invitation_output_failure_cleans_up_and_allows_restart() {
    use std::os::{fd::OwnedFd, unix::net::UnixStream};

    let root = tempfile::tempdir().unwrap();
    let (stdout, reader) = UnixStream::pair().unwrap();
    drop(reader); // Fail invitation output after the local listener and remote endpoint bind.
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        command(root.path())
            .args(["gateway", "--remote", "--offline", "--pair"])
            .stdout(Stdio::from(OwnedFd::from(stdout)))
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
            .wait_with_output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains(&std::io::Error::from_raw_os_error(libc::EPIPE).to_string()),
        "{error}"
    );
    assert!(!root.path().join("runtime/gateway.sock").exists());
    local_gateway_at(root.path()).await;
    local_dashboard_at(root.path(), true).await;
}

#[tokio::test]
async fn gateway_startup_failure_preserves_an_unowned_socket() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    std::fs::create_dir(&runtime).unwrap();
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = runtime.join("gateway.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let before = std::fs::symlink_metadata(&socket).unwrap();
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        command(root.path()).args(["gateway", "--offline"]).output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("gateway socket is already present"),
        "{error}"
    );
    let after = std::fs::symlink_metadata(&socket).unwrap();
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
    let client = tokio::net::UnixStream::connect(&socket).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), listener.accept())
        .await
        .unwrap()
        .unwrap();
    drop(client);
}

async fn local_gateway_at(root: &Path) {
    use omochi_protocol::{OpenStream, VERSION, read_frame, session::Control, write_frame};

    let mut child = command(root)
        .args(["gateway", "--offline"])
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(line.starts_with("[omochi-ui] gateway "), "{line}");
    let socket = root.join("runtime/gateway.sock");
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        write_frame(&mut stream, &OpenStream::Control { protocol: VERSION })
            .await
            .unwrap();
        assert!(matches!(
            read_frame::<_, Control>(&mut stream).await.unwrap(),
            Some(Control::Hello {
                protocol: VERSION,
                ..
            })
        ));
    })
    .await
    .unwrap();
    assert_eq!(
        unsafe { libc::kill(child.id().unwrap() as i32, libc::SIGTERM) },
        0
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert!(!socket.exists());
}

#[tokio::test]
async fn native_hub_serves_bundled_dashboard_without_bun_and_cleans_up_owned_gateway() {
    local_dashboard(false).await;
}
#[tokio::test]
async fn offline_hub_preserves_local_http_and_websocket_without_remote_tools() {
    local_dashboard(true).await;
}
#[tokio::test]
async fn hub_bind_failure_cleans_up_owned_gateway_and_allows_restart() {
    let root = tempfile::tempdir().unwrap();
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = occupied.local_addr().unwrap().port().to_string();
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        command(root.path())
            .args(["hub", "--offline", "--port", &port])
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains(&std::io::Error::from_raw_os_error(libc::EADDRINUSE).to_string())
    );
    assert!(!root.path().join("runtime/gateway.sock").exists());
    drop(occupied);
    local_dashboard_at(root.path(), true).await;
}

#[tokio::test]
async fn hub_invalid_relay_exits_without_signal_and_allows_restart() {
    let root = tempfile::tempdir().unwrap();
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        command(root.path())
            .args(["hub", "--remote", "--relay", "not-a-url", "--port", "0"])
            .output(),
    )
    .await
    .expect("remote startup failure must terminate the hub without a signal")
    .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("Failed to parse relay URL"), "{error}");
    assert!(!root.path().join("runtime/gateway.sock").exists());
    local_dashboard_at(root.path(), true).await;
}

#[tokio::test]
async fn hub_remote_identity_failure_exits_without_signal_and_allows_restart() {
    let root = tempfile::tempdir().unwrap();
    omochi_transport::store::Store::open(root.path().join("config")).unwrap();
    let state_path = root.path().join("config/state.json");
    let original = std::fs::read(&state_path).unwrap();
    let mut state: serde_json::Value = serde_json::from_slice(&original).unwrap();
    state["hub_key"] = "".into();
    std::fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();

    let output = tokio::time::timeout(
        Duration::from_secs(10),
        command(root.path())
            .args(["hub", "--remote", "--port", "0"])
            .output(),
    )
    .await
    .expect("remote identity failure must terminate the hub without a signal")
    .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("invalid key length"), "{error}");
    assert!(!root.path().join("runtime/gateway.sock").exists());
    std::fs::write(state_path, original).unwrap();
    local_dashboard_at(root.path(), true).await;
}

#[tokio::test]
async fn hub_defers_remote_startup_until_gateway_registration_and_reports_failure() {
    let root = tempfile::tempdir().unwrap();
    let mut child = command(root.path())
        .args(["hub", "--relay", "not-a-url", "--port", "0"])
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let base = line.strip_prefix("[omochi-ui] dashboard ").unwrap();
    assert_eq!(
        reqwest::get(format!("{base}services"))
            .await
            .unwrap()
            .status(),
        200
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(600), child.wait())
            .await
            .is_err(),
        "an unpaired local hub must not start remote networking"
    );
    let store = omochi_transport::store::Store::open(root.path().join("config")).unwrap();
    let id = omochi_transport::SecretKey::from_bytes(&[42; 32]).public();
    store.register_gateway(id, "remote gateway").unwrap();
    assert!(
        !tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await
            .expect("lazy remote startup failure must terminate the hub without a signal")
            .unwrap()
            .success()
    );
    let error = lines.next_line().await.unwrap().unwrap();
    assert!(error.contains("Failed to parse relay URL"), "{error}");
    assert!(!root.path().join("runtime/gateway.sock").exists());
}

#[tokio::test]
async fn hub_bind_failure_preserves_an_existing_gateway() {
    let root = tempfile::tempdir().unwrap();
    let mut gateway = command(root.path())
        .args(["gateway", "--offline"])
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(gateway.stderr.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(line.starts_with("[omochi-ui] gateway "), "{line}");
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = occupied.local_addr().unwrap().port().to_string();
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        command(root.path())
            .args(["hub", "--offline", "--port", &port])
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!output.status.success());
    let socket = root.path().join("runtime/gateway.sock");
    assert!(socket.exists());
    assert!(gateway.try_wait().unwrap().is_none());
    tokio::net::UnixStream::connect(&socket).await.unwrap();
    assert_eq!(
        unsafe { libc::kill(gateway.id().unwrap() as i32, libc::SIGTERM) },
        0
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(10), gateway.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert!(!socket.exists());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn hub_store_lock_contention_keeps_single_worker_http_responsive() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let root = tempfile::tempdir().unwrap();
    let mut child = command(root.path())
        .env("TOKIO_WORKER_THREADS", "1")
        .args(["hub", "--port", "0", "--no-auto-gateway"])
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let base = line
        .strip_prefix("[omochi-ui] dashboard ")
        .unwrap()
        .trim_end_matches('/');
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let services = format!("{base}/services");
    assert_eq!(client.get(&services).send().await.unwrap().status(), 200);

    let lock_path = root.path().join("config/state.lock");
    let lock = tokio::time::timeout(
        Duration::from_secs(10),
        omochi_transport::blocking::run(move || omochi_transport::persistence::lock(&lock_path)),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    let pid = child.id().unwrap().to_string();
    // Observe the hub queued behind our lock before testing HTTP progress. Waiting
    // for an assumed number of poll intervals would not prove contention occurred.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let locks = std::fs::read_to_string("/proc/locks").unwrap();
            if locks.lines().any(|line| {
                line.split_whitespace().skip(1).take(5).eq([
                    "->",
                    "FLOCK",
                    "ADVISORY",
                    "WRITE",
                    pid.as_str(),
                ])
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("hub must wait for the held store lock");
    let response = client.get(&services).send().await;
    drop(lock);
    assert_eq!(client.get(&services).send().await.unwrap().status(), 200);
    assert_eq!(
        unsafe { libc::kill(child.id().unwrap() as i32, libc::SIGTERM) },
        0
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert_eq!(
        response
            .expect("store lock contention must not block local HTTP")
            .status(),
        200
    );
}
async fn local_dashboard(offline: bool) {
    let root = tempfile::tempdir().unwrap();
    local_dashboard_at(root.path(), offline).await;
}
async fn local_dashboard_at(root: &Path, offline: bool) {
    let mut child = command(root)
        .args(if offline {
            vec!["hub", "--offline", "--port", "0"]
        } else {
            vec!["hub", "--port", "0"]
        })
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let url = line
        .strip_prefix("[omochi-ui] dashboard http://")
        .unwrap()
        .trim_end_matches('/');
    for (path, expected) in [
        ("/", "<!doctype html>"),
        ("/services", "[]"),
        ("/api/presets", "[]"),
        ("/api/connections", "[]"),
    ] {
        let mut socket = tokio::net::TcpStream::connect(url).await.unwrap();
        socket
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).await.unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.to_lowercase().contains(expected), "{response}");
    }
    let (mut websocket, _) = tokio_tungstenite::connect_async(format!("ws://{url}/ws"))
        .await
        .unwrap();
    websocket.close(None).await.unwrap();
    drop(websocket);
    let reader_root = root.join("status-reader");
    let status = command(&reader_root)
        .args([
            "status",
            "--port",
            &url.parse::<std::net::SocketAddr>()
                .unwrap()
                .port()
                .to_string(),
        ])
        .output()
        .await
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&status.stdout).unwrap(),
        serde_json::json!([])
    );
    assert!(
        !reader_root.exists(),
        "status must not initialize identity or runtime state"
    );
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    assert_eq!(unsafe { kill(child.id().unwrap() as i32, 15) }, 0);
    assert!(
        tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert!(!root.join("runtime/gateway.sock").exists());
}
#[tokio::test]
async fn identities_persist_and_hub_and_gateway_have_separate_keys() {
    let root = tempfile::tempdir().unwrap();
    let first = command(root.path()).arg("id").output().await.unwrap();
    assert!(first.status.success());
    assert_eq!(first.stdout.len(), 65);
    let again = command(root.path()).arg("id").output().await.unwrap();
    assert_eq!(first.stdout, again.stdout);
    let hub = command(root.path())
        .args(["id", "--hub"])
        .output()
        .await
        .unwrap();
    assert!(hub.status.success());
    assert_ne!(hub.stdout, first.stdout);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn trust_forget_waits_for_other_process_and_rejects_a_stopped_process() {
    use omochi_transport::{
        live,
        store::{Role, Store},
    };
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path().join("config")).unwrap();
    let other = tempfile::tempdir().unwrap();
    let peer = Store::open(other.path().join("config"))
        .unwrap()
        .identity(Role::Hub)
        .unwrap()
        .public();
    let invite = store.invite(0, u64::MAX).unwrap();
    store.redeem(&invite, peer, "test-hub", 1).unwrap();
    let mut child = command(root.path())
        .args(["gateway", "--remote", "--offline"])
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let result = command(root.path())
        .args(["trust", "forget", "test-hub"])
        .output()
        .await
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!store.peers().unwrap().1.contains_key(&peer.to_string()));
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    let pid = child.id().unwrap() as i32;
    assert_eq!(unsafe { kill(pid, 19) }, 0); // SIGSTOP on Linux
    let result = live::forget(store.clone(), peer, Duration::from_millis(200)).await;
    assert!(
        result.is_err(),
        "a stopped live process must not acknowledge"
    );
    assert_eq!(unsafe { kill(pid, 18) }, 0); // SIGCONT on Linux
    live::forget(store, peer, Duration::from_secs(3))
        .await
        .unwrap();
    assert_eq!(unsafe { kill(pid, 15) }, 0);
    assert!(child.wait().await.unwrap().success());
}

#[tokio::test]
async fn up_serves_preset_and_terminates_owned_service_group() {
    let root = tempfile::tempdir().unwrap();
    let composition = root.path().join("omochi.toml");
    std::fs::write(
        &composition,
        r#"
version = 1
id = "example"
[services.worker]
command = ["/bin/sh", "-c", "echo $$ > worker.pid; while :; do :; done"]
[dashboard]
name = "Example"
rows = [{panels = ["main"]}]
[dashboard.panels.main]
title = "Worker"
service = "worker"
"#,
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_omochi-ui"));
    child
        .arg("--config-dir")
        .arg(root.path().join("config"))
        .arg("--runtime-dir")
        .arg(root.path().join("omochi"))
        .arg("up")
        .arg(&composition)
        .args(["--port", "0"])
        .env("PATH", "/nonexistent")
        .kill_on_drop(true)
        .stderr(Stdio::piped());
    let mut child = child.spawn().unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let url = url::Url::parse(line.strip_prefix("[omochi-ui] dashboard ").unwrap()).unwrap();
    assert_eq!(url.query(), Some("preset=example"));
    let mut socket = tokio::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap()))
        .await
        .unwrap();
    socket
        .write_all(b"GET /api/presets HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).await.unwrap();
    assert!(response.contains("\"id\":\"example\""), "{response}");
    assert!(response.contains("\"serviceId\":\"worker\""));
    let pid = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(pid) = std::fs::read_to_string(root.path().join("worker.pid"))
                .ok()
                .and_then(|text| text.trim().parse::<i32>().ok())
                .filter(|pid| *pid > 0)
            {
                break pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        unsafe { libc::kill(child.id().unwrap() as i32, libc::SIGTERM) },
        0
    );
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("up shutdown timed out")
        .unwrap();
    let mut stderr = String::new();
    while let Some(line) = lines.next_line().await.unwrap() {
        stderr.push_str(&line);
        stderr.push('\n');
    }
    assert!(status.success(), "up exited with {status}: {stderr}");
    assert_eq!(
        unsafe { libc::kill(pid, 0) },
        -1,
        "service survived up shutdown"
    );
    assert!(!root.path().join("omochi/gateway.sock").exists());
}

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog/expected.rs"]
#[rustfmt::skip]
mod generated_catalog;

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-enums/expected.rs"]
#[rustfmt::skip]
mod generated_scalar_catalog;

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-enum-names/expected.rs"]
#[rustfmt::skip]
mod generated_enum_names;

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-enum-identifiers/expected.rs"]
#[rustfmt::skip]
mod generated_enum_identifiers;

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-property-names/expected.rs"]
#[rustfmt::skip]
mod generated_property_names;

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-component-allof/expected.rs"]
#[rustfmt::skip]
mod generated_component_allof;

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-required-only/expected.rs"]
#[rustfmt::skip]
mod generated_required_only;

#[allow(dead_code, unused_imports, non_camel_case_types)]
#[path = "fixtures/catalog-component-names/expected.rs"]
#[rustfmt::skip]
mod generated_component_names;

#[tokio::test]
async fn catalog_component_names_compile_and_preserve_wire_names() {
    use generated_component_names::{_BadgeProps, Card2Props, SelfProps, classProps};
    use omochi_ui::ComponentProps;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-component-names.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-component-names/expected.rs")
    );
    assert_eq!(
        json!({
            Card2Props::NAME: Card2Props { label: "card".into() }.into_props(),
            SelfProps::NAME: SelfProps { label: "self".into() }.into_props(),
            _BadgeProps::NAME: _BadgeProps { label: "badge".into() }.into_props(),
            classProps::NAME: classProps { label: "keyword".into() }.into_props(),
        }),
        json!({"Card2": {"label": "card"}, "Self": {"label": "self"}, "_Badge": {"label": "badge"}, "class": {"label": "keyword"}})
    );
}

#[tokio::test]
async fn catalog_required_only_preserves_unconstrained_required_values() {
    use generated_required_only::{DirectProps, NestedProps, OnlyProps};
    use omochi_ui::ComponentProps;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-required-only.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-required-only/expected.rs")
    );
    let direct = DirectProps {
        label: "label".into(),
        metadata: json!({"enabled": true}),
    };
    let nested = NestedProps {
        label: "label".into(),
        note: None,
        parent_data: json!(null),
        nested_data: json!([1, "two"]),
    };
    let only = OnlyProps {
        payload: json!(false),
    };
    assert_eq!(
        json!([direct.into_props(), nested.into_props(), only.into_props()]),
        json!([
            {"label": "label", "metadata": {"enabled": true}},
            {"label": "label", "parentData": null, "nestedData": [1, "two"]},
            {"payload": false}
        ])
    );
}

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-annotation-data/expected.rs"]
#[rustfmt::skip]
mod generated_annotation_data;

#[tokio::test]
async fn catalog_literal_annotation_data_preserves_values_and_compiles() {
    use generated_annotation_data::AnnotationProps;
    use omochi_ui::ComponentProps;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-annotation-data.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-annotation-data/expected.rs")
    );
    let props = AnnotationProps {
        value: json!({"$ref": "literal.json"}),
        default: "label".into(),
        examples: vec!["example".into()],
    };
    assert_eq!(
        json!(props.into_props()),
        json!({
            "value": {"$ref": "literal.json"}, "default": "label", "examples": ["example"]
        })
    );
}

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-reference-documents/expected.rs"]
#[rustfmt::skip]
mod generated_reference_documents;

#[tokio::test]
async fn catalog_same_directory_reference_documents_resolve_and_compile() {
    use generated_reference_documents::ReferenceProps;
    use omochi_ui::ComponentProps;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-reference-documents.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-reference-documents/expected.rs")
    );
    let props = ReferenceProps {
        value: "label".into(),
    };
    assert_eq!(json!(props.into_props()), json!({"value": "label"}));
}

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-ref-siblings/expected.rs"]
#[rustfmt::skip]
mod generated_ref_siblings;

#[tokio::test]
async fn catalog_ref_siblings_preserve_values_and_compile() {
    use generated_ref_siblings::RefProps;
    use omochi_ui::ComponentProps;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-ref-siblings.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-ref-siblings/expected.rs")
    );
    let props = RefProps {
        choice: json!("b"),
        reverse: json!("b"),
        annotated: "label".into(),
        details: json!({"label": "label", "count": 2, "enabled": true}),
        required_only: json!({"label": "label", "count": 2}),
        array: json!(["b"]),
        array_type: json!(["label"]),
        object_type: json!({"label": "label"}),
        typed: json!("b"),
        union: json!("label"),
        combined: json!("b"),
        required_in_all_of: json!({"label": "label", "count": 2}),
    };
    assert_eq!(
        json!(props.into_props()),
        json!({
            "choice": "b", "reverse": "b", "annotated": "label",
            "details": {"label": "label", "count": 2, "enabled": true},
            "requiredOnly": {"label": "label", "count": 2}, "array": ["b"],
            "arrayType": ["label"], "objectType": {"label": "label"}, "typed": "b", "union": "label",
            "combined": "b", "requiredInAllOf": {"label": "label", "count": 2}
        })
    );
}

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-adjacent-constraints/expected.rs"]
#[rustfmt::skip]
mod generated_adjacent_constraints;

#[tokio::test]
async fn catalog_adjacent_constraints_preserve_values_and_compile() {
    use generated_adjacent_constraints::{AdjacentChoice, AdjacentProps};
    use omochi_ui::ComponentProps;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-adjacent-constraints.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-adjacent-constraints/expected.rs")
    );
    let props = AdjacentProps {
        choice: AdjacentChoice::Allowed,
        details: json!({"label": "label", "count": 2}),
        typed: json!("allowed"),
        both: json!("label"),
        items: vec![json!("allowed")],
        objects: vec![json!({"label": "label", "count": 2})],
        array: json!(["allowed"]),
    };
    assert_eq!(
        json!(props.into_props()),
        json!({
            "choice": "allowed", "details": {"label": "label", "count": 2}, "typed": "allowed", "both": "label",
            "items": ["allowed"], "objects": [{"label": "label", "count": 2}], "array": ["allowed"]
        })
    );
}

#[path = "fixtures/catalog-overlapping-properties/expected.rs"]
#[rustfmt::skip]
mod generated_overlapping_properties;

#[tokio::test]
async fn catalog_overlapping_properties_preserves_composed_values_in_every_order() {
    use generated_overlapping_properties::*;
    use omochi_ui::ComponentProps;
    use serde_json::json;

    assert_eq!(CATALOG_ID, "urn:test:catalog-overlapping-properties");
    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-overlapping-properties.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // This compiled fixture must remain the exact current CLI output.
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-overlapping-properties/expected.rs")
    );
    let details = json!({"label": "label", "enabled": true, "count": 2});
    for props in [
        ForwardProps {
            value: json!("b"),
            details: details.clone(),
            same: None,
        }
        .into_props(),
        ReverseProps {
            value: json!("b"),
            details: details.clone(),
            same: None,
        }
        .into_props(),
        ParentProps {
            value: json!("b"),
            details: details.clone(),
            same: None,
        }
        .into_props(),
        ParentReverseProps {
            value: json!("b"),
            details: details.clone(),
            same: None,
        }
        .into_props(),
    ] {
        assert_eq!(json!(props), json!({"value": "b", "details": details}));
    }
    // Equal repeated schemas keep their precise original type.
    let same = ForwardProps {
        value: json!("b"),
        details,
        same: Some("same".to_owned()),
    };
    assert_eq!(same.into_props()["same"], "same");
}

#[tokio::test]
async fn catalog_component_allof_preserves_parent_and_nested_properties() {
    use generated_component_allof::{EmptyProps, NestedProps, SiblingProps};
    use omochi_ui::ComponentProps;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-component-allof.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Compile the exact current CLI output, then exercise its public conversion.
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-component-allof/expected.rs")
    );
    let sibling = SiblingProps {
        label: "label".into(),
        count: 2.0,
        note: None,
    };
    let nested = NestedProps {
        label: "label".into(),
        count: 2.0,
        is_valid: true,
        note: None,
        accessibility: None,
        checks: None,
        validation_errors: None,
    };
    assert_eq!(
        json!(sibling.into_props()),
        json!({"label": "label", "count": 2.0})
    );
    assert_eq!(
        json!(nested.clone().into_props()),
        json!({"label": "label", "count": 2.0, "isValid": true})
    );
    assert_eq!(
        json!(
            EmptyProps {
                label: "label".into()
            }
            .into_props()
        ),
        json!({"label": "label"})
    );
    assert_eq!(
        json!(
            NestedProps {
                note: Some("note".into()),
                accessibility: Some(json!({"label": "accessible"})),
                checks: Some(vec![]),
                validation_errors: Some(vec!["error".into()]),
                ..nested
            }
            .into_props()
        ),
        json!({
            "label": "label", "count": 2.0, "isValid": true, "note": "note",
            "accessibility": {"label": "accessible"}, "checks": [], "validationErrors": ["error"]
        })
    );
}

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-builder-key/expected.rs"]
#[rustfmt::skip]
mod generated_builder_key;

#[tokio::test]
async fn catalog_rust_preserves_builder_reserved_key_through_ui() {
    use generated_builder_key::{OptionalProps, RequiredOnlyProps, ShortcutProps};
    use omochi_ui::Ui;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-builder-key.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-builder-key/expected.rs")
    );
    let mut ui = Ui::new("main", generated_builder_key::CATALOG_ID);
    ui.component(
        "shortcut",
        ShortcutProps {
            key: "Enter".into(),
        },
    )
    .unwrap();
    ui.component("optional", OptionalProps { key: None })
        .unwrap();
    ui.component(
        "supplied",
        OptionalProps {
            key: Some("Escape".into()),
        },
    )
    .unwrap();
    ui.component(
        "required",
        RequiredOnlyProps {
            key: json!({"arbitrary": true}),
        },
    )
    .unwrap();
    assert_eq!(
        json!(ui.build().root.slots[0].nodes),
        json!([
            {"component": "Shortcut", "key": "shortcut", "props": {"key": "Enter"}},
            {"component": "Optional", "key": "optional", "props": {}},
            {"component": "Optional", "key": "supplied", "props": {"key": "Escape"}},
            {"component": "RequiredOnly", "key": "required", "props": {"key": {"arbitrary": true}}},
        ])
    );
}

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-nested-key/expected.rs"]
#[rustfmt::skip]
mod generated_nested_key;

#[tokio::test]
async fn catalog_rust_preserves_nested_key_through_ui() {
    use omochi_ui::Ui;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-nested-key.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-nested-key/expected.rs")
    );
    let mut ui = Ui::new("main", generated_nested_key::CATALOG_ID);
    ui.component(
        "shortcut",
        generated_nested_key::ShortcutProps {
            details: json!({"key": "Enter"}),
        },
    )
    .unwrap();
    assert_eq!(
        json!(ui.build().root.slots[0].nodes),
        json!([{"component": "Shortcut", "key": "shortcut", "props": {"details": {"key": "Enter"}}}])
    );
}

#[tokio::test]
async fn catalog_rust_fields_serialize_with_the_original_property_names() {
    use generated_property_names::NamedProps;
    use omochi_ui::ComponentProps;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let schema: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../spec/fixtures/catalog-property-names.json"
    ))
    .unwrap();
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-property-names.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Compile the exact current CLI output, then check its public conversion.
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-property-names/expected.rs")
    );
    let props = NamedProps {
        source_url: "source".into(),
        data_key: "data".into(),
        r#type: "type".into(),
        r#match: "match".into(),
        self_: "self".into(),
        crate_: "crate".into(),
        super_: "super".into(),
        r#gen: "gen".into(),
        optional_url: None,
    };
    let expected = json!({
        "sourceURL": "source", "data_key": "data", "type": "type", "match": "match",
        "self": "self", "crate": "crate", "super": "super", "gen": "gen"
    });
    assert_eq!(json!(props.clone().into_props()), expected);
    for key in schema["components"]["Named"]["required"]
        .as_array()
        .unwrap()
    {
        assert!(expected.get(key.as_str().unwrap()).is_some());
    }
    let mut expected_optional = expected.as_object().unwrap().clone();
    expected_optional.insert("optionalURL".into(), json!("optional"));
    assert_eq!(
        NamedProps {
            optional_url: Some("optional".into()),
            ..props
        }
        .into_props(),
        expected_optional
    );
}

#[tokio::test]
async fn catalog_enum_identifiers_compile_and_preserve_punctuation_and_collisions() {
    use generated_enum_identifiers::*;
    use omochi_ui::ComponentProps;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-enum-identifiers.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-enum-identifiers/expected.rs")
    );

    for (mode_, expected) in [(PanelMode3::A, "a!"), (PanelMode3::B, "b?")] {
        assert_eq!(
            json!(
                PanelProps {
                    mode: PanelMode::Plain,
                    mode_,
                    mode2: PanelMode2::Compact,
                    tone_: PanelTone::Warm,
                }
                .into_props()
            ),
            json!({"mode": "plain", "mode!": expected, "mode2": "compact", "tone?": "warm"})
        );
    }
    assert_eq!(
        json!(SelfProps { _1: SelfValue::One }.into_props()),
        json!({"_1": "one"})
    );
    assert_eq!(
        json!(
            _1Props {
                _2: Value2::Number,
                _3type: Type::Keyword,
                self_: SelfValue2::Reserved,
            }
            .into_props()
        ),
        json!({"_2": "number", "_3type": "keyword", "self": "reserved"})
    );
}

#[tokio::test]
async fn catalog_enum_type_names_compile_without_collisions_and_preserve_wire_values() {
    use generated_enum_names::*;
    use omochi_ui::ComponentProps;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-enum-names.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-enum-names/expected.rs")
    );

    for (props, expected) in [
        (
            PanelProps {
                props: PanelProps3::Primary,
                props2: PanelProps2::Compact,
                tone_props: PanelToneProps2::Warm,
            },
            json!({"props": "primary", "props2": "compact", "toneProps": "warm"}),
        ),
        (
            PanelProps {
                props: PanelProps3::Secondary,
                props2: PanelProps2::Expanded,
                tone_props: PanelToneProps2::Cool,
            },
            json!({"props": "secondary", "props2": "expanded", "toneProps": "cool"}),
        ),
    ] {
        assert_eq!(json!(props.into_props()), expected);
    }
    assert_eq!(json!(PanelToneProps {}.into_props()), json!({}));
    for (tone_size, expected) in [
        (CardToneSize::Large, "large"),
        (CardToneSize::Small, "small"),
    ] {
        assert_eq!(
            json!(CardProps { tone_size }.into_props()),
            json!({"toneSize": expected})
        );
    }
    for (size, expected) in [
        (CardToneSize2::Wide, "wide"),
        (CardToneSize2::Narrow, "narrow"),
    ] {
        assert_eq!(
            json!(CardToneProps { size }.into_props()),
            json!({"size": expected})
        );
    }
    for (object, expected) in [
        (JsonObject2::Literal, "literal"),
        (JsonObject2::Bound, "bound"),
    ] {
        assert_eq!(
            json!(JsonProps { object }.into_props()),
            json!({"object": expected})
        );
    }
}

#[tokio::test]
async fn catalog_scalar_enums_compile_and_serialize_with_their_json_types() {
    use generated_scalar_catalog::{ScalarProps, ScalarTone};
    use omochi_ui::ComponentProps;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = command(root.path())
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("../../spec/fixtures/catalog-enums.json"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // The compiled module must be exactly what the current CLI generates.
    assert_eq!(
        std::fs::read(root.path().join("catalog.rs")).unwrap(),
        include_bytes!("fixtures/catalog-enums/expected.rs")
    );

    for (tone, raw) in [
        (ScalarTone::Info, "info"),
        (ScalarTone::FooBar, "foo-bar"),
        (ScalarTone::FooBar2, "fooBar"),
        (ScalarTone::Value, "1"),
        (ScalarTone::Value2, "2"),
        (ScalarTone::SelfValue, "self"),
        (ScalarTone::X, "x-"),
        (ScalarTone::Value3, "\u{1}"),
    ] {
        assert_eq!(serde_json::to_value(tone).unwrap(), json!(raw));
    }
    for mixed in [
        json!(true),
        json!(false),
        json!(1),
        json!(2),
        json!(null),
        json!("true"),
        json!(1.5),
    ] {
        for enabled in [true, false] {
            for count in [1, 2] {
                let props = ScalarProps {
                    enabled,
                    count: count.into(),
                    ratio: serde_json::Number::from_f64(1.5).unwrap(),
                    empty: (),
                    mixed: mixed.clone(),
                    tone: ScalarTone::Info,
                    flags: vec![true, false],
                    counts: vec![vec![1.into(), 2.into()]],
                    choices: vec![json!(true), json!(false), json!(1), json!(2)],
                    nullable: json!(null),
                    optional: None,
                };
                assert_eq!(
                    json!(props.clone().into_props()),
                    json!({
                        "enabled": enabled, "count": count, "ratio": 1.5, "empty": null,
                        "mixed": mixed, "tone": "info", "flags": [true, false],
                        "counts": [[1, 2]], "choices": [true, false, 1, 2], "nullable": null
                    })
                );
                assert_eq!(
                    json!(
                        ScalarProps {
                            optional: Some(false),
                            ..props
                        }
                        .into_props()
                    )["optional"],
                    json!(false)
                );
            }
        }
    }
}

#[tokio::test]
async fn catalog_id_injection_is_inert_when_generated_python_is_imported() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("catalog.json");
    for key in ["catalogId", "$id"] {
        for (prefix, terminator) in [
            ("demo", "\n"),
            ("demo", "\r"),
            ("demo", "\r\n"),
            ("coding: unicode_escape", "\\n"),
            ("coding=unicode_escape", "\\n"),
        ] {
            let id = format!(
                "{prefix}{terminator}print(\"CATALOG_ID_EXECUTED\"){terminator}#\u{2028}\u{2029}"
            );
            std::fs::write(
                &source,
                serde_json::json!({key: id, "components": {}}).to_string(),
            )
            .unwrap();
            let output = command(root.path())
                .args(["generate", "catalog", "--catalog"])
                .arg(&source)
                .args(["--name", "InjectionCatalog", "--python"])
                .arg(root.path().join("generated_catalog.py"))
                .output()
                .await
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new("python3")
                .current_dir(root.path())
                .env(
                    "PYTHONPATH",
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../packages/sdk/python/src"),
                )
                .args([
                    "-B",
                    "-c",
                    "import json, generated_catalog; print(json.dumps(generated_catalog.INJECTIONCATALOG.id))",
                ])
                .output()
                .await
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            // Injected print output must not precede the exact runtime ID.
            assert_eq!(
                serde_json::from_slice::<String>(&output.stdout).unwrap(),
                id
            );
        }
    }
}

#[tokio::test]
async fn catalog_generation_runs_without_bun_or_state_and_check_detects_stale_output() {
    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/catalog");
    let mut generate = command(root.path());
    generate
        .args(["generate", "catalog", "--catalog"])
        .arg(source.join("catalog.json"))
        .arg("--typescript")
        .arg(root.path().join("catalog.ts"))
        .arg("--python")
        .arg(root.path().join("catalog.py"))
        .arg("--rust")
        .arg(root.path().join("catalog.rs"));
    let output = generate.output().await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !root.path().join("config").exists(),
        "code generation must not initialize identity state"
    );
    for extension in ["ts", "py", "rs"] {
        assert_eq!(
            std::fs::read(root.path().join(format!("catalog.{extension}"))).unwrap(),
            std::fs::read(source.join(format!("expected.{extension}"))).unwrap()
        );
    }
    generate.arg("--check");
    assert!(generate.output().await.unwrap().status.success());
    std::fs::write(root.path().join("catalog.ts"), "user edit").unwrap();
    let output = generate.output().await.unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("stale"));
    assert_eq!(
        std::fs::read_to_string(root.path().join("catalog.ts")).unwrap(),
        "user edit"
    );
}

#[tokio::test]
async fn remote_dashboard_listener_starts_without_registered_gateways_and_keeps_http_local() {
    assert_remote_dashboard_web_url(None, "https://app.omochi.fyi/").await;
}

#[tokio::test]
async fn remote_dashboard_listener_uses_custom_web_url() {
    assert_remote_dashboard_web_url(
        Some("https://ui.example/dashboard/"),
        "https://ui.example/dashboard/",
    )
    .await;
}

async fn assert_remote_dashboard_web_url(web_url: Option<&str>, expected: &str) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let root = tempfile::tempdir().unwrap();
    let mut hub = command(root.path());
    hub.arg("hub");
    if let Some(web_url) = web_url {
        hub.args(["--web-url", web_url]);
    }
    let mut child = hub
        .args([
            "--remote",
            "--relay",
            "https://127.0.0.1:9/",
            "--no-auto-gateway",
            "--port",
            "0",
        ])
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let base = line
        .strip_prefix("[omochi-ui] dashboard ")
        .unwrap()
        .trim_end_matches('/')
        .to_owned();
    assert!(base.starts_with("http://127.0.0.1:"));
    let client = reqwest::Client::new();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let info: serde_json::Value = client
                .get(format!("{base}/api/sharing"))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if info["hubId"].as_str().is_some_and(|s| s.len() == 64) {
                assert_eq!(info["webUrl"], expected);
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        client
            .get(format!("{base}/api/sharing"))
            .header("Origin", "https://attacker.example")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        unsafe { libc::kill(child.id().unwrap() as i32, libc::SIGTERM) },
        0
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}
