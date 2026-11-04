use crate::resources::{ResourceRegistry, is_resource};
use std::collections::BTreeSet;
use std::io::{BufRead, Read};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncWriteExt, Stdout};

use crate::Result;

// Count raw bytes before UTF-8 decoding, excluding only the terminating LF.
const MAX_STDIN_FRAME_BYTES: usize = 4 * 1024 * 1024;

fn read_stdin_line(reader: &mut impl BufRead) -> std::io::Result<Option<String>> {
    let mut line = Vec::new();
    let read = reader
        .take((MAX_STDIN_FRAME_BYTES + 1) as u64)
        .read_until(b'\n', &mut line)?;
    if read == 0 {
        return Ok(None);
    }
    if line.last() == Some(&b'\n') {
        line.pop();
    }
    if line.len() > MAX_STDIN_FRAME_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "stdin frame exceeds 4 MiB limit",
        ));
    }
    String::from_utf8(line)
        .map(Some)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[async_trait]
pub trait Transport: Send {
    fn set_resources(&mut self, _resources: ResourceRegistry) {}
    fn capabilities(&self) -> BTreeSet<String> {
        BTreeSet::new()
    }
    async fn send(&mut self, message: &Value) -> Result<()>;
    async fn receive(&mut self) -> Result<Option<Value>>;
    /// Stop and await receive-side work, including listeners and discovery publication.
    /// Called during runtime shutdown; implementations should be idempotent and may
    /// retain sending support for subsequent direct runtime calls.
    async fn stop_receiving(&mut self) {}
}

/// A JSON-lines transport that owns process stdin while receiving.
///
/// Stdin is read on a dedicated thread, rather than Tokio's blocking pool, so
/// an idle pipe cannot prevent the async runtime or process from exiting.
/// `stop_receiving` joins the async resource pump but cannot cancel a portable
/// blocking stdin read: that detached thread exits on its next read completion
/// or EOF. Do not reuse process stdin after stopping this transport; an in-flight
/// read can still consume input. The thread retains no resource or output state.
pub struct StdioTransport {
    lines: Option<mpsc::Receiver<std::io::Result<String>>>,
    receiving: bool,
    inbound: Option<mpsc::Receiver<Result<Option<Value>>>>,
    reader: Option<tokio::task::JoinHandle<()>>,
    stdout: Arc<Mutex<Stdout>>,
    capabilities: BTreeSet<String>,
}

