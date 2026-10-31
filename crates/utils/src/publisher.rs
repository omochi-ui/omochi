//! One serialized publisher actor owns the SDK transport and validates whole batches.
use crate::events::EventBuffer;
use crate::local::LocalTransport;
use anyhow::{Context, Result, bail};
use omochi_ui::{A2uiSnapshot, UnixSocketServerOptions};
use serde_json::{Value, json};
use std::path::PathBuf;
use tokio::{sync::oneshot, task::JoinHandle};
use tokio_util::sync::CancellationToken;

enum Request {
    Publish(Vec<Value>, oneshot::Sender<Result<Value>>),
    List(oneshot::Sender<Value>),
}
#[derive(Clone)]
pub struct Publisher {
    commands: omochi_protocol::queue::Sender<Request>,
    pub events: EventBuffer,
    pub service_id: String,
    pub socket_path: PathBuf,
    stop: CancellationToken,
}
pub struct PublisherTask {
    task: JoinHandle<Result<()>>,
    stop: CancellationToken,
}
impl PublisherTask {
    pub async fn close(self) -> Result<()> {
        self.stop.cancel();
        self.task.await?
    }
}
impl Publisher {
    pub async fn open(options: UnixSocketServerOptions) -> Result<(Self, PublisherTask)> {
        let service_id = options.service_id.clone();
        let mut transport = LocalTransport::bind(options).await?;
        let socket_path = transport.socket_path().to_owned();
        let (commands, mut requests) = omochi_protocol::queue::channel(
            8,
            omochi_protocol::queue::DEFAULT_BYTES,
            omochi_protocol::MAX_FRAME_BYTES,
        );
        let stop = CancellationToken::new();
        let publisher = Self {
            commands,
            events: EventBuffer::default(),
            service_id: service_id.clone(),
            socket_path,
            stop: stop.clone(),
        };
        let events = publisher.events.clone();
        let cancellation = stop.clone();
        let task = tokio::spawn(async move {
            let mut snapshot = A2uiSnapshot::new();
            let result = async {
                loop {
                    tokio::select! {
                        _ = cancellation.cancelled() => break,
                        message = transport.receive() => match message? {
                            Some(message) => events.push(message),
                            None => break,
                        },
                        request = requests.recv() => {
                            let Some(request) = request else { break; };
                            let (request, _charge) = request.into_parts();
                            match request {
                            Request::List(reply) => { let _ = reply.send(json!({"serviceId":service_id,"surfaceIds":snapshot.surface_ids()})); },
                            Request::Publish(batch, reply) => {
                                let result = publish(&mut transport, &mut snapshot, &service_id, batch).await;
                                let _ = reply.send(result);
                            },
                            }
                        }
                    }
                }
                Ok(())
            }.await;
            events.close();
            transport.close().await;
            result
        });
        Ok((publisher, PublisherTask { task, stop }))
    }
    pub async fn publish(&self, batch: Vec<Value>) -> Result<Value> {
        if self.stop.is_cancelled() {
            bail!("publisher is closed");
        }
        // Bound queued data before entering the actor, not just while processing it.
        let bytes = serde_json::to_vec(&batch)?.len();
        if batch.is_empty() || bytes > 4 * 1024 * 1024 {
            bail!("messages must be a non-empty batch of at most 4 MiB");
        }
        let (reply, response) = oneshot::channel();
        self.commands
            .try_send(Request::Publish(batch, reply), bytes)
            .context("publisher is closed or busy")?;
        response.await.context("publisher is closed")?
    }
    pub async fn list(&self) -> Result<Value> {
        let (reply, response) = oneshot::channel();
        self.commands
            .try_send(Request::List(reply), 1)
            .context("publisher is closed or busy")?;
        response.await.context("publisher is closed")
    }
}
fn validate(message: &Value) -> Result<()> {
    if message["version"] != "v0.9" {
        bail!("expected A2UI v0.9");
    }
    let kinds = [
        "createSurface",
        "updateComponents",
        "updateDataModel",
        "deleteSurface",
    ];
    let kinds: Vec<_> = kinds
        .into_iter()
        .filter(|k| message.get(k).is_some())
        .collect();
    if kinds.len() != 1 {
        bail!("expected exactly one server message");
    }
    let kind = kinds[0];
    let body = &message[kind];
    if !body["surfaceId"].as_str().is_some_and(|s| !s.is_empty()) {
        bail!("surfaceId must be a non-empty string");
    }
    match kind {
        "createSurface" => {
            if !body["catalogId"].is_string() {
                bail!("catalogId must be a string");
            }
            if body.get("sendDataModel").is_some_and(|v| !v.is_boolean()) {
                bail!("sendDataModel must be boolean");
            }
        }
        "updateComponents" => {
            let components = body["components"]
                .as_array()
                .context("components must be an array")?;
            for component in components {
                if !component["id"].is_string() || !component["component"].is_string() {
                    bail!("component needs string id and component");
                }
            }
        }
        "updateDataModel" => omochi_protocol::data_path::validate(message)?,
        _ => {}
    }
    Ok(())
}
async fn publish(
    transport: &mut LocalTransport,
    snapshot: &mut A2uiSnapshot,
    service: &str,
    batch: Vec<Value>,
) -> Result<Value> {
    let mut candidate = A2uiSnapshot::new();
    for message in snapshot.messages() {
        candidate.apply(&message)?;
    }
    for (index, message) in batch.iter().enumerate() {
        validate(message)
            .and_then(|_| candidate.apply(message).map_err(Into::into))
            .with_context(|| format!("messages[{index}]"))?;
        if serde_json::to_vec(&candidate.messages())?.len() > 16 * 1024 * 1024 {
            bail!("snapshot exceeds 16 MiB");
        }
    }
    if candidate
        .messages()
        .iter()
        .any(|message| message.to_string().len() > omochi_protocol::MAX_FRAME_BYTES)
    {
        bail!("snapshot message exceeds 4 MiB");
    }
    for message in &batch {
        transport.send(message).await?;
    }
    *snapshot = candidate;
    Ok(json!({"accepted":batch.len(),"serviceId":service,"surfaceIds":snapshot.surface_ids()}))
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    #[tokio::test]
    async fn batches_are_atomic_and_late_clients_replay_current_state_and_actions() {
        let root = tempfile::tempdir().unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let options = UnixSocketServerOptions::new("test").with_runtime_dir(root.path());
        let (publisher, task) = Publisher::open(options.clone()).await.unwrap();
        assert!(Publisher::open(options).await.is_err());
        let create =
            json!({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"}});
        let missing =
            json!({"version":"v0.9","updateDataModel":{"surfaceId":"missing","value":{}}});
        assert!(
            publisher
                .publish(vec![create.clone(), missing])
                .await
                .is_err()
        );
        assert_eq!(publisher.list().await.unwrap()["surfaceIds"], json!([]));
        publisher.publish(vec![create, json!({"version":"v0.9","updateDataModel":{"surfaceId":"main","value":{"count":1}}})]).await.unwrap();
        publisher.publish(vec![json!({"version":"v0.9","updateDataModel":{"surfaceId":"main","path":"/count","value":2}})]).await.unwrap();
        let socket = tokio::net::UnixStream::connect(&publisher.socket_path)
            .await
            .unwrap();
        let (read, mut write) = socket.into_split();
        let mut lines = BufReader::new(read).lines();
        let replay = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            let first = lines.next_line().await.unwrap().unwrap();
            let second = lines.next_line().await.unwrap().unwrap();
            (first, second)
        })
        .await
        .unwrap();
        assert!(replay.0.contains("createSurface"));
        assert!(replay.1.contains("\"count\":2"));
        write.write_all(b"{\"version\":\"v0.9\",\"action\":{\"surfaceId\":\"main\",\"name\":\"submit\",\"context\":{}}}\n").await.unwrap();
        assert_eq!(
            publisher
                .events
                .read(0, 1000, &CancellationToken::new())
                .await
                .unwrap()["events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let path = publisher.socket_path.clone();
        task.close().await.unwrap();
        assert!(!path.exists());
        assert!(publisher.publish(vec![json!({})]).await.is_err());
    }
}
