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
// Scheduling is independent of OS process timing. `pause` prevents future runs;
// an already running command is allowed to finish.
struct Schedule {
    paused: bool,
    next: Option<Instant>,
}
impl Schedule {
    async fn wait(&self) {
        match self.next {
            Some(at) => tokio::time::sleep_until(at).await,
            None => std::future::pending().await,
        }
    }
    fn new() -> Self {
        Self {
            paused: false,
            next: Some(Instant::now()),
        }
    }
    fn pause(&mut self) {
        self.paused = true;
        self.next = None;
    }
    fn resume(&mut self, running: bool) {
        self.paused = false;
        if !running {
            self.next = Some(Instant::now());
        }
    }
    fn finished(&mut self, interval: Duration) {
        self.next = if self.paused {
            None
        } else {
            Some(Instant::now() + interval)
        };
    }
}
#[derive(Default)]
struct PreviousRun(Option<(String, String)>);
impl PreviousRun {
    fn changed(&mut self, description: String, text: String) -> bool {
        let changed = self
            .0
            .as_ref()
            .is_none_or(|previous| previous.0 != description || previous.1 != text);
        self.0 = Some((description, text));
        changed
    }
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
    let mut schedule = Schedule::new();
    let mut previous = PreviousRun::default();
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
                    let mut controls = vec![if schedule.paused { ("resume","Resume") } else { ("pause","Pause") }];
                    if current.is_none() { controls.push(("run","Run now")); }
                    let mut sections = Vec::new();
                    if options.diff { sections.push(("diff","Changes since previous run",difference.fenced())); }
                    if !history.is_empty() { sections.push(("history","History",history.iter().cloned().collect::<Vec<_>>().join("\n"))); }
                    surface.render_sections(&format!("{status}{}",if schedule.paused { " · Paused" } else { "" }),&displayed,&controls,&sections).await?;
                    dirty = false;
                },
                _ = schedule.wait() => {
                    schedule.next = None; output.reset("");
                    if !options.on_change { status = "Running…".into(); displayed.reset(""); dirty = true; }
                    current = Some(Running::start(options.command.clone(),options.timeout));
                },
                page = publisher.events.read(cursor,30000,&cancel_events) => {
                    let page = page?; cursor = page["nextCursor"].as_u64().unwrap();
                    for event in page["events"].as_array().unwrap() {
                        let action = &event["message"]["action"];
                        if action["surfaceId"] != "main" { continue; }
                        match action["name"].as_str() {
                            Some("pause") if !schedule.paused => { schedule.pause(); dirty = true; },
                            Some("resume") if schedule.paused => { schedule.resume(current.is_some()); dirty = true; },
                            Some("run") if current.is_none() => { schedule.next = Some(Instant::now()); },
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
                    if options.diff {
                        difference.reset(&match &previous.0 {
                            Some((_,before)) if before == &output.text => "No changes.".into(),
                            Some((_,before)) => similar::TextDiff::configure().timeout(Duration::from_millis(50)).diff_lines(before,&output.text).unified_diff().context_radius(3).header("previous","current").to_string(),
                            None => "No previous run.".into(),
                        });
                    }
                    let changed = previous.changed(description.clone(),output.text.clone());
                    let at = chrono::Utc::now().to_rfc3339();
                    status = format!("{description} · {} ms · {at}",outcome.duration.as_millis());
                    if options.history > 0 {
                        history.push_back(format!("- {at} · {description} · {} ms · {}",outcome.duration.as_millis(),if changed { "changed" } else { "unchanged" }));
                        while history.len() > options.history { history.pop_front(); }
                    }
                    if !options.on_change || changed { displayed = output.clone(); dirty = true; }
                    schedule.finished(options.interval);
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
    use futures_util::poll;

    #[tokio::test(start_paused = true)]
    async fn pause_cancels_pending_runs_and_completion_cannot_rearm_them() {
        let mut schedule = Schedule::new();
        schedule.wait().await;
        schedule.next = None; // The controlled command has started.
        schedule.pause();
        // Completing a command after pause must not schedule another one.
        schedule.finished(Duration::from_secs(5));
        tokio::time::advance(Duration::from_secs(3600)).await;
        assert!(poll!(std::pin::pin!(schedule.wait())).is_pending());
        schedule.resume(false);
        schedule.wait().await;
        schedule.finished(Duration::from_secs(5));
        schedule.pause(); // Also cancel a restart that was already scheduled.
        tokio::time::advance(Duration::from_secs(3600)).await;
        assert!(poll!(std::pin::pin!(schedule.wait())).is_pending());
    }

    #[tokio::test(start_paused = true)]
    async fn resume_during_a_run_waits_for_completion_then_the_full_interval() {
        let mut schedule = Schedule::new();
        schedule.next = None;
        schedule.pause();
        schedule.resume(true);
        assert!(poll!(std::pin::pin!(schedule.wait())).is_pending());
        schedule.finished(Duration::from_secs(5));
        let wait = schedule.wait();
        tokio::pin!(wait);
        assert!(poll!(&mut wait).is_pending());
        tokio::time::advance(Duration::from_secs(4)).await;
        assert!(poll!(&mut wait).is_pending());
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(poll!(&mut wait).is_ready());
    }

    #[test]
    fn identical_completed_runs_are_suppressed_but_output_or_result_changes_are_visible() {
        let mut previous = PreviousRun::default();
        assert!(previous.changed("Exited with code 0".into(), "same".into()));
        for _ in 0..100 {
            assert!(!previous.changed("Exited with code 0".into(), "same".into()));
        }
        assert!(previous.changed("Exited with code 0".into(), "different".into()));
        assert!(previous.changed("Timed out".into(), "different".into()));
        assert!(!previous.changed("Timed out".into(), "different".into()));
    }
}
