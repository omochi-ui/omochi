use crate::{
    command::{Options, Outcome, Running},
    output::OutputBuffer,
    publisher::Publisher,
    surface::TextSurface,
};
use anyhow::Result;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Restart {
    Never,
    OnFailure,
    Always,
}
impl Restart {
    fn allows(self, outcome: &Outcome) -> bool {
        match self {
            Self::Never => false,
            Self::Always => true,
            Self::OnFailure => outcome.failed(),
        }
    }
}
fn launch(options: &Options, output: &mut OutputBuffer, automatic: bool) -> Running {
    output.append(&format!(
        "\n--- {} at {} ---\n",
        if automatic {
            "Automatically restarted"
        } else {
            "Started"
        },
        chrono::Utc::now().to_rfc3339()
    ));
    Running::start(options.clone(), None)
}
pub async fn run(
    publisher: Publisher,
    name: String,
    options: Options,
    policy: Restart,
    delay: Duration,
    stop: CancellationToken,
) -> Result<()> {
    let mut surface = TextSurface::new(publisher.clone(), name);
    let mut current: Option<Running> = None;
    let mut output = OutputBuffer::default();
    let mut status = "Stopped".to_owned();
    let mut cursor = 0;
    let mut dirty = true;
    let mut count = 0;
    let mut next: Option<Instant> = None;
    let mut manual_restart = false;
    let mut suppressed = false;
    let events_cancel = CancellationToken::new();
    let mut interval = tokio::time::interval(Duration::from_millis(50));
    let result = async {
        loop {
            let (receiver,result) = match current.as_mut() { Some(run)=>(Some(&mut run.output),Some(&mut run.result)),None=>(None,None) };
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = interval.tick(), if dirty => {
                    let controls: &[(&str,&str)] = if current.is_some() { &[("stop","Stop"),("restart","Restart")] } else { &[("start","Start")] };
                    surface.render_controls(&format!("{status} · automatic restarts: {count}"),&output,controls).await?;
                    dirty = false;
                },
                _ = async { match next { Some(at)=>tokio::time::sleep_until(at).await,None=>std::future::pending().await } } => {
                    next = None; count += 1; current = Some(launch(&options,&mut output,true));
                    status = "Running…".into(); suppressed = false; dirty = true;
                },
                page = publisher.events.read(cursor,30000,&events_cancel) => {
                    let page = page?; cursor = page["nextCursor"].as_u64().unwrap();
                    for event in page["events"].as_array().unwrap() {
                        let action = &event["message"]["action"];
                        if action["surfaceId"] != "main" { continue; }
                        match action["name"].as_str() {
                            Some("start") if current.is_none() => {
                                next = None; suppressed = false; manual_restart = false;
                                current = Some(launch(&options,&mut output,false)); status = "Running…".into(); dirty = true;
                            },
                            Some("stop") => {
                                next = None; suppressed = true; manual_restart = false;
                                if let Some(run) = &current { run.stop.cancel(); }
                                status = "Stopped".into(); dirty = true;
                            },
                            Some("restart") => {
                                next = None; suppressed = true;
                                if let Some(run) = &current { manual_restart = true; run.stop.cancel(); }
                                else { suppressed = false; current = Some(launch(&options,&mut output,false)); status = "Running…".into(); }
                                dirty = true;
                            },
                            _=>{},
                        }
                    }
                    if page["closed"] == true { break; }
                },
                chunk = async { match receiver { Some(receiver)=>receiver.recv().await,None=>std::future::pending().await } } => {
                    if let Some(chunk) = chunk { output.append(&chunk); dirty = true; }
                    else { tokio::task::yield_now().await; }
                },
                outcome = async { match result { Some(result)=>result.await,None=>std::future::pending().await } } => {
                    let outcome = outcome?;
                    if let Some(mut finished) = current.take() { while let Ok(chunk) = finished.output.try_recv() { output.append(&chunk); } }
                    output.append(&format!("\n--- {} at {} ---\n",outcome.describe(),chrono::Utc::now().to_rfc3339()));
                    status = if suppressed { "Stopped".into() } else { outcome.describe() }; dirty = true;
                    if manual_restart {
                        manual_restart = false; suppressed = false;
                        current = Some(launch(&options,&mut output,false)); status = "Running…".into();
                    } else if !suppressed && policy.allows(&outcome) {
                        status.push_str(" · restarting soon…"); next = Some(Instant::now()+delay);
                    }
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
