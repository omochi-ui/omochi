//! Browser HTTP/WebSocket boundary; all routes share the same Origin policy.
use crate::{
    Hub,
    dashboards::{ApiError, DashboardStore, validate},
};
use axum::{
    Extension, Json, Router,
    body::{Body, to_bytes},
    extract::{
        Query, Request, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, Method, StatusCode, header},
    middleware::{self, Next},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{any, get},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{convert::Infallible, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct HttpState {
    pub hub: Hub,
    pub dashboards: DashboardStore,
    pub allow_origins: Vec<String>,
    pub assets: Option<PathBuf>,
    pub presets: Vec<Value>,
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.0).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({"error":self.1})),
        )
            .into_response()
    }
}
fn allowed(headers: &HeaderMap, extra: &[String]) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    if extra.iter().any(|item| item == origin) {
        return true;
    }
    url::Url::parse(origin).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https")
            && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
    })
}
async fn origin(State(state): State<Arc<HttpState>>, request: Request, next: Next) -> Response {
    if !allowed(request.headers(), &state.allow_origins) {
        return ApiError::new(403, "Forbidden origin").into_response();
    }
    let origin = request.headers().get(header::ORIGIN).cloned();
    let mut response = if request.method() == Method::OPTIONS {
        let mut response = StatusCode::NO_CONTENT.into_response();
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            "GET, POST, PUT, DELETE, OPTIONS".parse().unwrap(),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            "Content-Type".parse().unwrap(),
        );
        response
    } else {
        next.run(request).await
    };
    response
        .headers_mut()
        .append(header::VARY, "Origin".parse().unwrap());
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    if let Some(origin) = origin {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    }
    response
}
pub async fn serve(
    state: HttpState,
    address: SocketAddr,
) -> anyhow::Result<(SocketAddr, JoinHandle<anyhow::Result<()>>)> {
    let mut ids = std::collections::BTreeSet::new();
    for preset in &state.presets {
        let id = preset
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty() && id.len() <= 100)
            .ok_or_else(|| anyhow::anyhow!("invalid preset ID"))?;
        if !ids.insert(id) {
            anyhow::bail!("duplicate preset ID")
        }
        validate(preset.clone()).map_err(|e| anyhow::anyhow!(e.1))?;
    }
    *state.hub.owner_presets.write().unwrap() = state.presets.clone();
    let stop = state.hub.stop.clone();
    let browser_tasks = state.hub.browser_tasks.clone();
    let state = Arc::new(state);
    let app = Router::new()
        .route("/ws", get(upgrade))
        .route("/images/{id}", any(crate::browser_images::image))
        .route(
            "/resources/{route}/{resource}",
            get(crate::resource_http::resource),
        );
    let app = if state.hub.standalone.is_some() {
        app.route("/", get(upgrade))
    } else {
        app.route("/services", get(services))
            .route("/events", get(events))
            .route("/api/dashboards", any(dashboards))
            .route("/api/dashboards/{id}", any(dashboards))
            .route("/api/sharing", get(sharing))
            .route("/api/owners", any(owners))
            .route("/api/owners/{id}", any(owners))
            .route("/api/shares", any(shares))
            .route("/api/shares/{id}", any(shares))
            .route("/api/presets", get(presets))
            .route("/api/connections", get(connections))
            .fallback(static_ui)
    }
    .layer(Extension(crate::browser_images::References::default()))
    .layer(middleware::from_fn_with_state(state.clone(), origin))
    .with_state(state);
    let listener = tokio::net::TcpListener::bind(address).await?;
    let address = listener.local_addr()?;
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await?;
        // HTTP upgrades are detached from axum's graceful shutdown. Wait for
        // their close frames before the CLI can exit and drop the runtime.
        browser_tasks.close();
        tokio::time::timeout(Duration::from_secs(2), browser_tasks.wait()).await?;
        Ok(())
    });
    Ok((address, task))
}
async fn sharing(State(state): State<Arc<HttpState>>) -> Json<Value> {
    Json(
        state
            .hub
            .dashboard_host()
            .unwrap_or(json!({"enabled":false})),
    )
}
/// Local owner administration; the common Origin middleware protects every route.
async fn shares(
    State(state): State<Arc<HttpState>>,
    request: Request,
) -> Result<Response, ApiError> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Create {
        board_id: String,
        permissions: crate::sharing::Permissions,
    }
    let method = request.method().clone();
    let id = request
        .uri()
        .path()
        .strip_prefix("/api/shares/")
        .map(str::to_owned);
    let boards = state.dashboards.clone();
    let control = crate::control::HubControl {
        hub: state.hub.clone(),
        boards: boards.clone(),
    };
    if method == Method::GET && id.is_none() {
        return Ok(
            Json(control.execute(crate::control::Command::SharesList).await?).into_response(),
        );
    }
    if method == Method::DELETE {
        let id = id.ok_or_else(|| ApiError::new(405, "Method not allowed"))?;
        control
            .execute(crate::control::Command::ShareRevoke { id })
            .await?;
        return Ok(Json(json!({"deleted":true})).into_response());
    }
    if method != Method::POST || id.is_some() {
        return Err(ApiError::new(405, "Method not allowed"));
    }
    if request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .map(str::trim)
        != Some("application/json")
    {
        return Err(ApiError::new(415, "Use application/json"));
    }
    let bytes = to_bytes(request.into_body(), 4096)
        .await
        .map_err(|_| ApiError::new(413, "Sharing request too large"))?;
    let input: Create =
        serde_json::from_slice(&bytes).map_err(|_| ApiError::new(400, "Invalid share"))?;
    let value = control
        .execute(crate::control::Command::ShareCreate {
            board_id: input.board_id,
            permissions: input.permissions,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(value)).into_response())
}
async fn connections(State(state): State<Arc<HttpState>>) -> Json<Vec<Value>> {
    Json(state.hub.connection_status().await)
}
async fn services(State(state): State<Arc<HttpState>>) -> Json<Vec<Value>> {
    Json(state.hub.services().await)
}
async fn presets(State(state): State<Arc<HttpState>>) -> Json<Vec<Value>> {
    Json(state.presets.clone())
}
async fn events(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let mut changes = state.hub.changes();
    let stream = async_stream::stream! {
        loop {
            yield Ok::<_, Infallible>(Event::default().event("services").data(serde_json::to_string(&state.hub.services().await).unwrap()));
            tokio::select! { _ = state.hub.stop.cancelled() => break, result = changes.recv() => { if matches!(result, Err(tokio::sync::broadcast::error::RecvError::Closed)) { break } } }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
async fn upgrade(
    State(state): State<Arc<HttpState>>,
    Extension(references): Extension<crate::browser_images::References>,
    Query(query): Query<std::collections::BTreeMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Response {
    let guard = state.hub.browser_tasks.token();
    ws.max_message_size(omochi_protocol::MAX_FRAME_BYTES)
        .max_frame_size(omochi_protocol::MAX_FRAME_BYTES)
        .on_upgrade(move |socket| async move {
            let _guard = guard;
            if query.get("images").is_some_and(|version| version == "1") {
                crate::browser_images::browser(socket, state.hub.clone(), references).await;
            } else {
                browser(socket, state.hub.clone()).await;
            }
        })
}
async fn browser(socket: WebSocket, hub: Hub) {
    let subscription = hub.subscribe().await;
    let (mut writer, mut reader) = socket.split();
    for message in subscription.snapshot {
        let sent = tokio::select! {
            _ = hub.stop.cancelled() => break,
            result = tokio::time::timeout(
                Duration::from_secs(5),
                writer.send(Message::Text(message.to_string().into()))
            ) => result,
        };
        if !matches!(sent, Ok(Ok(()))) {
            return;
        }
    }
    let mut events = subscription.events;
    let read = async {
        while let Some(Ok(message)) = reader.next().await {
            match message {
                Message::Text(text) => {
                    let Ok(value) = serde_json::from_str(&text) else {
                        return 1002;
                    };
                    if hub.action(value).await.is_err() {
                        return 1002;
                    }
                }
                Message::Ping(_) | Message::Pong(_) => {}
                Message::Binary(_) => return 1003,
                _ => break,
            }
        }
        1000
    };
    let write = async {
        while let Ok(message) = events.recv().await {
            if !matches!(
                tokio::time::timeout(
                    Duration::from_secs(5),
                    writer.send(Message::Text(message.to_string().into()))
                )
                .await,
                Ok(Ok(()))
            ) {
                break;
            }
        }
    };
    let code = tokio::select! {
        code = read => code,
        _ = write => 1001,
        _ = hub.stop.cancelled() => hub.close_code.load(std::sync::atomic::Ordering::Acquire),
    };
    let _ = tokio::time::timeout(
        Duration::from_secs(1),
        writer.send(Message::Close(Some(axum::extract::ws::CloseFrame {
            code,
            reason: "session ended".into(),
        }))),
    )
    .await;
}
async fn dashboards(
    State(state): State<Arc<HttpState>>,
    request: Request,
) -> Result<Response, ApiError> {
    let id = request
        .uri()
        .path()
        .strip_prefix("/api/dashboards/")
        .map(str::to_owned);
    if id
        .as_ref()
        .is_some_and(|id| id.is_empty() || id.contains(['/', '%']))
    {
        return Err(ApiError::new(404, "Dashboard not found"));
    }
    let method = request.method().clone();
    let store = state.dashboards.clone();
    let control = crate::control::HubControl {
        hub: state.hub.clone(),
        boards: store.clone(),
    };
    if method == Method::GET {
        let value = control.board_async(id, None, false).await?;
        return Ok(Json(value).into_response());
    }
    if !((method == Method::POST && id.is_none())
        || ((method == Method::PUT || method == Method::DELETE) && id.is_some()))
    {
        return Err(ApiError::new(405, "Method not allowed"));
    }
    if request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.split(';').next())
        .map(str::trim)
        != Some("application/json")
    {
        return Err(ApiError::new(415, "Use application/json"));
    }
    let bytes = to_bytes(request.into_body(), 1024 * 1024)
        .await
        .map_err(|_| ApiError::new(413, "Dashboard request is too large"))?;
    let input = serde_json::from_slice(&bytes).map_err(|_| ApiError::new(400, "Invalid JSON"))?;
    let remove = method == Method::DELETE;
    let result = control.board_async(id, Some(input), remove).await?;
    if remove {
        Ok(Json(json!({"deleted":true})).into_response())
    } else {
        Ok((
            if method == Method::POST {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            },
            Json(result),
        )
            .into_response())
    }
}
static BUNDLED_UI: include_dir::Dir = include_dir::include_dir!("$OMOCHI_DASHBOARD_ASSETS");

async fn static_ui(State(state): State<Arc<HttpState>>, request: Request) -> Response {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let path = request.uri().path();
    let relative = if path == "/" || path == "/index.html" {
        "index.html"
    } else if path.starts_with("/assets/") {
        &path[1..]
    } else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if relative
        .split('/')
        .any(|s| s == ".." || s == "." || s.contains(['%', '\\']))
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let bytes = if let Some(root) = &state.assets {
        let Ok(root) = tokio::fs::canonicalize(root).await else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let Ok(path) = tokio::fs::canonicalize(root.join(relative)).await else {
            return StatusCode::NOT_FOUND.into_response();
        };
        if !path.starts_with(root) {
            return StatusCode::FORBIDDEN.into_response();
        }
        let Ok(bytes) = tokio::fs::read(&path).await else {
            return StatusCode::NOT_FOUND.into_response();
        };
        bytes
    } else {
        let Some(file) = BUNDLED_UI.get_file(relative) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        file.contents().to_vec()
    };
    let path = std::path::Path::new(relative);
    let mime = match path.extension().and_then(|x| x.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    };
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, bytes.len())
        .header("X-Content-Type-Options", "nosniff")
        .body(if request.method() == Method::HEAD {
            Body::empty()
        } else {
            Body::from(bytes)
        })
        .unwrap()
}

async fn owners(
    State(state): State<Arc<HttpState>>,
    request: Request,
) -> Result<Response, ApiError> {
    let store = state.dashboards.owners();
    let control = crate::control::HubControl {
        hub: state.hub.clone(),
        boards: state.dashboards.clone(),
    };
    if request.uri().path().starts_with("/api/owners/") && request.method() != Method::DELETE {
        return Err(ApiError::new(405, "Method not allowed"));
    }
    match *request.method() {
        Method::GET => Ok(Json(control.owners_list().await?).into_response()),
        Method::POST => {
            let host = state
                .hub
                .dashboard_host()
                .ok_or_else(|| ApiError::new(400, "Enable remote access first"))?;
            if host["relayUrls"].as_array().is_none_or(|r| r.is_empty()) {
                return Err(ApiError::new(503, "Relay address unavailable"));
            }
            let (grant, secret) = control.owner_create().await?;
            use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
            let payload = json!({"version":1,"hubId":host["hubId"],"relayUrls":host["relayUrls"],"grantId":grant.id,"mode":"owner","secret":secret});
            Ok(Json(json!({"grant":grant,"url":format!("{}#owner:{}",host["webUrl"].as_str().unwrap(),URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap()))})).into_response())
        }
        Method::DELETE => {
            let id = request
                .uri()
                .path()
                .strip_prefix("/api/owners/")
                .ok_or_else(|| ApiError::new(405, "Missing owner ID"))?;
            store.revoke_live(id).await?;
            Ok(Json(json!({"deleted":true})).into_response())
        }
        _ => Err(ApiError::new(405, "Method not allowed")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    #[tokio::test]
    async fn browser_api_preserves_crud_conflicts_sse_and_origin_checks() {
        let root = tempfile::tempdir().unwrap();
        let hub = Hub::new();
        let dashboards = DashboardStore::open(root.path().join("state")).unwrap();
        let assets = root.path().join("assets");
        std::fs::create_dir(&assets).unwrap();
        std::fs::write(assets.join("index.html"), "<html>dashboard</html>").unwrap();
        let (address, task) = serve(
            HttpState {
                hub: hub.clone(),
                dashboards,
                allow_origins: vec![],
                assets: Some(assets),
                presets: vec![],
            },
            "127.0.0.1:0".parse().unwrap(),
        )
        .await
        .unwrap();
        let base = format!("http://{address}");
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::new();
        assert_eq!(
            client
                .get(format!("{base}/services"))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap(),
            json!([])
        );
        assert_eq!(
            client
                .get(format!("{base}/"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "<html>dashboard</html>"
        );
        assert_eq!(
            client
                .get(format!("{base}/services"))
                .header("Origin", "https://attacker.example")
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        assert_eq!(
            client
                .get(format!("{base}/api/connections"))
                .header("Origin", "https://attacker.example")
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        let created = client
            .post(format!("{base}/api/dashboards"))
            .json(&json!({"name":"Home","panels":[]}))
            .send()
            .await
            .unwrap();
        assert_eq!(created.status(), 201);
        let created: Value = created.json().await.unwrap();
        let shares_url = format!("{base}/api/shares");
        assert_eq!(
            client
                .get(&shares_url)
                .header("Origin", "https://attacker.example")
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        let public =
            json!({"boardId":created["id"], "permissions":{"interact":false,"editLayout":false}});
        assert_eq!(
            client
                .post(&shares_url)
                .header("Origin", "https://attacker.example")
                .json(&public)
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        let share = client.post(&shares_url).json(&public).send().await.unwrap();
        assert_eq!(share.status(), 201);
        let share: Value = share.json().await.unwrap();
        assert_eq!(share["secret"].as_str().unwrap().len(), 64);
        let listed: Value = client
            .get(&shares_url)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1);
        assert!(listed[0].get("secret").is_none());
        assert!(listed[0].get("digest").is_none());
        let forbidden = json!({"boardId":created["id"], "public":true, "permissions":{"interact":true,"editLayout":false}});
        assert_eq!(
            client
                .post(&shares_url)
                .json(&forbidden)
                .send()
                .await
                .unwrap()
                .status(),
            400
        );
        let share_url = format!("{shares_url}/{}", share["grant"]["id"].as_str().unwrap());
        assert_eq!(
            client
                .delete(&share_url)
                .header("Origin", "https://attacker.example")
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        assert_eq!(
            client.delete(&share_url).send().await.unwrap().status(),
            200
        );
        assert_eq!(
            client.delete(&share_url).send().await.unwrap().status(),
            404
        );
        let url = format!("{base}/api/dashboards/{}", created["id"].as_str().unwrap());
        assert_eq!(
            client
                .put(&url)
                .json(&json!({"name":"stale","panels":[],"revision":0}))
                .send()
                .await
                .unwrap()
                .status(),
            409
        );
        let saved: Value = client
            .put(&url)
            .json(&json!({"name":"new","panels":[],"revision":1}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(saved["revision"], 2);
        assert_eq!(
            client
                .delete(&url)
                .json(&json!({"revision":2}))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap(),
            json!({"deleted":true})
        );
        let mut events = client.get(format!("{base}/events")).send().await.unwrap();
        let first = events.chunk().await.unwrap().unwrap();
        assert!(String::from_utf8_lossy(&first).contains("event: services"));
        drop(events);
        let mut request = format!("ws://{address}/ws").into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Origin", "https://attacker.example".parse().unwrap());
        let error = tokio_tungstenite::connect_async(request).await.unwrap_err();
        assert!(
            matches!(error, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 403)
        );
        let (mut websocket, _) = tokio_tungstenite::connect_async(format!("ws://{address}/ws"))
            .await
            .unwrap();
        websocket.close(None).await.unwrap();
        drop(websocket);
        hub.shutdown();
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
