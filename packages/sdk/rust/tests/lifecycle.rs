use omochi_ui::{App, CallbackTransport, Error, ResourceRegistry, Result};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;

#[tokio::test]
async fn invalid_inputs_do_not_stop_the_shared_receive_loop() {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    let mut app = App::new(0_i64);
    app.action::<i64, _, _>("add", |value, context| async move {
        context.update(|state| *state += value).await
    })
    .unwrap();
    let messages = Arc::new(Mutex::new(VecDeque::from([
        json!({"type": "invoke", "action": "add", "input": "invalid"}),
        json!({"type": "invoke", "action": "add", "input": 2}),
        json!({"action": {"name": "add", "context": {"input": false}}}),
        json!({"action": {"name": "add", "context": {"input": 3}}}),
    ])));
    let transport = CallbackTransport::new(
        |_| async { Ok(()) },
        move || {
            let message = messages.lock().unwrap().pop_front();
            async move { Ok(message) }
        },
    );
    let mut runtime = app.build();
    runtime.start_with(transport).await.unwrap();
    assert_eq!(runtime.read(|state| *state).await, 5);
    assert!(matches!(
        runtime.invoke("add", json!("invalid")).await,
        Err(Error::Protocol(_))
    ));
    assert_eq!(runtime.read(|state| *state).await, 5);
}

struct TaskStopped(Arc<AtomicBool>);
impl Drop for TaskStopped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn direct_shutdown_closes_resources_after_stopping_tasks() {
    struct ReadOnDrop {
        resources: ResourceRegistry,
        request: Value,
        readable: Arc<AtomicBool>,
    }
    impl Drop for ReadOnDrop {
        fn drop(&mut self) {
            self.readable.store(
                self.resources.respond(&self.request)["status"] == 200,
                Ordering::SeqCst,
            );
        }
    }

    let mut runtime = App::new(()).build();
    let resources = runtime.resources.clone();
    let uri = resources.bytes(b"media", "video/mp4").unwrap();
    let id = uri.rsplit('/').next().unwrap();
    let requests = ["stat", "read"].map(|op| {
        json!({"omochi":"resources/1","kind":"request","id":"test",
            "resource":id,"op":op,"offset":0,"length":5})
    });
    // Locate this resource's private copy without changing the process-wide temp directory.
    let directory = std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("omochi-resources-")
                && path.join(id).is_file()
        })
        .expect("registered resource must have an owned storage directory");
    assert_eq!(std::fs::read(directory.join(id)).unwrap(), b"media");
    for request in &requests {
        assert_eq!(resources.respond(request)["status"], 200);
    }

    let readable = Arc::new(AtomicBool::new(false));
    let guard = ReadOnDrop {
        resources: resources.clone(),
        request: requests[1].clone(),
        readable: readable.clone(),
    };
    let entered = Arc::new(Notify::new());
    let notification = entered.clone();
    runtime.spawn(async move {
        let _guard = guard;
        notification.notify_one();
        std::future::pending::<Result<()>>().await
    });
    entered.notified().await;

    for _ in 0..2 {
        runtime.shutdown().await;
        assert!(runtime.is_closed());
        assert!(readable.load(Ordering::SeqCst));
        for request in &requests {
            assert_eq!(resources.respond(request)["status"], 404);
        }
        assert!(resources.bytes(b"new", "video/mp4").is_err());
        assert!(!directory.exists(), "shutdown must remove owned storage");
    }
}

#[tokio::test]
async fn shutdown_runs_on_every_exit_and_preserves_the_original_error() {
    for stage in [
        "render",
        "initial send",
        "receive",
        "action",
        "response send",
        "eof",
    ] {
        let mut app = App::new(());
        app.surface("main", move |ui, _| {
            if stage == "render" {
                return Err(Error::Declaration(stage.into()));
            }
            ui.text("hello")
        })
        .unwrap();
        app.action::<(), _, _>("fail", |_, _| async {
            Err(Error::Protocol("action".into()))
        })
        .unwrap();
        let mut runtime = app.build();
        let resources = runtime.resources.clone();
        let uri = resources.bytes(b"media", "video/mp4").unwrap();
        let resource = uri.rsplit('/').next().unwrap();
        let stopped = Arc::new(AtomicBool::new(false));
        let entered = Arc::new(Notify::new());
        let notification = entered.clone();
        let guard = TaskStopped(stopped.clone());
        runtime.spawn(async move {
            let _guard = guard;
            notification.notify_one();
            std::future::pending::<Result<()>>().await
        });
        entered.notified().await;

        let responding = Arc::new(AtomicBool::new(false));
        let sending = responding.clone();
        let transport = CallbackTransport::new(
            move |_| {
                let fail = stage == "initial send"
                    || stage == "response send" && sending.load(Ordering::SeqCst);
                async move {
                    if fail {
                        Err(Error::Transport(stage.into()))
                    } else {
                        Ok(())
                    }
                }
            },
            move || {
                responding.store(true, Ordering::SeqCst);
                async move {
                    match stage {
                        "receive" => Err(Error::Transport(stage.into())),
                        "action" => Ok(Some(json!({"type": "invoke", "action": "fail"}))),
                        "response send" => {
                            Ok(Some(json!({"type": "getSurface", "surface": "main"})))
                        }
                        _ => Ok(None),
                    }
                }
            },
        );
        let result = runtime.start_with(transport).await;
        match stage {
            "eof" => assert!(result.is_ok()),
            "render" => assert!(matches!(result, Err(Error::Declaration(ref s)) if s == stage)),
            "action" => assert!(matches!(result, Err(Error::Protocol(ref s)) if s == stage)),
            _ => assert!(matches!(result, Err(Error::Transport(ref s)) if s == stage)),
        }
        assert!(runtime.is_closed(), "runtime remained open after {stage}");
        assert!(stopped.load(Ordering::SeqCst), "task survived {stage}");
        for op in ["stat", "read"] {
            let request = json!({"kind":"request","id":"test","op":op,
                "resource":resource,"offset":0,"length":5});
            assert_eq!(resources.respond(&request)["status"], 404, "after {stage}");
        }
        assert!(
            resources.bytes(b"new", "video/mp4").is_err(),
            "after {stage}"
        );
    }
}
