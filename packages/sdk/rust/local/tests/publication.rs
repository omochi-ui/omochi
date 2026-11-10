use omochi_local::{LocalEndpoint, LocalRuntime};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn shared_runtime_v2_fixture_matches_native_contract() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../spec/fixtures/local-service-v2.json"
    ))
    .unwrap();
    for value in fixture["validServiceIds"].as_array().unwrap() {
        omochi_local::validate_service_id(value.as_str().unwrap()).unwrap();
    }
    for value in fixture["invalidServiceIds"].as_array().unwrap() {
        assert!(omochi_local::validate_service_id(value.as_str().unwrap()).is_err());
    }
    for (name, supported) in [("unix", cfg!(unix)), ("windows", cfg!(windows))] {
        let manifest: omochi_local::Manifest =
            serde_json::from_value(fixture[name].clone()).unwrap();
        manifest.validate().unwrap();
        assert_eq!(serde_json::to_value(&manifest).unwrap(), fixture[name]);
        assert_eq!(
            manifest.endpoint.resolve(std::path::Path::new(".")).is_ok(),
            supported
        );
    }
    for path in ["a\\b", "a\0b", "", ".", "..", "../a"] {
        assert!(
            LocalEndpoint::Unix { path: path.into() }
                .resolve(std::path::Path::new("."))
                .is_err()
        );
    }
    for name in [
        r"\\.\pipe\foreign",
        r"\\server\pipe\omochi-local-v2-0123456789abcdef0123456789abcdef",
    ] {
        assert!(
            LocalEndpoint::NamedPipe { name: name.into() }
                .resolve(std::path::Path::new("."))
                .is_err()
        );
    }
}

#[tokio::test]
async fn ownership_discovery_stream_and_successor() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("v2");
    let runtime = LocalRuntime::prepare(Some(root.clone())).unwrap();
    for id in ["CON", "Demo", ".", "..", "a/b", ""] {
        assert!(runtime.publish(id, "Demo", None).await.is_err());
    }
    let owner = runtime.publish("con", "Demo", None).await.unwrap();
    assert!(owner.directory().ends_with("svc-con~"));
    assert_eq!(
        runtime
            .publish("con", "Contender", None)
            .await
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    let services = runtime.scan().unwrap();
    assert_eq!(services["con"].manifest.version, 2);
    let endpoint = services["con"].endpoint.clone();
    let (client, server) = tokio::join!(endpoint.connect(), owner.accept());
    let (mut client, mut server) = (client.unwrap(), server.unwrap());
    client.write_all(b"frame\n").await.unwrap();
    let mut bytes = [0; 6];
    server.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"frame\n");
    let mut finished = owner.finished();
    owner.stop().await;
    assert!(runtime.scan().unwrap().is_empty());
    drop(owner);
    assert!(
        runtime
            .publish("con", "Before clients close", None)
            .await
            .is_err()
    );
    drop(server);
    drop(client);
    finished.wait_for(|done| *done).await.unwrap();
    let successor = runtime.publish("con", "Successor", None).await.unwrap();
    assert_eq!(runtime.scan().unwrap()["con"].manifest.name, "Successor");
    successor.stop().await;
    drop(successor);
    assert!(root.join(".svc-con.lock~").exists());
}
#[test]
fn refuses_legacy_directory_and_path_traversal() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("legacy");
    omochi_local::files::private_directory(&root).unwrap();
    std::fs::write(root.join("manifest.json"), b"{}").unwrap();
    assert!(LocalRuntime::prepare(Some(root)).is_err());
    for path in ["../a", "/tmp/a", ".", "..", "a/b"] {
        assert!(
            LocalEndpoint::Unix { path: path.into() }
                .resolve(temp.path())
                .is_err()
        );
    }
}
#[cfg(unix)]
#[test]
fn refuses_foreign_writable_root_and_symlink() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let public = temp.path().join("public");
    std::fs::create_dir(&public).unwrap();
    std::fs::set_permissions(&public, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(LocalRuntime::prepare(Some(public.clone())).is_err());
    assert_eq!(
        std::fs::metadata(&public).unwrap().permissions().mode() & 0o777,
        0o777
    );
    let link = temp.path().join("link");
    symlink(&public, &link).unwrap();
    assert!(LocalRuntime::prepare(Some(link)).is_err());
}

