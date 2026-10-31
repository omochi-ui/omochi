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
#[derive(Default)]
struct Schedule {
    next: Option<Instant>,
    manual_restart: bool,
    suppressed: bool,
}
impl Schedule {
    async fn wait(&self) {
        match self.next {
            Some(at) => tokio::time::sleep_until(at).await,
            None => std::future::pending().await,
        }
    }
    fn start(&mut self) {
        self.next = None;
        self.suppressed = false;
        self.manual_restart = false;
    }
    fn stop(&mut self) {
        self.next = None;
        self.suppressed = true;
        self.manual_restart = false;
    }
    fn restart(&mut self, running: bool) {
        self.stop();
        if running {
            self.manual_restart = true;
        } else {
            self.start();
        }
    }
    // Return true only when a pending manual restart should launch immediately.
    fn finished(&mut self, policy: Restart, outcome: &Outcome, delay: Duration) -> bool {
        if self.manual_restart {
            self.start();
            return true;
        }
        self.next = if !self.suppressed && policy.allows(outcome) {
            Some(Instant::now() + delay)
        } else {
            None
        };
        false
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
    let mut schedule = Schedule::default();
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
                _ = schedule.wait() => {
                    schedule.start(); count += 1; current = Some(launch(&options,&mut output,true));
                    status = "Running…".into(); dirty = true;
                },
                page = publisher.events.read(cursor,30000,&events_cancel) => {
                    let page = page?; cursor = page["nextCursor"].as_u64().unwrap();
                    for event in page["events"].as_array().unwrap() {
                        let action = &event["message"]["action"];
                        if action["surfaceId"] != "main" { continue; }
                        match action["name"].as_str() {
                            Some("start") if current.is_none() => {
                                schedule.start();
                                current = Some(launch(&options,&mut output,false)); status = "Running…".into(); dirty = true;
                            },
                            Some("stop") => {
                                schedule.stop();
                                if let Some(run) = &current { run.stop.cancel(); }
                                status = if current.is_some() { "Stopping…" } else { "Stopped" }.into(); dirty = true;
                            },
                            Some("restart") => {
                                schedule.restart(current.is_some());
                                if let Some(run) = &current { run.stop.cancel(); }
                                else { current = Some(launch(&options,&mut output,false)); status = "Running…".into(); }
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
                    status = if schedule.suppressed { "Stopped".into() } else { outcome.describe() }; dirty = true;
                    if schedule.finished(policy, &outcome, delay) {
                        current = Some(launch(&options,&mut output,false)); status = "Running…".into();
                    } else if schedule.next.is_some() {
                        status.push_str(" · restarting soon…");
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::poll;

    fn outcome(code: i32) -> Outcome {
        Outcome {
            code: Some(code),
            signal: None,
            error: None,
            timed_out: false,
            duration: Duration::ZERO,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn restart_policy_schedules_only_allowed_outcomes_after_the_full_delay() {
        for (policy, code, automatic) in [
            (Restart::Never, 0, false),
            (Restart::Never, 2, false),
            (Restart::OnFailure, 0, false),
            (Restart::OnFailure, 2, true),
            (Restart::Always, 0, true),
            (Restart::Always, 2, true),
        ] {
            let mut schedule = Schedule::default();
            schedule.start();
            assert!(!schedule.finished(policy, &outcome(code), Duration::from_secs(5)));
            let wait = schedule.wait();
            tokio::pin!(wait);
            assert!(poll!(&mut wait).is_pending());
            tokio::time::advance(Duration::from_secs(4)).await;
            assert!(poll!(&mut wait).is_pending());
            tokio::time::advance(Duration::from_secs(1)).await;
            assert_eq!(poll!(&mut wait).is_ready(), automatic);
            if !automatic {
                tokio::time::advance(Duration::from_secs(3600)).await;
                assert!(poll!(&mut wait).is_pending());
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn stop_cancels_scheduled_restarts_and_suppresses_in_flight_completion() {
        for already_completed in [false, true] {
            let mut schedule = Schedule::default();
            schedule.start();
            if already_completed {
                schedule.finished(Restart::Always, &outcome(0), Duration::from_secs(5));
            }
            schedule.stop();
            if !already_completed {
                assert!(!schedule.finished(Restart::Always, &outcome(2), Duration::from_secs(5)));
            }
            tokio::time::advance(Duration::from_secs(3600)).await;
            assert!(poll!(std::pin::pin!(schedule.wait())).is_pending());
            // A manual restart of an idle process launches immediately in the caller.
            schedule.restart(false);
            assert!(!schedule.suppressed);
            assert!(!schedule.manual_restart);
            schedule.finished(Restart::Always, &outcome(0), Duration::from_secs(5));
            assert!(schedule.next.is_some());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn manual_restart_waits_for_reaping_and_a_later_stop_cancels_it() {
        let mut schedule = Schedule::default();
        schedule.restart(true);
        assert!(poll!(std::pin::pin!(schedule.wait())).is_pending());
        assert!(schedule.finished(Restart::Never, &outcome(0), Duration::from_secs(5)));
        assert!(!schedule.manual_restart);
        schedule.restart(true);
        schedule.stop();
        assert!(!schedule.finished(Restart::Always, &outcome(0), Duration::from_secs(5)));
        assert!(poll!(std::pin::pin!(schedule.wait())).is_pending());
    }
}
