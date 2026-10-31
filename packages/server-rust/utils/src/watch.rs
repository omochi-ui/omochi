use crate::{
    command::{Options, Running},
    output::OutputBuffer,
    publisher::Publisher,
    surface::TextSurface,
};
use anyhow::Result;
use std::{collections::VecDeque, time::Duration};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
pub struct WatchOptions {
    pub command: Options,
    pub interval: Duration,
    pub timeout: Option<Duration>,
    pub diff: bool,
    pub on_change: bool,
    pub history: usize,
}
pub async fn run(
    publisher: Publisher,
    name: String,
    options: WatchOptions,
    stop: CancellationToken,
) -> Result<()> {
    let mut surface = TextSurface::new(publisher.clone(), name);
    let mut current: Option<Running> = None;
    let mut output = OutputBuffer::default();
    let mut displayed = OutputBuffer::default();
    let mut status = "Starting…".to_owned();
    let mut cursor = 0;
    let mut dirty = true;
    let mut paused = false;
    let mut next = Some(Instant::now());
    let mut previous: Option<(String, String)> = None;
    let mut difference = OutputBuffer::default();
    difference.reset("No previous run.");
    let mut history = VecDeque::new();
    let cancel_events = CancellationToken::new();
    let mut interval = tokio::time::interval(Duration::from_millis(50));
    let result = async {
        loop {
            let (receiver,result) = match current.as_mut() { Some(run)=>(Some(&mut run.output),Some(&mut run.result)),None=>(None,None) };
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = interval.tick(), if dirty => {
                    let mut controls = vec![if paused { ("resume","Resume") } else { ("pause","Pause") }];
                    if current.is_none() { controls.push(("run","Run now")); }
                    let mut sections = Vec::new();
                    if options.diff { sections.push(("diff","Changes since previous run",difference.fenced())); }
                    if !history.is_empty() { sections.push(("history","History",history.iter().cloned().collect::<Vec<_>>().join("\n"))); }
                    surface.render_sections(&format!("{status}{}",if paused { " · Paused" } else { "" }),&displayed,&controls,&sections).await?;
                    dirty = false;
                },
                _ = async { match next { Some(at)=>tokio::time::sleep_until(at).await,None=>std::future::pending().await } } => {
                    next = None; output.reset("");
                    if !options.on_change { status = "Running…".into(); displayed.reset(""); dirty = true; }
                    current = Some(Running::start(options.command.clone(),options.timeout));
                },
                page = publisher.events.read(cursor,30000,&cancel_events) => {
                    let page = page?; cursor = page["nextCursor"].as_u64().unwrap();
                    for event in page["events"].as_array().unwrap() {
                        let action = &event["message"]["action"];
                        if action["surfaceId"] != "main" { continue; }
                        match action["name"].as_str() {
                            Some("pause") if !paused => { paused = true; next = None; dirty = true; },
                            Some("resume") if paused => { paused = false; if current.is_none() { next = Some(Instant::now()); } dirty = true; },
                            Some("run") if current.is_none() => { next = Some(Instant::now()); },
                            _=>{},
                        }
                    }
                    if page["closed"] == true { break; }
                },
                chunk = async { match receiver { Some(receiver)=>receiver.recv().await,None=>std::future::pending().await } } => {
                    if let Some(chunk) = chunk {
                        output.append(&chunk);
                        if !options.on_change { displayed = output.clone(); dirty = true; }
                    } else { tokio::task::yield_now().await; }
                },
                outcome = async { match result { Some(result)=>result.await,None=>std::future::pending().await } } => {
                    let outcome = outcome?;
                    if let Some(mut finished) = current.take() { while let Ok(chunk) = finished.output.try_recv() { output.append(&chunk); } }
                    let description = outcome.describe();
                    let changed = previous.as_ref().is_none_or(|(result,text)| result != &description || text != &output.text);
                    if options.diff {
                        difference.reset(&match &previous {
                            Some((_,before)) if before == &output.text => "No changes.".into(),
                            Some((_,before)) => similar::TextDiff::configure().timeout(Duration::from_millis(50)).diff_lines(before,&output.text).unified_diff().context_radius(3).header("previous","current").to_string(),
                            None => "No previous run.".into(),
                        });
                    }
                    previous = Some((description.clone(),output.text.clone()));
                    let at = chrono::Utc::now().to_rfc3339();
                    status = format!("{description} · {} ms · {at}",outcome.duration.as_millis());
                    if options.history > 0 {
                        history.push_back(format!("- {at} · {description} · {} ms · {}",outcome.duration.as_millis(),if changed { "changed" } else { "unchanged" }));
                        while history.len() > options.history { history.pop_front(); }
                    }
                    if !options.on_change || changed { displayed = output.clone(); dirty = true; }
                    if !paused { next = Some(Instant::now()+options.interval); }
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
