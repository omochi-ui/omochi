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
async fn native_hub_serves_bundled_dashboard_without_bun_and_cleans_up_owned_gateway() {
    let root = tempfile::tempdir().unwrap();
    let mut child = command(root.path())
        .args(["hub", "--port", "0"])
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
    let reader_root = root.path().join("status-reader");
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
    assert!(!root.path().join("runtime/gateway.sock").exists());
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
command = ["/bin/sh", "-c", "echo $$ > worker.pid; exec /bin/sleep 120"]
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
            if let Ok(pid) = std::fs::read_to_string(root.path().join("worker.pid")) {
                break pid.trim().parse::<i32>().unwrap();
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
