//! A shell-free command session with bounded output and process-group teardown.
use crate::output::Decoder;
use std::{
    os::unix::process::ExitStatusExt,
    path::PathBuf,
    process::{ExitStatus, Stdio},
    time::{Duration, Instant},
};
use tokio::{io::AsyncReadExt, process::Command, sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
pub struct Options {
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
}
pub struct Outcome {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub error: Option<String>,
    pub timed_out: bool,
    pub duration: Duration,
}
impl Outcome {
    pub fn describe(&self) -> String {
        if let Some(error) = &self.error {
            format!("Failed to start: {error}")
        } else if self.timed_out {
            "Timed out".into()
        } else if let Some(signal) = self.signal {
            format!(
                "Stopped by {}",
                match signal {
                    libc::SIGTERM => "SIGTERM",
                    libc::SIGKILL => "SIGKILL",
                    libc::SIGINT => "SIGINT",
                    _ => "signal",
                }
            )
        } else {
            format!("Exited with code {}", self.code.unwrap_or(-1))
        }
    }
    pub fn failed(&self) -> bool {
        self.error.is_some() || self.signal.is_some() || self.code != Some(0)
    }
}
pub struct Running {
    pub output: mpsc::Receiver<String>,
    pub result: JoinHandle<Outcome>,
    pub stop: CancellationToken,
}
struct Group(i32);
impl Group {
    fn signal(&self, signal: i32) {
        unsafe {
            libc::kill(-self.0, signal);
        }
    }
}
impl Drop for Group {
    fn drop(&mut self) {
        self.signal(libc::SIGKILL);
    }
}
impl Running {
    pub fn start(options: Options, timeout: Option<Duration>) -> Self {
        let (output, receiver) = mpsc::channel(8);
        let stop = CancellationToken::new();
        let cancel = stop.clone();
        let result = tokio::spawn(async move {
            let started = Instant::now();
            let mut command = Command::new(options.argv.first().map(String::as_str).unwrap_or(""));
            command
                .args(options.argv.iter().skip(1))
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .process_group(0)
                .kill_on_drop(true);
            if let Some(cwd) = options.cwd {
                command.current_dir(cwd);
            }
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(error) => {
                    return Outcome {
                        code: None,
                        signal: None,
                        error: Some(error.to_string()),
                        timed_out: false,
                        duration: started.elapsed(),
                    };
                }
            };
            let group = Group(child.id().unwrap() as i32);
            let mut stdout = child.stdout.take().unwrap();
            let mut stderr = child.stderr.take().unwrap();
            let mut out_decoder = Decoder::default();
            let mut err_decoder = Decoder::default();
            let mut out_bytes = [0u8; 8192];
            let mut err_bytes = [0u8; 8192];
            let mut out_open = true;
            let mut err_open = true;
            let mut status: Option<ExitStatus> = None;
            let mut stopping = false;
            let mut timed_out = false;
            let mut deadline =
                tokio::time::Instant::now() + timeout.unwrap_or(Duration::from_secs(86400 * 365));
            loop {
                let chunk = tokio::select! {
                    _ = cancel.cancelled(), if !stopping => { stopping = true; group.signal(libc::SIGTERM); deadline = tokio::time::Instant::now()+Duration::from_secs(1); None },
                    _ = tokio::time::sleep_until(deadline) => {
                        if stopping { group.signal(libc::SIGKILL); out_open = false; err_open = false; }
                        else { timed_out = true; stopping = true; group.signal(libc::SIGTERM); }
                        deadline = tokio::time::Instant::now()+Duration::from_secs(1); None
                    },
                    result = child.wait(), if status.is_none() => {
                        status = result.ok();
                        // A completed command must not leave descendants holding its output pipes.
                        group.signal(libc::SIGKILL);
                        deadline = tokio::time::Instant::now()+Duration::from_secs(1);
                        stopping = true; None
                    },
                    read = stdout.read(&mut out_bytes), if out_open => match read {
                        Ok(0)|Err(_) => { out_open = false; Some(out_decoder.finish()) },
                        Ok(n) => Some(out_decoder.write(&out_bytes[..n])),
                    },
                    read = stderr.read(&mut err_bytes), if err_open => match read {
                        Ok(0)|Err(_) => { err_open = false; Some(err_decoder.finish()) },
                        Ok(n) => Some(err_decoder.write(&err_bytes[..n])),
                    },
                };
                if let Some(chunk) = chunk.filter(|c| !c.is_empty()) {
                    tokio::select! { _ = cancel.cancelled() => {}, _ = output.send(chunk) => {} }
                }
                if status.is_some() && !out_open && !err_open {
                    break;
                }
            }
            Outcome {
                code: status.and_then(|s| s.code()),
                signal: status.and_then(|s| s.signal()),
                error: None,
                timed_out,
                duration: started.elapsed(),
            }
        });
        Self {
            output: receiver,
            result,
            stop,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn captures_both_streams_and_times_out_term_resistant_group() {
        let mut run = Running::start(
            Options {
                argv: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "printf stdout; printf stderr >&2".into(),
                ],
                cwd: None,
            },
            None,
        );
        let mut output = String::new();
        while let Some(chunk) = run.output.recv().await {
            output.push_str(&chunk);
        }
        assert_eq!(run.result.await.unwrap().code, Some(0));
        assert!(output.contains("stdout") && output.contains("stderr"));
        let run = Running::start(
            Options {
                argv: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "trap '' TERM; while :; do :; done".into(),
                ],
                cwd: None,
            },
            Some(Duration::from_millis(30)),
        );
        let result = tokio::time::timeout(Duration::from_secs(3), run.result)
            .await
            .unwrap()
            .unwrap();
        assert!(result.timed_out);
        assert_eq!(result.signal, Some(libc::SIGKILL));
    }
}