#[cfg(unix)]
#[test]
#[ignore = "isolated default-runtime environment fixture"]
fn default_runtime_fixture() {
    LocalRuntime::prepare(None).unwrap();
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "isolated fallback-runtime publisher fixture"]
async fn fallback_runtime_publisher_fixture() {
    let runtime = LocalRuntime::prepare(None).unwrap();
    let owner = runtime
        .publish("temperature-typescript", "Temperature", None)
        .await
        .unwrap();
    let service = runtime
        .scan()
        .unwrap()
        .remove("temperature-typescript")
        .unwrap();
    let (client, server) = tokio::join!(service.endpoint.connect(), owner.accept());
    drop(client.unwrap());
    drop(server.unwrap());
    owner.stop().await;
}

#[cfg(unix)]
#[test]
fn fallback_runtime_fits_macos_temporary_directory() {
    // macOS commonly supplies a 45-byte per-user temporary directory.
    let temp = tempfile::Builder::new()
        .prefix("oc-")
        .tempdir_in("/tmp")
        .unwrap();
    let temporary = temp
        .path()
        .join("t".repeat(45 - temp.path().as_os_str().len() - 1));
    std::fs::create_dir(&temporary).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "fallback_runtime_publisher_fixture",
            "--nocapture",
        ])
        .env_remove("OMOCHI_RUNTIME_DIR")
        .env_remove("XDG_RUNTIME_DIR")
        .env("TMPDIR", &temporary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fallback publisher failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn fallback_runtime_rejects_unsafe_parent_before_touching_children() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for use_symlink in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp
            .path()
            .join(format!("omochi-{}", unsafe { libc::geteuid() }));
        let target = if use_symlink {
            let target = temp.path().join("target");
            std::fs::create_dir(&target).unwrap();
            symlink(&target, &parent).unwrap();
            target
        } else {
            std::fs::create_dir(&parent).unwrap();
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o777)).unwrap();
            parent
        };
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "default_runtime_fixture",
                "--nocapture",
            ])
            .env_remove("OMOCHI_RUNTIME_DIR")
            .env_remove("XDG_RUNTIME_DIR")
            .env("TMPDIR", temp.path())
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted unsafe fallback parent");
        assert!(
            std::fs::read_dir(&target).unwrap().next().is_none(),
            "traversed unsafe fallback parent"
        );
    }
}

#[cfg(unix)]
#[test]
fn default_runtime_rejects_unsafe_xdg_before_touching_children() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let xdg = temp.path().join("xdg");
    std::fs::create_dir(&xdg).unwrap();
    std::fs::set_permissions(&xdg, std::fs::Permissions::from_mode(0o777)).unwrap();
    let link = temp.path().join("link");
    symlink(&xdg, &link).unwrap();
    let foreign = temp.path().join("foreign");
    let mut paths = vec![&xdg, &link];
    if unsafe { libc::geteuid() } == 0 {
        std::fs::create_dir(&foreign).unwrap();
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(foreign.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::chown(name.as_ptr(), 1, 1) }, 0);
        paths.push(&foreign);
    }
    for path in paths {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "default_runtime_fixture",
                "--nocapture",
            ])
            .env_remove("OMOCHI_RUNTIME_DIR")
            .env("XDG_RUNTIME_DIR", path)
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "accepted unsafe XDG: {}",
            path.display()
        );
        assert!(!xdg.join("omochi").exists(), "traversed unsafe XDG");
        assert!(!foreign.join("omochi").exists(), "traversed foreign XDG");
    }
    assert_eq!(
        std::fs::metadata(&xdg).unwrap().permissions().mode() & 0o777,
        0o777
    );
}

