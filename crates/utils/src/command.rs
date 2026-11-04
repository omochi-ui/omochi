//! A shell-free command session with bounded output and process-group teardown.
use crate::output::Decoder;
use std::{
    future::Future,
    os::unix::process::ExitStatusExt,
    path::PathBuf,
    process::{ExitStatus, Stdio},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::mpsc,
    task::JoinHandle,
};
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
    /// Bounded output in read order. Undelivered chunks are discarded on timeout
    /// or cancellation, or after one second of blocked flushing following exit.
    /// Chunks already queued remain available after the result completes.
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
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();
            let (status, timed_out) = supervise(
                stdout,
                stderr,
                child.wait(),
                |signal| group.signal(signal),
                output,
                cancel,
                timeout,
            )
            .await;
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
// Keep output backpressure inside the lifecycle select. At most one decoded
// chunk waits outside the bounded channel; no further pipe reads can overtake it.
async fn supervise(
    mut stdout: impl AsyncRead + Unpin,
    mut stderr: impl AsyncRead + Unpin,
    wait: impl Future<Output = std::io::Result<ExitStatus>>,
    mut signal: impl FnMut(i32),
    output: mpsc::Sender<String>,
    cancel: CancellationToken,
    timeout: Option<Duration>,
) -> (Option<ExitStatus>, bool) {
    tokio::pin!(wait);
    let mut out_decoder = Decoder::default();
    let mut err_decoder = Decoder::default();
    let mut out_bytes = [0u8; 8192];
    let mut err_bytes = [0u8; 8192];
    let mut out_open = true;
    let mut err_open = true;
    let mut status = None;
    let mut waited = false;
    let mut stopping = false;
    let mut timed_out = false;
    let mut discard_output = false;
    let mut pending = None;
    let mut deadline =
        tokio::time::Instant::now() + timeout.unwrap_or(Duration::from_secs(86400 * 365));
    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled(), if !stopping => {
                stopping = true;
                discard_output = true;
                pending = None;
                signal(libc::SIGTERM);
                deadline = tokio::time::Instant::now() + Duration::from_secs(1);
                None
            },
            _ = tokio::time::sleep_until(deadline) => {
                // Timeout/cancel may drop output that has not reached the channel.
                // Normal exit gets one second to flush, then follows the same rule.
                discard_output = true;
                pending = None;
                if stopping {
                    signal(libc::SIGKILL);
                    out_open = false;
                    err_open = false;
                } else {
                    timed_out = true;
                    stopping = true;
                    signal(libc::SIGTERM);
                }
                deadline = tokio::time::Instant::now() + Duration::from_secs(1);
                None
            },
            result = &mut wait, if !waited => {
                waited = true;
                status = result.ok();
                // A completed command must not leave descendants holding its pipes.
                signal(libc::SIGKILL);
                deadline = tokio::time::Instant::now() + Duration::from_secs(1);
                stopping = true;
                None
            },
            permit = output.reserve(), if pending.is_some() => {
                match permit {
                    Ok(permit) => { permit.send(pending.take().unwrap()); },
                    Err(_) => {
                        pending = None;
                        discard_output = true;
                    },
                }
                None
            },
            read = stdout.read(&mut out_bytes), if out_open && pending.is_none() => match read {
                Ok(0) | Err(_) => { out_open = false; Some(out_decoder.finish()) },
                Ok(n) => Some(out_decoder.write(&out_bytes[..n])),
            },
            read = stderr.read(&mut err_bytes), if err_open && pending.is_none() => match read {
                Ok(0) | Err(_) => { err_open = false; Some(err_decoder.finish()) },
                Ok(n) => Some(err_decoder.write(&err_bytes[..n])),
            },
        };
        if let Some(chunk) = chunk.filter(|chunk| !discard_output && !chunk.is_empty()) {
            pending = Some(chunk);
        }
        if waited && !out_open && !err_open && pending.is_none() {
            break;
        }
    }
    (status, timed_out)
}
#[cfg(test)]
mod tests {
    use super::*;

    use std::{
        pin::Pin,
        task::{Context, Poll},
    };
    use tokio::{io::ReadBuf, sync::oneshot};

