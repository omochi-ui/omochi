//! Single-application bridge. Surface IDs and client draft metadata stay unchanged.
use crate::{
    OutboundAction, Subscription, apply_snapshot,
    scoped::{HubEvent, Scope, surface_id},
};
use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use omochi_protocol::MAX_FRAME_BYTES;
use omochi_ui::A2uiSnapshot;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Mutex, oneshot},
    task::JoinHandle,
};
use tokio_util::{
    codec::{FramedRead, FramedWrite, LinesCodec},
    sync::CancellationToken,
};
enum Command {
    Action(OutboundAction),
    Resource(Value, oneshot::Sender<Value>),
}
pub(crate) struct State {
    pub snapshot: Arc<A2uiSnapshot>,
    pub scopes: BTreeMap<String, Scope>,
    pub events: omochi_protocol::fanout::Sender<HubEvent>,
    pub scoped: Option<CancellationToken>,
}
#[derive(Clone)]
pub struct Backend {
    pub(crate) state: Arc<Mutex<State>>,
    commands: omochi_protocol::queue::Sender<Command>,
    resources: Arc<AtomicBool>,
    next: Arc<AtomicU64>,
}
impl Backend {
    pub fn attach(
        input: impl AsyncRead + Unpin + Send + 'static,
        output: impl AsyncWrite + Unpin + Send + 'static,
        stop: CancellationToken,
    ) -> (Self, JoinHandle<Result<()>>) {
        let (commands, mut receive) = omochi_protocol::queue::channel(
            8,
            omochi_protocol::queue::DEFAULT_BYTES,
            MAX_FRAME_BYTES,
        );
        let (events, _) =
            omochi_protocol::fanout::channel(32, omochi_protocol::fanout::DEFAULT_BYTES);
        let backend = Self {
            state: Arc::new(Mutex::new(State {
                snapshot: Arc::new(A2uiSnapshot::new()),
                scopes: BTreeMap::new(),
                events,
                scoped: None,
            })),
            commands,
            resources: Arc::new(AtomicBool::new(false)),
            next: Arc::new(AtomicU64::new(0)),
        };
        let shared = backend.clone();
        let task = tokio::spawn(async move {
            let lifetime = stop.child_token();
            let _lifetime = lifetime.clone().drop_guard();
            let mut input =
                FramedRead::new(input, LinesCodec::new_with_max_length(MAX_FRAME_BYTES));
            let mut output =
                FramedWrite::new(output, LinesCodec::new_with_max_length(MAX_FRAME_BYTES));
            let mut pending: BTreeMap<String, oneshot::Sender<Value>> = BTreeMap::new();
            loop {
                tokio::select! {
                    _ = stop.cancelled()=>break,
                    line = input.next()=>{
                        let Some(line) = line else { break; }; let line = line?;
                        if line.trim().is_empty() { continue; }
                        let value:Value = serde_json::from_str(&line).context("invalid child A2UI JSON")?;
                        if value["omochi"] == "resources/1" {
                            if value["kind"] == "hello" { shared.resources.store(true,Ordering::Release); }
                            if value["kind"] == "response" && let Some(id) = value["id"].as_str() && let Some(reply) = pending.remove(id) { let _ = reply.send(value); }
                            continue;
                        }
                        if value["version"] != "v0.9" { bail!("invalid child A2UI version"); }
                        let mut state = shared.state.lock().await;
                        apply_snapshot(Arc::make_mut(&mut state.snapshot),&value)?;
                        let surface = surface_id(&value).context("missing Surface")?.to_owned();
                        let scope = if value.get("deleteSurface").is_some() {
                            if let Some(old) = state.scopes.remove(&surface) { old.stop.cancel(); }
                            None
                        } else if value.get("createSurface").is_some() {
                            let scope = Scope { route: "_".into(), surface: surface.clone(), instance: 0,
                                generation: shared.next.fetch_add(1, Ordering::Relaxed), stop: lifetime.child_token() };
                            if let Some(old) = state.scopes.insert(surface, scope.clone()) { old.stop.cancel(); }
                            Some(scope)
                        } else { state.scopes.get(&surface).cloned() };
                        let _ = state.events.send(Arc::new(HubEvent { message: Arc::new(value), scope }));
                    },
                    command = receive.recv()=>{
                        let Some(command) = command else { break; };
                        let (command, _charge) = command.into_parts();
                        pending.retain(|_,sender| !sender.is_closed());
                        let mut scope = None;
                        let message = match command {
                            Command::Action(action)=>{
                                if let Some((surface, subscription)) = &action.scope
                                    && (surface.is_cancelled() || subscription.is_cancelled()) { continue; }
                                scope = action.scope;
                                action.message
                            },
                            Command::Resource(value,reply)=>{
                                if !shared.resources.load(Ordering::Acquire) {
                                    let _ = reply.send(json!({"omochi":"resources/1","kind":"response","id":value["id"],"status":501}));
                                    continue;
                                }
                                if pending.len() >= 8 { continue; }
                                pending.insert(value["id"].as_str().unwrap().into(),reply); value
                            },
                        };
                        let write = tokio::time::timeout(Duration::from_secs(5),output.send(message.to_string()));
                        if let Some((surface, subscription)) = scope {
                            tokio::select! {
                                _ = surface.cancelled() => bail!("Surface expired during action write"),
                                _ = subscription.cancelled() => bail!("subscription ended during action write"),
                                result = write => result??,
                            }
                        } else { write.await??; }
                    }
                }
            }
            Ok(())
        });
        (backend, task)
    }
    pub async fn subscribe(&self) -> Subscription {
        let state = self.state.lock().await;
        Subscription {
            snapshot: state.snapshot.messages(),
            events: crate::scoped::EventReceiver::scoped(state.events.subscribe()),
        }
    }
    pub fn action(&self, message: Value) -> Result<()> {
        self.enqueue_action(OutboundAction {
            message,
            scope: None,
        })
    }
    pub(crate) fn enqueue_action(&self, action: OutboundAction) -> Result<()> {
        let message = &action.message;
        if message["version"] != "v0.9"
            || !["action", "error"]
                .iter()
                .any(|kind| message[*kind]["surfaceId"].is_string())
        {
            bail!("invalid client message");
        }
        let bytes = serde_json::to_vec(&message)?.len();
        self.commands
            .try_send(Command::Action(action), bytes)
            .map_err(|_| anyhow::anyhow!("child unavailable or busy"))
    }
    pub async fn resource(&self, mut request: Value) -> Result<Value> {
        request["omochi"] = "resources/1".into();
        request["kind"] = "request".into();
        request["id"] = format!("dev{}", self.next.fetch_add(1, Ordering::Relaxed)).into();
        let bytes = serde_json::to_vec(&request)?.len();
        let (reply, response) = oneshot::channel();
        self.commands
            .try_send(Command::Resource(request, reply), bytes)
            .map_err(|_| anyhow::anyhow!("child unavailable or busy"))?;
        Ok(tokio::time::timeout(Duration::from_secs(15), response).await??)
    }
}