#[cfg(unix)]
#[test]
fn concurrent_publishers_prepare_a_missing_nested_runtime() {
    use std::sync::{Arc, Barrier};
    for _ in 0..20 {
        // Keep the socket below the portable Unix path limit on macOS as well.
        let temp = tempfile::Builder::new()
            .prefix("oc-")
            .tempdir_in("/tmp")
            .unwrap();
        let root = temp.path().join("missing/nested/v2");
        let barrier = Arc::new(Barrier::new(16));
        std::thread::scope(|scope| {
            let threads: Vec<_> = (0..16)
                .map(|index| {
                    let barrier = barrier.clone();
                    let root = root.clone();
                    scope.spawn(move || {
                        barrier.wait();
                        let runtime = LocalRuntime::prepare(Some(root)).unwrap();
                        tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .unwrap()
                            .block_on(async {
                                let owner = runtime
                                    .publish(&format!("publisher-{index}"), "Concurrent", None)
                                    .await
                                    .unwrap();
                                owner.stop().await;
                            });
                    })
                })
                .collect();
            for thread in threads {
                thread.join().unwrap();
            }
        });
        assert!(LocalRuntime::open(root).is_ok());
    }
}
#[tokio::test]
async fn gateway_record_and_independent_budget_streams() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = LocalRuntime::prepare(Some(temp.path().join("v2"))).unwrap();
    let owner = runtime.publish_gateway().await.unwrap();
    assert!(runtime.publish_gateway().await.is_err());
    let endpoint = runtime.gateway_endpoint().unwrap();
    let (client, server) = tokio::join!(endpoint.connect(), owner.accept());
    drop(client.unwrap());
    drop(server.unwrap());
    owner.stop().await;
    drop(owner);
    assert!(runtime.gateway_endpoint().is_err());
}

#[cfg(unix)]
#[test]
fn explicit_unlock_survives_an_inherited_open_file_description() {
    use omochi_local::files::FileLock;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private");
    omochi_local::files::private_directory(&root).unwrap();
    let path = root.join(".owner.lock~");
    let owner = FileLock::acquire(&path, true).unwrap();
    let inherited = owner.file().try_clone().unwrap();
    assert!(FileLock::acquire(&path, true).is_err());
    drop(owner);
    let successor = FileLock::acquire(&path, true).unwrap();
    drop(inherited);
    assert!(FileLock::acquire(&path, true).is_err());
    drop(successor);
    assert!(path.exists());
}
#[tokio::test]
async fn context_shutdown_cancels_accept_and_releases_ownership() {
    use std::sync::Arc;
    let temp = tempfile::tempdir().unwrap();
    let context = Arc::new(omochi_local::binding::Context::default());
    let root = temp.path().join("v2");
    let result = context
        .request(
            serde_json::json!({"op":"publish","serviceId":"demo","runtimeDir":root})
                .to_string()
                .as_bytes(),
            &[],
        )
        .await
        .unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&result).unwrap()["id"]
        .as_u64()
        .unwrap();
    let accepting = {
        let context = context.clone();
        tokio::spawn(async move {
            context
                .request(format!("{{\"op\":\"accept\",\"id\":{id}}}").as_bytes(), &[])
                .await
        })
    };
    tokio::task::yield_now().await;
    tokio::time::timeout(std::time::Duration::from_secs(5), context.shutdown())
        .await
        .unwrap();
    assert!(accepting.await.unwrap().is_err());
    let runtime = LocalRuntime::open(root).unwrap();
    let successor = runtime.publish("demo", "Successor", None).await.unwrap();
    successor.stop().await;
}
#[test]
#[ignore = "subprocess crash fixture"]
fn crash_fixture() {
    let root = std::env::var_os("OMOCHI_CRASH_ROOT").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let root = LocalRuntime::prepare(Some(root.into())).unwrap();
        let _owner = root.publish("crash", "Crash", None).await.unwrap();
        println!("publisher ready");
        std::future::pending::<()>().await;
    });
}
#[tokio::test]
async fn killed_publisher_leaves_recoverable_advertisement() {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("v2");
    let mut process = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "crash_fixture", "--nocapture"])
        .env("OMOCHI_CRASH_ROOT", &root)
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(process.stdout.take().unwrap()).lines();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(line) = lines.next_line().await.unwrap() {
            if line == "publisher ready" {
                return;
            }
        }
        panic!("fixture exited before publication");
    })
    .await
    .unwrap();
    let runtime = LocalRuntime::open(root.clone()).unwrap();
    assert_eq!(runtime.scan().unwrap().len(), 1);
    process.kill().await.unwrap();
    assert!(root.join("svc-crash~/manifest.json").exists());
    let successor = runtime.publish("crash", "Recovered", None).await.unwrap();
    assert_eq!(runtime.scan().unwrap()["crash"].manifest.name, "Recovered");
    successor.stop().await;
}
