//! Opt-in browser image transport. Only the WS adapter can issue a fetch reference.
//! References are one-use, short-lived and bound to the exact rendering subscription.
use crate::{Hub, capacity::CapacityError, dashboards::ApiError, scoped::SurfaceHandle};
use axum::{
    Extension,
    body::Body,
    extract::{
        Request,
        ws::{Message, WebSocket},
    },
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

const TTL: Duration = Duration::from_secs(60);
const MAX_REFERENCES: usize = 512;
const MAX_OWNER_REFERENCES: usize = 64;
const MAX_REFERENCE_BYTES: usize = 4 * 1024 * 1024;
const MAX_HANDLES: usize = 4096;
const MAX_HANDLE_BYTES: usize = 8 * 1024 * 1024;

struct Reference {
    owner: String,
    request: u64,
    handle: SurfaceHandle,
    uri: String,
    expires: Instant,
    used: bool,
    stop: CancellationToken,
}
impl Drop for Reference {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
#[derive(Clone)]
pub(crate) struct References {
    entries: Arc<Mutex<BTreeMap<String, Reference>>>,
    // Keep the bound until HTTP body disposal, not just until the network read ends.
    deliveries: Arc<Semaphore>,
}
impl Default for References {
    fn default() -> Self {
        Self {
            entries: Arc::default(),
            deliveries: Arc::new(Semaphore::new(2)),
        }
    }
}
impl References {
    async fn issue(
        &self,
        owner: &str,
        request: u64,
        handle: &SurfaceHandle,
        uri: &str,
    ) -> Result<String, ApiError> {
        if uri.len() > 16 * 1024 {
            return Err(ApiError::new(400, "Invalid image URI"));
        }
        handle
            .validate_current()
            .await
            .map_err(|_| ApiError::new(410, "Image scope expired"))?;
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|_, r| r.expires > Instant::now() && !r.handle.is_cancelled());
        let owner_entries: Vec<_> = entries.values().filter(|r| r.owner == owner).collect();
        if owner_entries.iter().any(|r| r.request == request) {
            return Err(ApiError::new(400, "Duplicate image request"));
        }
        if owner_entries.len() >= MAX_OWNER_REFERENCES
            || entries.len() >= MAX_REFERENCES
            || entries.values().map(|r| r.uri.len()).sum::<usize>() + uri.len()
                > MAX_REFERENCE_BYTES
        {
            return Err(ApiError::new(429, "Image reference limit reached"));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        entries.insert(
            id.clone(),
            Reference {
                owner: owner.into(),
                request,
                handle: handle.clone(),
                uri: uri.into(),
                expires: Instant::now() + TTL,
                used: false,
                stop: CancellationToken::new(),
            },
        );
        Ok(id)
    }
    fn release(&self, owner: &str, request: u64) {
        self.entries
            .lock()
            .unwrap()
            .retain(|_, r| r.owner != owner || r.request != request);
    }
    fn remove_owner(&self, owner: &str) {
        self.entries.lock().unwrap().retain(|_, r| r.owner != owner);
    }
}
struct Owner {
    id: String,
    references: References,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.references.remove_owner(&self.id);
    }
}

pub(crate) async fn image(
    Extension(references): Extension<References>,
    request: Request,
) -> Response {
    match fetch(references, request).await {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}
async fn fetch(references: References, request: Request) -> Result<Response, ApiError> {
    if request.method() != Method::GET {
        return Err(ApiError::new(405, "Use GET"));
    }
    let id = request.uri().path().strip_prefix("/images/").unwrap_or("");
    if request.uri().query().is_some()
        || id.len() != 32
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ApiError::new(404, "Image reference not found"));
    }
    let permit = references
        .deliveries
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::new(429, "Image delivery busy"))?;
    let (handle, uri, stop) = {
        let mut entries = references.entries.lock().unwrap();
        let r = entries
            .get_mut(id)
            .filter(|r| !r.used && r.expires > Instant::now() && !r.handle.is_cancelled())
            .ok_or_else(|| ApiError::new(410, "Image reference expired"))?;
        r.used = true;
        (r.handle.clone(), r.uri.clone(), r.stop.clone())
    };
    // Dropping the HTTP handler/body cancels any outstanding core fetch.
    let guard = stop.clone().drop_guard();
    let image = handle.image(&uri, stop.clone()).await.map_err(|error| {
        use crate::images::ImageError::*;
        let status = match error {
            Busy => 429,
            Cancelled | Unavailable => 410,
            Timeout => 504,
            Network => 502,
            _ => 422,
        };
        ApiError::new(status, &error.to_string())
    })?;
    let mime = image.mime;
    let (width, height) = (image.width, image.height);
    let length = image.bytes.len();
    let body = async_stream::stream! {
        let (_guard, _permit) = (guard, permit);
        for chunk in image.bytes.chunks(64 * 1024) {
            if stop.is_cancelled() || handle.is_cancelled() {
                yield Err(std::io::Error::other("image scope expired"));
                break;
            }
            yield Ok::<_, std::io::Error>(axum::body::Bytes::copy_from_slice(chunk));
        }
    };
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, length)
        .header("X-Omochi-Image-Width", width)
        .header("X-Omochi-Image-Height", height)
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "X-Omochi-Image-Width, X-Omochi-Image-Height",
        )
        .header(header::CACHE_CONTROL, "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from_stream(body))
        .unwrap())
}

