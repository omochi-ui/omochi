//! Adversarial protocol peer for browser framing tests only. Production admission and
//! revocation tests always use Gateway::listen_remote, without this peer.
use anyhow::{Context, Result, bail};
use iroh::Endpoint;
use omochi_gateway::Gateway;
use omochi_protocol::{ALPN, PAIR_ALPN, read_frame, write_frame};
use omochi_transport::store::{Role, Store};
use serde_json::{Value, json};
use std::{
    io::Cursor,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    sync::watch,
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct Faults {
    state: Arc<Mutex<State>>,
    release: watch::Sender<u64>,
    stop: CancellationToken,
}
#[derive(Default)]
struct State {
    next: Option<String>,
    maximum: bool,
}
impl Faults {
    pub fn new() -> Self {
        Self {
            state: Arc::default(),
            release: watch::channel(0).0,
            stop: CancellationToken::new(),
        }
    }
    pub fn set(&self, kind: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        match kind {
            "generation-max" => state.maximum = true,
            "generation-normal" => state.maximum = false,
            "partial-header"
            | "partial-body"
            | "silent-snapshot"
            | "interrupted-snapshot"
            | "stalled-snapshot"
            | "duplicate-roster"
            | "unknown-control"
            | "pong-mismatch"
            | "generation-replace"
            | "delete-recreate" => state.next = Some(kind.into()),
            "release" => {
                self.release.send_modify(|epoch| *epoch += 1);
            }
            _ => bail!("unsupported protocol fault"),
        }
        Ok(())
    }
    pub fn stop(&self) {
        self.stop.cancel();
    }
    async fn hold(&self) {
        let mut release = self.release.subscribe();
        let _ = super::output(json!({"kind":"fault-held"}));
        tokio::select! { _ = release.changed() => {}, _ = tokio::time::sleep(Duration::from_secs(120)) => {}, _ = self.stop.cancelled() => {} }
    }
    fn take(&self, open: &Value) -> Option<String> {
        let mut state = self.state.lock().unwrap();
        let next = state.next.as_deref()?;
        let control = matches!(
            next,
            "duplicate-roster" | "unknown-control" | "pong-mismatch" | "generation-replace"
        );
        if control && open["kind"] == "control"
            || !control && open["kind"] == "service" && open["service"] == "s"
        {
            state.next.take()
        } else {
            None
        }
    }
}
pub fn listen(
    endpoint: Endpoint,
    store: Store,
    gateway: Gateway,
    name: String,
    faults: Faults,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                _ = faults.stop.cancelled() => break,
                Some(_) = connections.join_next(), if !connections.is_empty() => {},
                incoming = endpoint.accept() => {
                    let Some(incoming) = incoming else { break };
                    if connections.len() >= 16 { incoming.refuse(); continue }
                    let store = store.clone(); let gateway = gateway.clone(); let faults = faults.clone(); let name = name.clone(); let node = endpoint.id().to_string();
                    connections.spawn(async move {
                        let connection = tokio::time::timeout(Duration::from_secs(10), incoming).await??;
                        if connection.alpn() == PAIR_ALPN { return omochi_transport::pairing::accept(connection, store).await }
                        if connection.alpn() != ALPN || !store.peers()?.1.get(&connection.remote_id().to_string()).is_some_and(|peer| peer.role == Role::Hub) {
                            connection.close(1u32.into(), b"not authorized"); bail!("unregistered protocol test peer")
                        }
                        let mut streams = JoinSet::new();
                        loop {
                            tokio::select! {
                                _ = faults.stop.cancelled() => break,
                                _ = connection.closed() => break,
                                Some(_) = streams.join_next(), if !streams.is_empty() => {},
                                stream = connection.accept_bi() => {
                                    let Ok((mut writer, mut reader)) = stream else { break };
                                    if streams.len() >= 32 { let _ = writer.reset(2u32.into()); let _ = reader.stop(2u32.into()); continue }
                                    let gateway = gateway.clone(); let faults = faults.clone(); let node = node.clone(); let name = name.clone();
                                    streams.spawn(async move {
                                        let mut open: Value = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut reader)).await??.context("missing test open")?;
                                        let mode = faults.take(&open);
                                        if let Some(mode) = &mode { super::output(json!({"kind":"fault-applied","fault":mode}))?; }
                                        if faults.state.lock().unwrap().maximum && open["service"] == "s" {
                                            if open["generation"].as_u64() != Some(u64::MAX) { bail!("maximum generation was rounded") }
                                            open["generation"] = json!(gateway.list().await.into_iter().find(|service| service.id == "s").context("missing test service")?.generation);
                                            super::output(json!({"kind":"maximum-generation-open","resource":open["kind"] == "resource"}))?;
                                        }
                                        let body = serde_json::to_vec(&open)?;
                                        let mut prefix = (body.len() as u32).to_be_bytes().to_vec(); prefix.extend(body);
                                        let reader = Cursor::new(prefix).chain(reader);
                                        let (backend_writer, mut output) = tokio::io::duplex(4 * 1024 * 1024 + 8);
                                        let backend = gateway.serve_stream(reader, backend_writer, &node, &name);
                                        let forward = async {
                                            let mut frames = 0;
                                            let mut replay = Vec::new();
                                            while let Some(mut frame) = read_frame::<_, Value>(&mut output).await? {
                                                frames += 1;
                                                let kind = frame["kind"].as_str().unwrap_or("").to_owned();
                                                if faults.state.lock().unwrap().maximum && kind == "services" {
                                                    if let Some(services) = frame["services"].as_array_mut() { for service in services { if service["id"] == "s" { service["generation"] = json!(u64::MAX); } } }
                                                }
                                                if mode.as_deref() == Some("delete-recreate") && kind == "message" {
                                                    if replay.len() >= 32 { bail!("fixture replay capacity") }
                                                    replay.push(frame.clone());
                                                }
                                                match mode.as_deref() {
                                                    Some("silent-snapshot") if frames == 1 => faults.hold().await,
                                                    Some("stalled-snapshot") if frames == 2 => faults.hold().await,
                                                    Some("interrupted-snapshot") if frames == 3 => { let _ = writer.reset(2u32.into()); bail!("interrupted fixture snapshot") },
                                                    Some("partial-header" | "partial-body") if frames == 1 => {
                                                        let body = serde_json::to_vec(&frame)?;
                                                        let mut bytes = (body.len() as u32).to_be_bytes().to_vec(); bytes.extend(body);
                                                        let cut = if mode.as_deref() == Some("partial-header") { 1 } else { 6 };
                                                        writer.write_all(&bytes[..cut]).await?;
                                                        faults.hold().await;
                                                        writer.write_all(&bytes[cut..]).await?;
                                                        continue;
                                                    }
                                                    Some("duplicate-roster") if kind == "services" => {
                                                        let services = frame["services"].as_array_mut().context("test roster")?;
                                                        services.push(services.first().context("empty test roster")?.clone());
                                                    }
                                                    Some("unknown-control") if kind == "hello" => { frame["unexpected"] = json!(true); },
                                                    Some("pong-mismatch") if kind == "pong" => { frame["id"] = json!(frame["id"].as_u64().context("test pong ID")? + 1); },
                                                    _ => {},
                                                }
                                                write_frame(&mut writer, &frame).await?;
                                                if mode.as_deref() == Some("generation-replace") && kind == "services" && frames == 2 {
                                                    faults.hold().await;
                                                    faults.state.lock().unwrap().maximum = true;
                                                    if let Some(services) = frame["services"].as_array_mut() { for service in services { if service["id"] == "s" { service["generation"] = json!(u64::MAX); } } }
                                                    write_frame(&mut writer, &frame).await?;
                                                }
                                                if mode.as_deref() == Some("delete-recreate") && kind == "snapshotEnd" {
                                                    write_frame(&mut writer, &json!({"kind":"message","message":{"version":"v0.9","deleteSurface":{"surfaceId":"main/nested"}}})).await?;
                                                    faults.hold().await;
                                                    for message in &replay { write_frame(&mut writer, message).await?; }
                                                    super::output(json!({"kind":"surface-recreated"}))?;
                                                }
                                            }
                                            writer.finish()?;
                                            Ok::<_, anyhow::Error>(())
                                        };
                                        tokio::try_join!(backend, forward)?;
                                        Ok::<_, anyhow::Error>(())
                                    });
                                }
                            }
                        }
                        connection.close(0u32.into(), b"protocol test ended"); streams.abort_all(); while streams.join_next().await.is_some() {}
                        Ok::<_, anyhow::Error>(())
                    });
                }
            }
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        endpoint.close().await;
    })
}
