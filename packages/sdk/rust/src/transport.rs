use crate::resources::{ResourceRegistry, is_resource};
use std::collections::BTreeSet;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines, Stdin, Stdout};

use crate::Result;

#[async_trait]
pub trait Transport: Send {
    fn set_resources(&mut self, _resources: ResourceRegistry) {}
    fn capabilities(&self) -> BTreeSet<String> {
        BTreeSet::new()
    }
    async fn send(&mut self, message: &Value) -> Result<()>;
    async fn receive(&mut self) -> Result<Option<Value>>;
}

pub struct StdioTransport {
    lines: Option<Lines<BufReader<Stdin>>>,
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
            lines: Some(BufReader::new(tokio::io::stdin()).lines()),
            inbound: None,
            reader: None,
            stdout: Arc::new(Mutex::new(tokio::io::stdout())),
            capabilities: capabilities.into_iter().map(Into::into).collect(),
        }
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
        let Some(mut lines) = self.lines.take() else {
            return;
        };
        let stdout = self.stdout.clone();
        let (sender, receiver) = mpsc::channel(256);
        self.inbound = Some(receiver);
        self.reader = Some(tokio::spawn(async move {
            loop {
                let message = match lines.next_line().await {
                    Ok(Some(line)) if line.trim().is_empty() => continue,
                    Ok(Some(line)) => serde_json::from_str::<Value>(&line)
                        .map(Some)
                        .map_err(crate::Error::from),
                    Ok(None) => break,
                    Err(error) => {
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
        loop {
            let Some(line) = self.lines.as_mut().unwrap().next_line().await? else {
                return Ok(None);
            };
            if !line.trim().is_empty() {
                return Ok(Some(serde_json::from_str(&line)?));
            }
        }
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
        } else {
            self.receiver.as_mut().unwrap()().await
        }
    }
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