impl StdioTransport {
    pub fn new() -> Self {
        Self::with_capabilities(Vec::<String>::new())
    }
    pub fn with_capabilities(capabilities: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            lines: None,
            receiving: true,
            inbound: None,
            reader: None,
            stdout: Arc::new(Mutex::new(tokio::io::stdout())),
            capabilities: capabilities.into_iter().map(Into::into).collect(),
        }
    }

    fn start_stdin(&mut self) {
        if self.lines.is_some() {
            return;
        }
        // One queued line bounds read-ahead between the blocking reader and the
        // resource pump. Closing the receiver also wakes a blocked channel send.
        let (sender, receiver) = mpsc::channel(1);
        let input = sender.clone();
        let reader = std::thread::Builder::new()
            .name("omochi-stdin".into())
            .spawn(move || {
                let stdin = std::io::stdin();
                let mut stdin = stdin.lock();
                while !input.is_closed() {
                    let result = match read_stdin_line(&mut stdin) {
                        Ok(None) => break,
                        Ok(Some(line)) => Ok(line),
                        Err(error) => Err(error),
                    };
                    let failed = result.is_err();
                    if input.blocking_send(result).is_err() || failed {
                        break;
                    }
                }
            });
        if let Err(error) = reader {
            let _ = sender.try_send(Err(error));
        }
        self.lines = Some(receiver);
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Transport for StdioTransport {
    fn set_resources(&mut self, resources: ResourceRegistry) {
        if self.reader.is_some() || !self.receiving {
            return;
        }
        self.start_stdin();
        let mut lines = self.lines.take().expect("stdin receiver was initialized");
        let stdout = self.stdout.clone();
        let (sender, receiver) = mpsc::channel(256);
        self.inbound = Some(receiver);
        self.reader = Some(tokio::spawn(async move {
            loop {
                let message = match lines.recv().await {
                    Some(Ok(line)) if line.trim().is_empty() => continue,
                    Some(Ok(line)) => serde_json::from_str::<Value>(&line)
                        .map(Some)
                        .map_err(crate::Error::from),
                    None => break,
                    Some(Err(error)) => {
                        let _ = sender.send(Err(error.into())).await;
                        break;
                    }
                };
                if let Ok(Some(value)) = &message {
                    if is_resource(value) {
                        if value["kind"] == "request" {
                            let response = format!("{}\n", resources.respond(value));
                            let mut output = stdout.lock().await;
                            if output.write_all(response.as_bytes()).await.is_err() {
                                break;
                            }
                            if output.flush().await.is_err() {
                                break;
                            }
                        }
                        continue;
                    }
                }
                if sender.send(message).await.is_err() {
                    break;
                }
            }
        }));
    }

    fn capabilities(&self) -> BTreeSet<String> {
        self.capabilities.clone()
    }
    async fn send(&mut self, message: &Value) -> Result<()> {
        let mut stdout = self.stdout.lock().await;
        stdout
            .write_all(serde_json::to_string(message)?.as_bytes())
            .await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
        Ok(())
    }
    async fn receive(&mut self) -> Result<Option<Value>> {
        if let Some(receiver) = &mut self.inbound {
            return receiver.recv().await.unwrap_or(Ok(None));
        }
        if !self.receiving {
            return Ok(None);
        }
        self.start_stdin();
        let lines = self.lines.as_mut().expect("stdin receiver was initialized");
        loop {
            let Some(line) = lines.recv().await else {
                return Ok(None);
            };
            let line = line?;
            if !line.trim().is_empty() {
                return Ok(Some(serde_json::from_str(&line)?));
            }
        }
    }
    async fn stop_receiving(&mut self) {
        self.receiving = false;
        self.inbound.take();
        self.lines.take();
        stop_reader(&mut self.reader).await;
    }
}

pub struct CallbackTransport<S, R> {
    sender: Arc<Mutex<S>>,
    receiver: Option<R>,
    inbound: Option<mpsc::Receiver<Result<Option<Value>>>>,
    reader: Option<tokio::task::JoinHandle<()>>,
    capabilities: BTreeSet<String>,
}

impl<S, R> CallbackTransport<S, R> {
    pub fn new(sender: S, receiver: R) -> Self {
        Self {
            sender: Arc::new(Mutex::new(sender)),
            receiver: Some(receiver),
            inbound: None,
            reader: None,
            capabilities: BTreeSet::new(),
        }
    }
    pub fn with_capabilities(
        mut self,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.capabilities = capabilities.into_iter().map(Into::into).collect();
        self
    }
}

#[async_trait]
impl<S, R, SF, RF> Transport for CallbackTransport<S, R>
where
    S: FnMut(Value) -> SF + Send + 'static,
    R: FnMut() -> RF + Send + 'static,
    SF: std::future::Future<Output = Result<()>> + Send + 'static,
    RF: std::future::Future<Output = Result<Option<Value>>> + Send + 'static,
{
    fn set_resources(&mut self, resources: ResourceRegistry) {
        let Some(mut receive) = self.receiver.take() else {
            return;
        };
        let send = self.sender.clone();
        let (sender, receiver) = mpsc::channel(256);
        self.inbound = Some(receiver);
        self.reader = Some(tokio::spawn(async move {
            loop {
                let message = receive().await;
                if let Ok(Some(value)) = &message {
                    if is_resource(value) {
                        if value["kind"] == "request" {
                            let mut output = send.lock().await;
                            if let Err(error) = output(resources.respond(value)).await {
                                let _ = sender.send(Err(error)).await;
                                break;
                            }
                        }
                        continue;
                    }
                }
                let done = !matches!(message, Ok(Some(_)));
                if sender.send(message).await.is_err() || done {
                    break;
                }
            }
        }));
    }
    fn capabilities(&self) -> BTreeSet<String> {
        self.capabilities.clone()
    }
    async fn send(&mut self, message: &Value) -> Result<()> {
        let mut send = self.sender.lock().await;
        send(message.clone()).await
    }
    async fn receive(&mut self) -> Result<Option<Value>> {
        if let Some(receiver) = &mut self.inbound {
            receiver.recv().await.unwrap_or(Ok(None))
        } else if let Some(receive) = &mut self.receiver {
            receive().await
        } else {
            Ok(None)
        }
    }
    async fn stop_receiving(&mut self) {
        self.inbound.take();
        self.receiver.take();
        stop_reader(&mut self.reader).await;
    }
}

async fn stop_reader(reader: &mut Option<tokio::task::JoinHandle<()>>) {
    if let Some(task) = reader.as_mut() {
        task.abort();
        // Retain the handle until joined so cancellation of shutdown can be resumed.
        let _ = task.await;
    }
    reader.take();
}
impl<S, R> Drop for CallbackTransport<S, R> {
    fn drop(&mut self) {
        if let Some(reader) = &self.reader {
            reader.abort();
        }
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        if let Some(reader) = &self.reader {
            reader.abort();
        }
    }
}
