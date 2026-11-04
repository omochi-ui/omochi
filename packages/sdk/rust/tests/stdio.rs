use omochi_ui::{App, Error, ResourceRegistry, StdioTransport, Transport};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const CHILD_MODE: &str = "OMOCHI_STDIO_TEST_MODE";
const DEADLINE: Duration = Duration::from_secs(5);
const FRAME_LIMIT: usize = 4 * 1024 * 1024;

// Run the actual transport in a fresh process: stdin is process-global, and a
// regression can hang Tokio's destructor even after its async work has finished.
#[test]
fn stdio_child_process() {
    let Ok(mode) = std::env::var(CHILD_MODE) else {
        return;
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        if matches!(
            mode.as_str(),
            "action_error" | "json_error" | "resources" | "runtime_eof" | "runtime_frame_error"
        ) {
            let mut app = App::new(());
            app.action::<(), _, _>("fail", |_, _| async {
                Err(Error::Protocol("handler failed".into()))
            })
            .unwrap();
            app.action::<(), _, _>("wait", |_, _| async {
                println!("{}", json!({"action": "started"}));
                std::future::pending().await
            })
            .unwrap();
            let mut app_runtime = app.build();
            let result = app_runtime.start().await;
            match mode.as_str() {
                "action_error" => assert!(
                    matches!(result, Err(Error::Protocol(ref error)) if error == "handler failed")
                ),
                "json_error" => assert!(matches!(result, Err(Error::Json(_)))),
                "runtime_frame_error" => assert!(
                    matches!(result, Err(Error::Io(ref error)) if error.kind() == std::io::ErrorKind::InvalidData && error.to_string().contains("4 MiB"))
                ),
                "runtime_eof" => result.unwrap(),
                _ => panic!("pending action unexpectedly completed: {result:?}"),
            }
            assert!(app_runtime.is_closed());
        } else {
            let mut transport = StdioTransport::new();
            if mode != "direct_stop" && mode != "direct_frame_error" {
                transport.set_resources(ResourceRegistry::default());
            }
            transport.send(&json!({"ready": true})).await.unwrap();
            if mode == "ordered_eof" {
                for expected in 0..3 {
                    assert_eq!(transport.receive().await.unwrap(), Some(json!(expected)));
                }
                assert_eq!(transport.receive().await.unwrap(), None);
            } else if matches!(mode.as_str(), "frame_error" | "direct_frame_error") {
                assert!(
                    matches!(transport.receive().await, Err(Error::Io(ref error)) if error.kind() == std::io::ErrorKind::InvalidData && error.to_string().contains("4 MiB"))
                );
                assert_eq!(transport.receive().await.unwrap(), None);
            } else if mode == "boundary" {
                assert_eq!(transport.receive().await.unwrap(), Some(json!("雪🦀")));
                assert_eq!(transport.receive().await.unwrap(), None);
            } else if mode == "io_error" {
                // Invalid UTF-8 makes std::io's line reader return InvalidData.
                assert!(matches!(transport.receive().await, Err(Error::Io(_))));
                assert_eq!(transport.receive().await.unwrap(), None);
            } else {
                assert_eq!(transport.receive().await.unwrap(), Some(json!("stop")));
            }
            if mode != "drop" {
                for _ in 0..2 {
                    transport.stop_receiving().await;
                    assert_eq!(transport.receive().await.unwrap(), None);
                }
            }
        }
        println!("{}", json!({"finished": true}));
    });
    // In particular, this must not wait for upstream stdin to close.
    drop(runtime);
}

struct StdioChild {
    process: Child,
    input: Option<ChildStdin>,
    output: mpsc::Receiver<Value>,
}