async fn send(
    writer: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    value: Value,
) -> anyhow::Result<()> {
    tokio::time::timeout(
        Duration::from_secs(5),
        writer.send(Message::Text(value.to_string().into())),
    )
    .await??;
    Ok(())
}
pub(crate) async fn browser(socket: WebSocket, hub: Hub, references: References) {
    let Ok(mut subscription) = hub.subscribe_browser().await else {
        return;
    };
    let cancelled = subscription.cancellation_token();
    let owner = Owner {
        id: uuid::Uuid::new_v4().to_string(),
        references,
    };
    let (mut writer, mut reader) = socket.split();
    if send(&mut writer, json!({"omochi":"images/1","kind":"hello"}))
        .await
        .is_err()
    {
        return;
    }
    let mut handles: BTreeMap<String, (String, SurfaceHandle)> = BTreeMap::new();
    let mut waiting = None;
    let mut deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let code = loop {
        tokio::select! {
            biased;
            _ = hub.stop.cancelled() => break hub.close_code.load(std::sync::atomic::Ordering::Acquire),
            _ = cancelled.cancelled() => break 1013,
            _ = tokio::time::sleep_until(deadline), if waiting.is_some() => break 1013,
            received = reader.next() => {
                let Some(Ok(message)) = received else { break 1000 };
                let Message::Text(text) = message else {
                    if matches!(message, Message::Ping(_) | Message::Pong(_)) { continue }
                    break 1000;
                };
                let Ok(value) = serde_json::from_str::<Value>(&text) else { break 1002 };
                if value["omochi"] != "images/1" {
                    if hub.action(value).await.is_err() { break 1002 }
                    continue;
                }
                match value["kind"].as_str() {
                    Some("ack") => {
                        let Some(sequence) = value["sequence"].as_u64() else { break 1002 };
                        if waiting != Some(sequence) || subscription.acknowledge(sequence).is_err() { break 1002 }
                        waiting = None;
                    }
                    Some("release") => {
                        let Some(id) = value["id"].as_u64() else { break 1002 };
                        owner.references.release(&owner.id, id);
                    }
                    Some("image") => {
                        let (Some(id), Some(token), Some(uri)) = (value["id"].as_u64(), value["handle"].as_str(), value["uri"].as_str()) else { break 1002 };
                        let handle = handles.values().find(|(id, _)| id == token).map(|(_, h)| h);
                        let result = match handle {
                            Some(handle) => owner.references.issue(&owner.id, id, handle, uri).await,
                            None => Err(ApiError::new(410, "Image scope expired")),
                        };
                        let response = match result {
                            Ok(reference) => json!({"omochi":"images/1","kind":"image","id":id,"reference":reference}),
                            Err(error) => json!({"omochi":"images/1","kind":"image","id":id,"error":error.1}),
                        };
                        if send(&mut writer, response).await.is_err() { break 1001 }
                    }
                    _ => break 1002,
                }
            }
            result = subscription.next_batch(), if waiting.is_none() => {
                let batch = match result {
                    Ok(batch) => batch,
                    Err(error) if error.downcast_ref::<CapacityError>() == Some(&CapacityError::MessageLimit) => break 1009,
                    Err(_) => break 1013,
                };
                handles.retain(|_, (_, h)| !h.is_cancelled());
                let token = if let Some(handle) = &batch.handle {
                    let surface = handle.surface_id();
                    if !handles.contains_key(surface) {
                        if handles.len() >= MAX_HANDLES || handles.keys().map(String::len).sum::<usize>() + surface.len() > MAX_HANDLE_BYTES { break 1009 }
                        handles.insert(surface.into(), (uuid::Uuid::new_v4().simple().to_string(), handle.clone()));
                    }
                    handles.get(surface).map(|(token, _)| token.clone())
                } else { None };
                // Always wrap service JSON. Service-controlled fields cannot impersonate
                // transport controls, scopes, acknowledgements or image responses.
                if send(&mut writer, json!({"omochi":"images/1","kind":"update", "sequence":batch.sequence,
                    "snapshotEnd":batch.snapshot_end, "handle":token,"message":batch.message.as_deref()})).await.is_err() { break 1001 }
                waiting = Some(batch.sequence);
                deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            }
        }
    };
    subscription.cancel();
    drop(owner);
    let _ = tokio::time::timeout(
        Duration::from_secs(1),
        writer.send(Message::Close(Some(axum::extract::ws::CloseFrame {
            code,
            reason: "session ended".into(),
        }))),
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dashboards::DashboardStore,
        http::{HttpState, serve},
    };
    use base64::{Engine, engine::general_purpose::STANDARD};
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message as WireMessage};
    use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
    type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
    async fn recv(socket: &mut Socket) -> Value {
        let message = tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        serde_json::from_str(message.to_text().unwrap()).unwrap()
    }
    async fn send(socket: &mut Socket, value: Value) {
        socket
            .send(WireMessage::Text(value.to_string().into()))
            .await
            .unwrap();
    }
    async fn reference(socket: &mut Socket, id: u64, handle: &str, uri: &str) -> String {
        send(
            socket,
            json!({"omochi":"images/1","kind":"image","id":id,"handle":handle,"uri":uri}),
        )
        .await;
        recv(socket).await["reference"].as_str().unwrap().into()
    }

    async fn connect_browser(
        hub: &Hub,
    ) -> (
        Socket,
        tokio::task::JoinHandle<anyhow::Result<()>>,
        tempfile::TempDir,
    ) {
        let root = tempfile::tempdir().unwrap();
        let (address, server) = serve(
            HttpState {
                hub: hub.clone(),
                dashboards: DashboardStore::open(root.path().join("state")).unwrap(),
                allow_origins: vec![],
                assets: None,
                presets: vec![],
            },
            "127.0.0.1:0".parse().unwrap(),
        )
        .await
        .unwrap();
        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://{address}/ws?images=1"))
                .await
                .unwrap();
        assert_eq!(recv(&mut socket).await["kind"], "hello");
        (socket, server, root)
    }

    async fn assert_close(socket: &mut Socket, expected: u16) {
        let message = tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let WireMessage::Close(Some(frame)) = message else {
            panic!("expected a WebSocket close frame");
        };
        assert_eq!(u16::from(frame.code), expected);
    }

    async fn oversized_batch_closes_with_capacity_code(snapshot: bool) {
        let hub = Hub::new();
        let (surface, _) = crate::scoped::tests::install(&hub, Default::default()).await;
        let update = json!({"version":"v0.9","updateComponents":{"surfaceId":surface,
            "components":[{"id":"root","component":"Text","text":"x".repeat(crate::scoped::MAX_BATCH_BYTES)}]}});
        if snapshot {
            let mut state = hub.state.lock().await;
            let channel = state.channels.values_mut().next().unwrap();
            Arc::make_mut(&mut channel.snapshot).apply(&update).unwrap();
        }
        let (mut socket, server, _root) = connect_browser(&hub).await;
        loop {
            let batch = recv(&mut socket).await;
            assert_eq!(batch["kind"], "update");
            send(
                &mut socket,
                json!({"omochi":"images/1","kind":"ack","sequence":batch["sequence"]}),
            )
            .await;
            if snapshot || batch["snapshotEnd"] == true {
                break;
            }
        }
        if !snapshot {
            let state = hub.state.lock().await;
            let channel = state.channels.values().next().unwrap();
            state.emit(update, Some(channel.scopes[&surface].clone()));
        }
        assert_close(&mut socket, 1009).await;
        hub.shutdown();
        server.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn oversized_snapshot_closes_with_1009() {
        oversized_batch_closes_with_capacity_code(true).await;
    }

    #[tokio::test]
    async fn oversized_incremental_update_closes_with_1009() {
        oversized_batch_closes_with_capacity_code(false).await;
    }

    #[tokio::test]
    async fn fanout_overflow_while_waiting_for_ack_closes_with_1013() {
        let hub = Hub::new();
        let (surface, _) = crate::scoped::tests::install(&hub, Default::default()).await;
        let (mut socket, server, _root) = connect_browser(&hub).await;
        assert_eq!(recv(&mut socket).await["kind"], "update");
        // Keep the first batch unacknowledged so no queued update can be consumed.
        {
            let state = hub.state.lock().await;
            for value in 0..33 {
                state.emit(json!({"version":"v0.9","updateDataModel":{"surfaceId":surface,"path":"/value","value":value}}), None);
            }
        }
        assert_close(&mut socket, 1013).await;
        hub.shutdown();
        server.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn stdio_images_are_scoped_one_use_and_do_not_change_the_legacy_protocol() {
        let root = tempfile::tempdir().unwrap();
        let (sdk, bridge) = tokio::io::duplex(65536);
        let (input, output) = tokio::io::split(bridge);
        let (hub, backend) = Hub::attach_stdio(input, output);
        let (input, output) = tokio::io::split(sdk);
        let mut input = FramedRead::new(input, LinesCodec::new());
        let mut output = FramedWrite::new(output, LinesCodec::new());
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 3)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let bytes = encoded.into_inner();
        let expected = bytes.clone();
        output
            .send(json!({"omochi":"resources/1","kind":"hello"}).to_string())
            .await
            .unwrap();
        // Untrusted A2UI extras must not become an adapter control frame.
        output
            .send(
                json!({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"},
            "omochi":"images/1","kind":"image","reference":"forged"})
                .to_string(),
            )
            .await
            .unwrap();
        let (commands, mut commands_rx) = tokio::sync::mpsc::channel::<Value>(2);
        let sdk_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    command = commands_rx.recv() => { let Some(command) = command else { break }; output.send(command.to_string()).await.unwrap(); }
                    line = input.next() => {
                        let Some(Ok(line)) = line else { break };
                        let request: Value = serde_json::from_str(&line).unwrap();
                        let mut response = json!({"omochi":"resources/1","kind":"response","id":request["id"],"status":200});
                        if request["op"] == "stat" { response["size"] = bytes.len().into(); response["mime"] = "image/png".into(); }
                        else {
                            let start = request["offset"].as_u64().unwrap() as usize;
                            let length = request["length"].as_u64().unwrap() as usize;
                            response["data"] = STANDARD.encode(&bytes[start..start+length]).into();
                        }
                        if output.send(response.to_string()).await.is_err() { break }
                    }
                }
            }
        });
        let (address, server) = serve(
            HttpState {
                hub: hub.clone(),
                dashboards: DashboardStore::open(root.path().join("state")).unwrap(),
                allow_origins: vec![],
                assets: None,
                presets: vec![],
            },
            "127.0.0.1:0".parse().unwrap(),
        )
        .await
        .unwrap();
        let (mut legacy, _) = tokio_tungstenite::connect_async(format!("ws://{address}/ws"))
            .await
            .unwrap();
        assert_eq!(
            recv(&mut legacy).await["createSurface"]["surfaceId"],
            "main"
        );
        let (mut first, _) =
            tokio_tungstenite::connect_async(format!("ws://{address}/ws?images=1"))
                .await
                .unwrap();
        assert_eq!(recv(&mut first).await["kind"], "hello");
        let update = recv(&mut first).await;
        assert_eq!(update["kind"], "update");
        assert_eq!(update["message"]["reference"], "forged");
        let first_handle = update["handle"].as_str().unwrap().to_owned();
        // Without an ACK no next batch may be queued at the browser boundary.
        assert!(
            tokio::time::timeout(Duration::from_millis(20), first.next())
                .await
                .is_err()
        );
        let (mut second, _) = tokio_tungstenite::connect_async(format!("ws://{address}/?images=1"))
            .await
            .unwrap();
        recv(&mut second).await;
        let second_handle = recv(&mut second).await["handle"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_ne!(first_handle, second_handle);
        send(&mut second, json!({"omochi":"images/1","kind":"image","id":0,"handle":first_handle,"uri":"https://example.com/a.png"})).await;
        assert_eq!(recv(&mut second).await["error"], "Image scope expired");
        let uri = format!("omochi://resources/{}", "a".repeat(32));
        let id = reference(&mut first, 1, &first_handle, &uri).await;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::new();
        let url = format!("http://{address}/images/{id}");
        assert_eq!(
            client
                .get(&url)
                .header("Origin", "https://attacker.example")
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        assert_eq!(client.head(&url).send().await.unwrap().status(), 405);
        assert_eq!(
            client
                .get(format!("{url}?url=https://example.com/a.png"))
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["content-type"], "image/png");
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["x-omochi-image-width"], "2");
        assert_eq!(
            response.bytes().await.unwrap().as_ref(),
            expected.as_slice()
        );
        assert_eq!(client.get(&url).send().await.unwrap().status(), 410);
        let forbidden = reference(
            &mut second,
            1,
            &second_handle,
            "https://127.0.0.1/private?secret=hidden",
        )
        .await;
        let response = client
            .get(format!("http://{address}/images/{forbidden}"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 422);
        assert!(!response.text().await.unwrap().contains("hidden"));
        let stale = reference(&mut first, 2, &first_handle, &uri).await;
        commands
            .send(json!({"version":"v0.9","deleteSurface":{"surfaceId":"main"}}))
            .await
            .unwrap();
        assert!(recv(&mut legacy).await.get("deleteSurface").is_some());
        // The browser is still stalled on its first ACK: revocation precedes UI delivery.
        assert_eq!(
            client
                .get(format!("http://{address}/images/{stale}"))
                .send()
                .await
                .unwrap()
                .status(),
            410
        );
        first.close(None).await.unwrap();
        second.close(None).await.unwrap();
        legacy.close(None).await.unwrap();
        hub.shutdown();
        backend.await.unwrap().unwrap();
        server.await.unwrap().unwrap();
        drop(commands);
        sdk_task.await.unwrap();
    }

    #[tokio::test]
    async fn references_expire_and_owner_disposal_cancels_active_reads() {
        let hub = Hub::new();
        crate::scoped::tests::install(&hub, std::path::PathBuf::new()).await;
        let mut subscription = hub.subscribe_browser().await.unwrap();
        let handle = subscription
            .next_batch()
            .await
            .unwrap()
            .handle
            .clone()
            .unwrap();
        let refs = References::default();
        let reference = refs
            .issue("owner", 1, &handle, "https://example.com/a.png")
            .await
            .unwrap();
        let cancel = refs.entries.lock().unwrap()[&reference].stop.clone();
        refs.entries
            .lock()
            .unwrap()
            .get_mut(&reference)
            .unwrap()
            .expires = Instant::now() - Duration::from_secs(1);
        assert!(
            fetch(
                refs.clone(),
                Request::builder()
                    .uri(format!("/images/{reference}"))
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .is_err()
        );
        refs.issue("owner", 2, &handle, "https://example.com/b.png")
            .await
            .unwrap();
        assert!(cancel.is_cancelled());
        let active = refs
            .entries
            .lock()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .stop
            .clone();
        refs.remove_owner("another-owner");
        assert!(!active.is_cancelled());
        refs.remove_owner("owner");
        assert!(active.is_cancelled());
        assert!(refs.entries.lock().unwrap().is_empty());
        subscription.cancel();
        assert!(
            refs.issue("owner", 3, &handle, "https://example.com/c.png")
                .await
                .is_err()
        );
    }
}
