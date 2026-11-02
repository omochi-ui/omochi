use omochi_ui::{App, CallbackTransport, Error, Result};
use serde_json::json;
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
    }
}
