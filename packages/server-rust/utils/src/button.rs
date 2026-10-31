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
                            current = Some(Running::start(options.clone(),None));
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