impl StdioChild {
    fn spawn(mode: &str) -> Self {
        let mut process = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "stdio_child_process", "--nocapture"])
            .env(CHILD_MODE, mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = process.stdin.take();
        let stdout = process.stdout.take().unwrap();
        let (sender, output) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                // The test harness also writes human-readable status lines.
                if let Ok(value) = serde_json::from_str(&line) {
                    if sender.send(value).is_err() {
                        break;
                    }
                }
            }
        });
        let child = Self {
            process,
            input,
            output,
        };
        let ready = child.read();
        assert!(ready["ready"] == true || ready["kind"] == "hello");
        child
    }

    fn write(&mut self, bytes: &[u8]) {
        self.input.as_mut().unwrap().write_all(bytes).unwrap();
        self.input.as_mut().unwrap().flush().unwrap();
    }

    fn read(&self) -> Value {
        self.output
            .recv_timeout(DEADLINE)
            .expect("child must produce the expected output before the deadline")
    }

    fn assert_exit(&mut self) {
        assert_eq!(self.read(), json!({"finished": true}));
        let deadline = Instant::now() + DEADLINE;
        loop {
            if let Some(status) = self.process.try_wait().unwrap() {
                assert!(status.success(), "child failed: {status}");
                return;
            }
            assert!(
                Instant::now() < deadline,
                "child hung during Tokio shutdown"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for StdioChild {
    fn drop(&mut self) {
        // Also clean up a hung regression before releasing the upstream pipe.
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

#[test]
fn runtime_errors_exit_with_upstream_stdin_still_open() {
    for (mode, input) in [
        (
            "action_error",
            b"{\"type\":\"invoke\",\"action\":\"fail\",\"input\":null}\n".as_slice(),
        ),
        ("json_error", b"invalid json\n".as_slice()),
    ] {
        let mut child = StdioChild::spawn(mode);
        child.write(input);
        child.assert_exit();
        assert!(child.input.is_some(), "stdin must remain open through exit");
    }
}

#[test]
fn stop_and_drop_exit_with_upstream_stdin_still_open() {
    for mode in ["stop", "direct_stop", "drop"] {
        let mut child = StdioChild::spawn(mode);
        child.write(b"\"stop\"\n");
        child.assert_exit();
        assert!(child.input.is_some(), "stdin must remain open through exit");
    }
}

#[test]
fn stdio_preserves_lines_and_drains_buffered_messages_before_eof() {
    let mut child = StdioChild::spawn("ordered_eof");
    child.write(b"\n0\r\n   \n1\n2");
    drop(child.input.take());
    child.assert_exit();
}

#[test]
fn runtime_exits_normally_after_stdin_eof() {
    let mut child = StdioChild::spawn("runtime_eof");
    drop(child.input.take());
    child.assert_exit();
}

#[test]
fn stdin_read_error_is_delivered_before_eof() {
    let mut child = StdioChild::spawn("io_error");
    child.write(b"\xff\n");
    child.assert_exit();
}

#[test]
fn oversized_stdio_frames_fail_before_newline_or_eof() {
    for mode in ["frame_error", "direct_frame_error", "runtime_frame_error"] {
        let mut child = StdioChild::spawn(mode);
        // Keep stdin open and omit LF: receiving must fail at the first excess byte.
        child.write(&vec![b' '; FRAME_LIMIT + 1]);
        child.assert_exit();
        assert!(child.input.is_some());
    }
}

#[test]
fn stdio_frame_limit_counts_raw_bytes_including_carriage_return() {
    for suffix in [b"\n".as_slice(), b"\r\n".as_slice(), b"".as_slice()] {
        let mut child = StdioChild::spawn("boundary");
        let message = serde_json::to_vec(&json!("雪🦀")).unwrap();
        let carriage_return = usize::from(suffix.starts_with(b"\r"));
        let mut frame = vec![b' '; FRAME_LIMIT - message.len() - carriage_return];
        frame.extend_from_slice(&message);
        frame.extend_from_slice(suffix);
        // Split within the UTF-8 sequence and ensure buffering spans pipe reads.
        let split = FRAME_LIMIT - carriage_return - 4;
        child.write(&frame[..split]);
        child.write(&frame[split..]);
        drop(child.input.take());
        child.assert_exit();
    }
    for tail in ["\r", "雪"] {
        let mut child = StdioChild::spawn("frame_error");
        let mut frame = vec![b' '; FRAME_LIMIT];
        frame.extend_from_slice(tail.as_bytes());
        child.write(&frame);
        child.assert_exit();
    }
}

#[test]
fn oversized_completed_whitespace_is_not_ignored() {
    let mut child = StdioChild::spawn("frame_error");
    let mut frame = vec![b' '; FRAME_LIMIT + 1];
    frame.extend_from_slice(b"\n\"stop\"\n");
    child.write(&frame);
    child.assert_exit();
}

#[test]
fn resources_are_answered_while_an_action_is_pending() {
    let mut child = StdioChild::spawn("resources");
    child.write(b"{\"type\":\"invoke\",\"action\":\"wait\",\"input\":null}\n");
    assert_eq!(child.read(), json!({"action": "started"}));
    child.write(
        b"{\"omochi\":\"resources/1\",\"kind\":\"request\",\"id\":\"pending\",\"resource\":\"00000000000000000000000000000000\",\"op\":\"stat\"}\n",
    );
    let response = child.read();
    assert_eq!(response["kind"], "response");
    assert_eq!(response["id"], "pending");
    assert_eq!(response["status"], 404);
    // The action deliberately never finishes; Drop terminates this fixture.
}
