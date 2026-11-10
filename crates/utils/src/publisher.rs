//! One serialized publisher actor owns the SDK transport and validates whole batches.
use crate::events::EventBuffer;
use crate::local::LocalTransport;
use anyhow::{Context, Result, bail};
use omochi_protocol::session::validate_service_message_frame;
use omochi_ui::{A2uiSnapshot, LocalServerOptions};
use serde_json::{Value, json};

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
    pub endpoint: omochi_local::LocalEndpoint,
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
    pub async fn open(options: LocalServerOptions) -> Result<(Self, PublisherTask)> {
        let service_id = options.service_id.clone();
        let mut transport = LocalTransport::bind(options).await?;
        let endpoint = transport.endpoint().clone();
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
            endpoint,
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
            .and_then(|_| validate_service_message_frame(message).map_err(Into::into))
            .and_then(|_| candidate.apply(message).map_err(Into::into))
            .with_context(|| format!("messages[{index}]"))?;
        let replay = candidate.messages();
        for message in &replay {
            validate_service_message_frame(message)
                .with_context(|| format!("messages[{index}] snapshot"))?;
        }
        if serde_json::to_vec(&replay)?.len() > 16 * 1024 * 1024 {
            bail!("snapshot exceeds 16 MiB");
        }
    }
    for message in &batch {
        transport.send(message).await?;
    }
    *snapshot = candidate;
    Ok(json!({"accepted":batch.len(),"serviceId":service,"surfaceIds":snapshot.surface_ids()}))
}
#[cfg(all(test, unix))]
mod tests {
    fn socket_path(endpoint: &omochi_local::LocalEndpoint) -> &std::path::Path {
        match endpoint {
            omochi_local::LocalEndpoint::Unix { path } => path,
            _ => unreachable!(),
        }
    }
    use super::*;
    use omochi_protocol::{MAX_FRAME_BYTES, session::ServiceFrame};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    fn data(value: Value) -> Value {
        json!({"version":"v0.9","updateDataModel":{"surfaceId":"main","path":"/","value":value}})
    }

    fn patch(key: &str, value: &str) -> Value {
        json!({"version":"v0.9","updateDataModel":{"surfaceId":"main","path":format!("/{key}"),"value":value}})
    }

    fn framed_size(message: &Value) -> usize {
        serde_json::to_vec(&ServiceFrame::Message {
            message: message.clone(),
        })
        .unwrap()
        .len()
    }

    fn value_for_frame_size(size: usize, split: bool) -> Value {
        let mut value = json!({"a":"","b":""});
        let payload = size - framed_size(&data(value.clone()));
        let first = if split { payload / 2 } else { payload };
        value["a"] = Value::String("x".repeat(first));
        value["b"] = Value::String("y".repeat(payload - first));
        assert_eq!(framed_size(&data(value.clone())), size);
        value
    }

    async fn seeded(value: Value) -> (tempfile::TempDir, Publisher, PublisherTask) {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let (publisher, task) =
            Publisher::open(LocalServerOptions::new("test").with_runtime_dir(root.path()))
                .await
                .unwrap();
        publisher
            .publish(vec![
                json!({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"}}),
                data(value),
            ])
            .await
            .unwrap();
        (root, publisher, task)
    }

    async fn replay(
        publisher: &Publisher,
    ) -> (
        Vec<Value>,
        tokio::io::Lines<BufReader<tokio::net::UnixStream>>,
    ) {
        let socket = tokio::net::UnixStream::connect(socket_path(&publisher.endpoint))
            .await
            .unwrap();
        let mut lines = BufReader::new(socket).lines();
        let mut messages = Vec::new();
        for _ in 0..2 {
            let line = tokio::time::timeout(std::time::Duration::from_secs(5), lines.next_line())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            messages.push(serde_json::from_str(&line).unwrap());
        }
        (messages, lines)
    }

    async fn assert_rejected_unchanged(publisher: &Publisher, batch: Vec<Value>) {
        let (before, mut existing) = replay(publisher).await;
        let error = publisher.publish(batch).await.unwrap_err();
        assert!(format!("{error:#}").contains("framed transport limit"));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), existing.next_line())
                .await
                .is_err(),
            "a rejected batch must send no messages to existing clients"
        );
        let (after, _) = replay(publisher).await;
        assert_eq!(after, before, "rejected batches must preserve late replay");
    }

    #[tokio::test]
    async fn accumulated_patches_reserve_the_gateway_frame_envelope() {
        // The raw replay fits 4 MiB, but its gateway wrapper does not.
        let value = value_for_frame_size(MAX_FRAME_BYTES + 19, true);
        let (_root, publisher, task) = seeded(json!({"a":value["a"],"b":""})).await;
        let update = patch("b", value["b"].as_str().unwrap());
        validate_service_message_frame(&update).unwrap();
        assert_rejected_unchanged(&publisher, vec![update]).await;
        task.close().await.unwrap();
    }

    #[tokio::test]
    async fn oversized_intermediate_replay_rejects_a_batch_even_if_it_finishes_small() {
        let value = value_for_frame_size(MAX_FRAME_BYTES - 128, false);
        let (_root, publisher, task) = seeded(value).await;
        let grow = patch("b", &"y".repeat(256));
        let shrink = patch("a", "");
        validate_service_message_frame(&grow).unwrap();
        validate_service_message_frame(&shrink).unwrap();
        assert_rejected_unchanged(&publisher, vec![patch("b", "staged"), grow, shrink]).await;
        task.close().await.unwrap();
    }

    #[tokio::test]
    async fn oversized_outgoing_message_rejects_the_entire_batch() {
        let (_root, publisher, task) = seeded(json!({"a":"original"})).await;
        // Unknown fields are omitted from the materialized data replay, but still
        // travel in the outgoing SDK message and need the same frame budget.
        let mut oversized = patch("a", "small");
        oversized["extra"] = json!("");
        let padding = MAX_FRAME_BYTES + 1 - framed_size(&oversized);
        oversized["extra"] = json!("x".repeat(padding));
        assert!(serde_json::to_vec(&vec![oversized.clone()]).unwrap().len() <= MAX_FRAME_BYTES);
        assert_rejected_unchanged(&publisher, vec![oversized]).await;
        task.close().await.unwrap();
    }

    #[tokio::test]
    async fn accepts_accumulated_replay_at_the_exact_framed_limit() {
        let value = value_for_frame_size(MAX_FRAME_BYTES, true);
        let (_root, publisher, task) = seeded(json!({"a":value["a"],"b":""})).await;
        publisher
            .publish(vec![patch("b", value["b"].as_str().unwrap())])
            .await
            .unwrap();
        let (messages, _) = replay(&publisher).await;
        assert_eq!(messages[1], data(value));
        assert_eq!(framed_size(&messages[1]), MAX_FRAME_BYTES);
        for message in messages {
            validate_service_message_frame(&message).unwrap();
        }
        task.close().await.unwrap();
    }

    #[tokio::test]
    async fn batches_are_atomic_and_late_clients_replay_current_state_and_actions() {
        let root = tempfile::tempdir().unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let options = LocalServerOptions::new("test").with_runtime_dir(root.path());
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
        let socket = tokio::net::UnixStream::connect(socket_path(&publisher.endpoint))
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
        let path = socket_path(&publisher.endpoint).to_owned();
        task.close().await.unwrap();
        assert!(!path.exists());
        assert!(publisher.publish(vec![json!({})]).await.is_err());
    }
}
