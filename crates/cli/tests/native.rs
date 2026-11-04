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
    local_dashboard(false).await;
}
#[tokio::test]
async fn offline_hub_preserves_local_http_and_websocket_without_remote_tools() {
    local_dashboard(true).await;
}
async fn local_dashboard(offline: bool) {
    let root = tempfile::tempdir().unwrap();
    let mut child = command(root.path())
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
#[path = "fixtures/catalog-property-names/expected.rs"]
#[rustfmt::skip]
mod generated_property_names;

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-component-allof/expected.rs"]
#[rustfmt::skip]
mod generated_component_allof;

#[allow(dead_code, unused_imports)]
#[path = "fixtures/catalog-overlapping-properties/expected.rs"]
#[rustfmt::skip]
mod generated_overlapping_properties;

#[tokio::test]
async fn catalog_overlapping_properties_preserves_composed_values_in_every_order() {
    use generated_overlapping_properties::*;
    use omochi_ui::ComponentProps;
    use serde_json::json;

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
