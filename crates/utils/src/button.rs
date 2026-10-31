use crate::{
    command::{Options, Running},
    output::OutputBuffer,
    publisher::Publisher,
    surface::TextSurface,
};
use anyhow::Result;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn run(
    publisher: Publisher,
    name: String,
    options: Options,
    stop: CancellationToken,
) -> Result<()> {
    run_with(publisher, name, options, stop, |options| {
        Running::start(options, None)
    })
    .await
}
async fn run_with(
    publisher: Publisher,
    name: String,
    options: Options,
    stop: CancellationToken,
    mut launch: impl FnMut(Options) -> Running,
) -> Result<()> {
    let mut surface = TextSurface::new(publisher.clone(), name);
    let mut current: Option<Running> = None;
    let mut output = OutputBuffer::default();
    let mut status = "Ready".to_owned();
    let mut cursor = 0;
    let mut dirty = true;
    let events_cancel = CancellationToken::new();
    let mut interval = tokio::time::interval(Duration::from_millis(50));
    let result = async {
        loop {
            let (receiver, result) = match current.as_mut() { Some(run) => (Some(&mut run.output),Some(&mut run.result)), None => (None,None) };
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = interval.tick(), if dirty => {
                    let controls: &[(&str,&str)] = if current.is_some() { &[] } else { &[("run","Run")] };
                    surface.render_controls(&status,&output,controls).await?; dirty = false;
                },
                page = publisher.events.read(cursor,30000,&events_cancel) => {
                    let page = page?; cursor = page["nextCursor"].as_u64().unwrap();
                    for event in page["events"].as_array().unwrap() {
                        let action = &event["message"]["action"];
                        if action["surfaceId"] == "main" && action["name"] == "run" && current.is_none() {
                            output.reset(""); status = "Running…".into(); dirty = true;
                            current = Some(launch(options.clone()));
                        }
                    }
                    if page["closed"] == true { break; }
                },
                chunk = async { match receiver { Some(receiver) => receiver.recv().await, None => std::future::pending().await } } => {
                    if let Some(chunk) = chunk { output.append(&chunk); dirty = true; }
                    else { tokio::task::yield_now().await; }
                },
                result = async { match result { Some(result) => result.await, None => std::future::pending().await } } => {
                    let result = result?;
                    if let Some(mut finished) = current.take() { while let Ok(chunk) = finished.output.try_recv() { output.append(&chunk); } }
                    status = result.describe(); dirty = true;
                },
            }
        }
        Ok(())
    }.await;
    if let Some(run) = current {
        run.stop.cancel();
        let _ = run.result.await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Outcome;
    use omochi_ui::UnixSocketServerOptions;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn duplicate_actions_do_not_start_another_command_before_completion() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let (publisher, task) =
            Publisher::open(UnixSocketServerOptions::new("button").with_runtime_dir(root.path()))
                .await
                .unwrap();
        for _ in 0..3 {
            publisher.events.push(serde_json::json!({
                "version":"v0.9", "action":{"surfaceId":"main", "name":"run", "context":{}}
            }));
        }
        // The closed page is an explicit end-of-input barrier: run_with must
        // consume all queued actions, cancel the command and join it before returning.
        publisher.events.close();
        let mut launches = 0;
        tokio::time::timeout(
            Duration::from_secs(5),
            run_with(
                publisher,
                "Button".into(),
                Options {
                    argv: vec![],
                    cwd: None,
                },
                CancellationToken::new(),
                |_| {
                    launches += 1;
                    let (_output, receiver) = tokio::sync::mpsc::channel(1);
                    let stop = CancellationToken::new();
                    let cancelled = stop.clone();
                    let result = tokio::spawn(async move {
                        cancelled.cancelled().await;
                        Outcome {
                            code: None,
                            signal: Some(libc::SIGTERM),
                            error: None,
                            timed_out: false,
                            duration: Duration::ZERO,
                        }
                    });
                    Running {
                        output: receiver,
                        result,
                        stop,
                    }
                },
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(launches, 1);
        task.close().await.unwrap();
    }
}