    struct ControlledOutput {
        chunks: mpsc::Receiver<Vec<u8>>,
        read: mpsc::UnboundedSender<()>,
    }

    impl AsyncRead for ControlledOutput {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            match self.chunks.poll_recv(cx) {
                Poll::Ready(Some(bytes)) => {
                    buf.put_slice(&bytes);
                    self.read.send(()).unwrap();
                    Poll::Ready(Ok(()))
                }
                Poll::Ready(None) => Poll::Ready(Ok(())),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    struct ControlledCommand {
        result: JoinHandle<(Option<ExitStatus>, bool)>,
        output: mpsc::Receiver<String>,
        stop: CancellationToken,
        exit: oneshot::Sender<ExitStatus>,
        signals: mpsc::UnboundedReceiver<i32>,
        chunks: mpsc::Sender<Vec<u8>>,
        read: mpsc::UnboundedReceiver<()>,
    }

    async fn backpressured(timeout: Option<Duration>) -> ControlledCommand {
        let (output, receiver) = mpsc::channel(1);
        output.send("queued".into()).await.unwrap();
        let (chunks, input) = mpsc::channel(8);
        let (read_tx, mut read) = mpsc::unbounded_channel();
        let (exit, exited) = oneshot::channel();
        let (signal_tx, signals) = mpsc::unbounded_channel();
        let stop = CancellationToken::new();
        let cancel = stop.clone();
        let result = tokio::spawn(supervise(
            ControlledOutput {
                chunks: input,
                read: read_tx,
            },
            tokio::io::empty(),
            async { Ok(exited.await.unwrap()) },
            move |signal| {
                signal_tx.send(signal).unwrap();
            },
            output,
            cancel,
            timeout,
        ));
        chunks.send(b"pending".to_vec()).await.unwrap();
        // The channel is full and the supervisor has read its next chunk.
        read.recv().await.unwrap();
        assert_eq!(receiver.len(), 1);
        assert!(!result.is_finished());
        ControlledCommand {
            result,
            output: receiver,
            stop,
            exit,
            signals,
            chunks,
            read,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn output_backpressure_cannot_delay_timeout_escalation_or_reaping() {
        let mut run = backpressured(Some(Duration::from_millis(50))).await;
        tokio::time::advance(Duration::from_millis(49)).await;
        tokio::task::yield_now().await;
        assert!(run.signals.try_recv().is_err());
        tokio::time::advance(Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(run.signals.try_recv(), Ok(libc::SIGTERM));
        tokio::time::advance(Duration::from_millis(999)).await;
        tokio::task::yield_now().await;
        assert!(run.signals.try_recv().is_err());
        tokio::time::advance(Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(run.signals.try_recv(), Ok(libc::SIGKILL));
        assert!(!run.result.is_finished(), "must wait for reaping");
        run.exit.send(ExitStatus::from_raw(libc::SIGKILL)).unwrap();
        let (status, timed_out) = run.result.await.unwrap();
        assert!(timed_out);
        assert_eq!(status.unwrap().signal(), Some(libc::SIGKILL));
        assert_eq!(run.output.recv().await.as_deref(), Some("queued"));
        assert_eq!(
            run.output.recv().await,
            None,
            "undelivered output is discarded"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn output_backpressure_cannot_delay_cancellation() {
        let mut run = backpressured(None).await;
        run.stop.cancel();
        assert_eq!(run.signals.recv().await, Some(libc::SIGTERM));
        run.exit.send(ExitStatus::from_raw(libc::SIGTERM)).unwrap();
        drop(run.chunks);
        let (status, timed_out) = run.result.await.unwrap();
        assert!(!timed_out);
        assert_eq!(status.unwrap().signal(), Some(libc::SIGTERM));
        assert_eq!(run.output.recv().await.as_deref(), Some("queued"));
        assert_eq!(run.output.recv().await, None);
    }

    #[tokio::test(start_paused = true)]
    async fn exited_command_bounds_output_flush_without_marking_a_timeout() {
        let mut run = backpressured(Some(Duration::from_secs(10))).await;
        run.exit.send(ExitStatus::from_raw(0)).unwrap();
        assert_eq!(run.signals.recv().await, Some(libc::SIGKILL));
        tokio::time::advance(Duration::from_millis(999)).await;
        tokio::task::yield_now().await;
        assert!(!run.result.is_finished());
        tokio::time::advance(Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        assert!(run.result.is_finished());
        let (status, timed_out) = run.result.await.unwrap();
        assert!(!timed_out);
        assert_eq!(status.unwrap().code(), Some(0));
        assert_eq!(run.output.recv().await.as_deref(), Some("queued"));
        assert_eq!(run.output.recv().await, None);
    }

    #[tokio::test(start_paused = true)]
    async fn resumed_output_preserves_order_and_split_utf8_through_exit() {
        let mut run = backpressured(None).await;
        run.chunks.send(vec![0xe3]).await.unwrap();
        run.chunks.send(vec![0x81, 0x82]).await.unwrap();
        run.chunks.send(vec![0xe3]).await.unwrap();
        assert_eq!(run.output.recv().await.as_deref(), Some("queued"));
        assert_eq!(run.output.recv().await.as_deref(), Some("pending"));
        // Both pieces have actually been read before delivering child completion.
        run.read.recv().await.unwrap();
        run.read.recv().await.unwrap();
        run.exit.send(ExitStatus::from_raw(0)).unwrap();
        drop(run.chunks);
        let mut remaining = String::new();
        while let Some(chunk) = run.output.recv().await {
            remaining.push_str(&chunk);
        }
        assert_eq!(remaining, "あ�");
        let (status, timed_out) = run.result.await.unwrap();
        assert_eq!(status.unwrap().code(), Some(0));
        assert!(!timed_out);
    }

    async fn finish(mut run: Running) -> Outcome {
        // A watchdog only: this deadline never coordinates the child startup.
        match tokio::time::timeout(Duration::from_secs(10), &mut run.result).await {
            Ok(result) => result.unwrap(),
            Err(_) => {
                run.result.abort();
                let _ = run.result.await;
                panic!("command did not terminate");
            }
        }
    }

    #[tokio::test]
    async fn captures_both_streams() {
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
    }

    async fn full_output(run: &mut Running) {
        let ready = tokio::time::timeout(Duration::from_secs(10), async {
            while run.output.len() < run.output.max_capacity() {
                assert!(
                    !run.result.is_finished(),
                    "command exited before filling output"
                );
                tokio::task::yield_now().await;
            }
        })
        .await;
        if ready.is_err() {
            run.result.abort();
            let _ = (&mut run.result).await;
            panic!("command did not fill the output queue");
        }
    }

    #[tokio::test]
    async fn timeout_terminates_and_reaps_with_an_unconsumed_output_queue() {
        let mut run = Running::start(
            Options {
                argv: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "while :; do printf '%8192s' x; done".into(),
                ],
                cwd: None,
            },
            Some(Duration::from_secs(2)),
        );
        full_output(&mut run).await;
        let result = finish(run).await;
        assert!(result.timed_out);
        assert!(result.error.is_none());
        assert!(result.signal.is_some());
    }

    #[tokio::test]
    async fn cancellation_kills_a_ready_term_resistant_command() {
        let mut run = Running::start(
            Options {
                argv: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "trap '' TERM; printf 'READY\\n'; while :; do printf '%8192s' x; done".into(),
                ],
                cwd: None,
            },
            None,
        );
        let ready = tokio::time::timeout(Duration::from_secs(10), async {
            let mut output = String::new();
            while let Some(chunk) = run.output.recv().await {
                output.push_str(&chunk);
                if output.contains("READY\n") {
                    return true;
                }
            }
            false
        })
        .await;
        if !matches!(ready, Ok(true)) {
            run.result.abort();
            let _ = run.result.await;
            panic!("command did not acknowledge its SIGTERM handler");
        }
        full_output(&mut run).await;
        run.stop.cancel();
        let result = finish(run).await;
        assert!(!result.timed_out);
        assert!(result.error.is_none());
        assert_eq!(result.signal, Some(libc::SIGKILL));
    }
}
