fn private_root() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    omochi_local::LocalRuntime::prepare(Some(root.path().to_path_buf())).unwrap();
    root
}
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
};
#[tokio::test]
async fn native_stream_replays_unicode_keeps_eof_and_exits_while_stdin_is_idle() {
    for eof in [false, true] {
        let root = private_root();
        let mut child = Command::new(env!("CARGO_BIN_EXE_omochi-utils"))
            .args(["stream", "--service-id", "test", "--runtime-dir"])
            .arg(root.path())
            .env("PATH", "/nonexistent")
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut logs = BufReader::new(child.stderr.take().unwrap()).lines();
        tokio::time::timeout(Duration::from_secs(5), logs.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        input.write_all("hello あ world".as_bytes()).await.unwrap();
        if eof {
            input.shutdown().await.unwrap();
            drop(input);
        } else {
            child.stdin = Some(input);
        }
        let socket = tokio::net::UnixStream::connect(root.path().join("svc-test~/a2ui.sock"))
            .await
            .unwrap();
        let mut lines = BufReader::new(socket).lines();
        tokio::time::timeout(Duration::from_secs(3), async {
            let mut output = false;
            let mut closed = false;
            loop {
                let line = lines.next_line().await.unwrap().unwrap();
                output |= line.contains("hello あ world");
                closed |= line.contains("Input closed");
                if output && (!eof || closed) {
                    break;
                }
            }
        })
        .await
        .unwrap();
        assert!(child.try_wait().unwrap().is_none());
        unsafe {
            libc::kill(child.id().unwrap() as i32, libc::SIGTERM);
        }
        assert!(
            tokio::time::timeout(Duration::from_secs(3), child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
        assert!(!root.path().join("svc-test~/a2ui.sock").exists());
    }
}

#[tokio::test]
async fn button_only_runs_on_action_and_publishes_completion() {
    let root = private_root();
    let mut child = Command::new(env!("CARGO_BIN_EXE_omochi-utils"))
        .args(["button", "--service-id", "button", "--runtime-dir"])
        .arg(root.path())
        .arg("--cwd")
        .arg(root.path())
        .args(["--", "/bin/sh", "-c", "printf x >> runs; printf finished"])
        .env("PATH", "/nonexistent")
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut logs = BufReader::new(child.stderr.take().unwrap()).lines();
    tokio::time::timeout(Duration::from_secs(5), logs.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let socket = tokio::net::UnixStream::connect(root.path().join("svc-button~/a2ui.sock"))
        .await
        .unwrap();
    let (read, mut write) = socket.into_split();
    let mut lines = BufReader::new(read).lines();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if lines.next_line().await.unwrap().unwrap().contains("Ready") {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(!root.path().join("runs").exists());
    let action = b"{\"version\":\"v0.9\",\"action\":{\"surfaceId\":\"main\",\"name\":\"run\",\"context\":{}}}\n";
    write.write_all(action).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut exited = false;
        let mut output = false;
        loop {
            let line = lines.next_line().await.unwrap().unwrap();
            exited |= line.contains("Exited with code 0");
            output |= line.contains("finished");
            if exited && output {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("runs")).unwrap(),
        "x"
    );
    unsafe {
        libc::kill(child.id().unwrap() as i32, libc::SIGTERM);
    }
    assert!(
        tokio::time::timeout(Duration::from_secs(3), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}

#[tokio::test]
async fn process_restart_policies_and_manual_stop() {
    for (policy, code, automatic) in [
        ("on-failure", 2, true),
        ("always", 0, true),
        ("never", 2, false),
        ("on-failure", 0, false),
    ] {
        let root = private_root();
        let script = format!("printf x >> runs; exit {code}");
        let mut child = Command::new(env!("CARGO_BIN_EXE_omochi-utils"))
            .args(["process", "--service-id", "worker", "--runtime-dir"])
            .arg(root.path())
            .arg("--cwd")
            .arg(root.path())
            .args([
                "--restart",
                policy,
                "--restart-delay",
                "80ms",
                "--",
                "/bin/sh",
                "-c",
                &script,
            ])
            .env("PATH", "/nonexistent")
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut logs = BufReader::new(child.stderr.take().unwrap()).lines();
        tokio::time::timeout(Duration::from_secs(5), logs.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let socket = tokio::net::UnixStream::connect(root.path().join("svc-worker~/a2ui.sock"))
            .await
            .unwrap();
        let (read, mut write) = socket.into_split();
        let mut lines = BufReader::new(read).lines();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if lines
                    .next_line()
                    .await
                    .unwrap()
                    .unwrap()
                    .contains("Stopped")
                {
                    break;
                }
            }
        })
        .await
        .unwrap();
        assert!(!root.path().join("runs").exists());
        let action = |name: &str| {
            format!(
                "{{\"version\":\"v0.9\",\"action\":{{\"surfaceId\":\"main\",\"name\":\"{name}\",\"context\":{{}}}}}}\n"
            )
        };
        write.write_all(action("start").as_bytes()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if std::fs::read(root.path().join("runs"))
                    .unwrap_or_default()
                    .len()
                    >= if automatic { 2 } else { 1 }
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        if !automatic {
            // Wait for the actual command completion, not a presumed restart window.
            tokio::time::timeout(Duration::from_secs(3), async {
                while !lines
                    .next_line()
                    .await
                    .unwrap()
                    .unwrap()
                    .contains(&format!("Exited with code {code}"))
                {}
            })
            .await
            .unwrap();
            assert_eq!(std::fs::read(root.path().join("runs")).unwrap().len(), 1);
        }
        write.write_all(action("stop").as_bytes()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if lines
                    .next_line()
                    .await
                    .unwrap()
                    .unwrap()
                    .contains("Stopped")
                {
                    break;
                }
            }
        })
        .await
        .unwrap();
        let count = std::fs::read(root.path().join("runs")).unwrap().len();
        // Suppression over arbitrary time is covered by the virtual-clock tests.
        // A user restart remains available after suppressing automatic restarts.
        write.write_all(action("restart").as_bytes()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if std::fs::read(root.path().join("runs")).unwrap().len() > count {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        unsafe {
            libc::kill(child.id().unwrap() as i32, libc::SIGTERM);
        }
        assert!(
            tokio::time::timeout(Duration::from_secs(3), child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
    }
}

#[tokio::test]
async fn watch_diff_history_pause_and_resume() {
    let root = private_root();
    let mut child = Command::new(env!("CARGO_BIN_EXE_omochi-utils"))
        .args(["watch", "--service-id", "watch", "--runtime-dir"])
        .arg(root.path())
        .arg("--cwd")
        .arg(root.path())
        .args([
            "--interval",
            "80ms",
            "--diff",
            "--history",
            "3",
            "--",
            "/bin/sh",
            "-c",
            "printf x >> runs; /usr/bin/wc -c < runs",
        ])
        .env("PATH", "/nonexistent")
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut logs = BufReader::new(child.stderr.take().unwrap()).lines();
    tokio::time::timeout(Duration::from_secs(5), logs.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let socket = tokio::net::UnixStream::connect(root.path().join("svc-watch~/a2ui.sock"))
        .await
        .unwrap();
    let (read, mut write) = socket.into_split();
    let mut lines = BufReader::new(read).lines();
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut diff = false;
        let mut history = false;
        loop {
            let line = lines.next_line().await.unwrap().unwrap();
            diff |= line.contains("--- previous");
            history |= line.contains("History");
            if diff && history {
                break;
            }
        }
    })
    .await
    .unwrap();
    let action = |name: &str| {
        format!(
            "{{\"version\":\"v0.9\",\"action\":{{\"surfaceId\":\"main\",\"name\":\"{name}\",\"context\":{{}}}}}}\n"
        )
    };
    write.write_all(action("pause").as_bytes()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let line = lines.next_line().await.unwrap().unwrap();
            if line.contains("Paused") && line.contains("Exited with code 0") {
                break;
            }
        }
    })
    .await
    .unwrap();
    // Paused plus a completed result acknowledges that the in-flight run ended.
    let count = std::fs::read(root.path().join("runs")).unwrap().len();
    write.write_all(action("resume").as_bytes()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if std::fs::read(root.path().join("runs")).unwrap().len() > count {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    unsafe {
        libc::kill(child.id().unwrap() as i32, libc::SIGTERM);
    }
    assert!(
        tokio::time::timeout(Duration::from_secs(3), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}

#[tokio::test]
async fn watch_on_change_publishes_initial_result_and_timeout_is_visible() {
    for timed_out in [false, true] {
        let root = private_root();
        let mut command = Command::new(env!("CARGO_BIN_EXE_omochi-utils"));
        command
            .args(["watch", "--service-id", "watch", "--runtime-dir"])
            .arg(root.path());
        if timed_out {
            command.args([
                "--timeout",
                "30ms",
                "--interval",
                "1m",
                "--",
                "/bin/sh",
                "-c",
                "while :; do :; done",
            ]);
        } else {
            command.args([
                "--on-change",
                "--interval",
                "40ms",
                "--",
                "/bin/sh",
                "-c",
                "printf same",
            ]);
        }
        let mut child = command
            .env("PATH", "/nonexistent")
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut logs = BufReader::new(child.stderr.take().unwrap()).lines();
        tokio::time::timeout(Duration::from_secs(5), logs.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let socket = tokio::net::UnixStream::connect(root.path().join("svc-watch~/a2ui.sock"))
            .await
            .unwrap();
        let mut lines = BufReader::new(socket).lines();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if lines
                    .next_line()
                    .await
                    .unwrap()
                    .unwrap()
                    .contains(if timed_out {
                        "Timed out"
                    } else {
                        "Exited with code 0"
                    })
                {
                    break;
                }
            }
        })
        .await
        .unwrap();
        // Identical-result suppression is checked on completed inputs in unit tests,
        // rather than inferred from a short period without socket traffic.
        unsafe {
            libc::kill(child.id().unwrap() as i32, libc::SIGTERM);
        }
        assert!(
            tokio::time::timeout(Duration::from_secs(3), child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
    }
}

#[tokio::test]
async fn http_reports_unhealthy_and_recovery_through_native_action_loop() {
    use std::sync::{
        Arc,
        atomic::{AtomicU16, Ordering},
    };
    let status = Arc::new(AtomicU16::new(200));
    let status_copy = status.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/health", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let status = status_copy.load(Ordering::SeqCst);
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                let mut buffer = [0u8; 4096];
                let _ = socket.read(&mut buffer).await;
                let body = if status == 200 {
                    "{\"ok\":true}"
                } else {
                    "{\"ok\":false}"
                };
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    let root = private_root();
    let mut child = Command::new(env!("CARGO_BIN_EXE_omochi-utils"))
        .args(["http", "--service-id", "health", "--runtime-dir"])
        .arg(root.path())
        .args(["--interval", "1m", "--expect-body", "\"ok\":true", &url])
        .env("PATH", "/nonexistent")
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut logs = BufReader::new(child.stderr.take().unwrap()).lines();
    tokio::time::timeout(Duration::from_secs(5), logs.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let socket = tokio::net::UnixStream::connect(root.path().join("svc-health~/a2ui.sock"))
        .await
        .unwrap();
    let (read, mut write) = socket.into_split();
    let mut lines = BufReader::new(read).lines();
    for (code, expected) in [
        (200, "Initial result: healthy"),
        (503, "Became unhealthy"),
        (200, "Recovered"),
    ] {
        status.store(code, Ordering::SeqCst);
        if expected != "Initial result: healthy" {
            write.write_all(b"{\"version\":\"v0.9\",\"action\":{\"surfaceId\":\"main\",\"name\":\"run\",\"context\":{}}}\n").await.unwrap();
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if lines.next_line().await.unwrap().unwrap().contains(expected) {
                    break;
                }
            }
        })
        .await
        .unwrap();
    }
    unsafe {
        libc::kill(child.id().unwrap() as i32, libc::SIGTERM);
    }
    assert!(
        tokio::time::timeout(Duration::from_secs(3), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    server.abort();
    let _ = server.await;
}
