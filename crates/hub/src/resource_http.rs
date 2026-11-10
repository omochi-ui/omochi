use crate::{Hub, dashboards::ApiError, http::HttpState};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, Method, Response, header},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{io, sync::Arc};
async fn fetch(hub: &Hub, route: &str, request: Value) -> Result<Value, ApiError> {
    let value = hub
        .resource(route, request)
        .await
        .map_err(|_| ApiError::new(503, "Resource unavailable"))?;
    if value["status"] != 200 {
        let status = value["status"]
            .as_u64()
            .filter(|s| [400, 404, 416, 500, 501, 503, 504].contains(s))
            .unwrap_or(502);
        return Err(ApiError::new(status as u16, "Resource request failed"));
    }
    Ok(value)
}
pub async fn resource(
    State(state): State<Arc<HttpState>>,
    Path((route, resource)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
) -> Result<Response<Body>, ApiError> {
    if headers
        .get("sec-fetch-site")
        .is_some_and(|v| v == "cross-site")
        && !headers.contains_key(header::ORIGIN)
    {
        return Err(ApiError::new(403, "Forbidden resource origin"));
    }
    if resource.len() != 32
        || !resource
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ApiError::new(404, "Resource not found"));
    }
    let metadata = fetch(&state.hub, &route, json!({"op":"stat","resource":resource})).await?;
    let invalid = || ApiError::new(502, "Invalid resource metadata");
    let size = metadata["size"]
        .as_u64()
        .filter(|size| *size <= 9_007_199_254_740_991)
        .ok_or_else(invalid)?;
    let mime = metadata["mime"].as_str().ok_or_else(invalid)?;
    let valid_mime = mime.split_once('/').is_some_and(|(kind, suffix)| {
        ["image", "audio", "video"].contains(&kind)
            && !suffix.is_empty()
            && suffix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b))
    });
    let etag = metadata["etag"].as_str().ok_or_else(invalid)?;
    if !valid_mime
        || etag.len() != 34
        || !etag.starts_with('"')
        || !etag.ends_with('"')
        || !etag.as_bytes()[1..33]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return Err(invalid());
    }
    let (mut start, mut length, mut partial) = (0u64, size, false);
    if method == Method::GET
        && headers.get(header::IF_RANGE).is_none_or(|v| v == etag)
        && let Some(range) = headers
            .get(header::RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("bytes="))
        && let Some((left, right)) = range.split_once('-')
        && (!left.is_empty() || !right.is_empty())
        && left.bytes().all(|b| b.is_ascii_digit())
        && right.bytes().all(|b| b.is_ascii_digit())
    {
        let parsed = (|| {
            let start = if left.is_empty() {
                size.saturating_sub(right.parse::<u64>().ok()?)
            } else {
                left.parse().ok()?
            };
            let end = if !left.is_empty() && !right.is_empty() {
                right.parse::<u64>().ok()?.min(size.saturating_sub(1))
            } else {
                size.saturating_sub(1)
            };
            (start <= end && start < size).then(|| (start, end - start + 1))
        })();
        let Some((offset, count)) = parsed else {
            return Ok(Response::builder()
                .status(416)
                .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                .body(Body::empty())
                .unwrap());
        };
        start = offset;
        length = count;
        partial = true;
    }
    let mut response = Response::builder()
        .status(if partial { 206 } else { 200 })
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, length)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, etag)
        .header("X-Content-Type-Options", "nosniff")
        .header("Content-Security-Policy", "sandbox; default-src 'none'")
        .header(header::VARY, "Origin, Sec-Fetch-Site");
    if partial {
        response = response.header(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{size}", start, start + length - 1),
        );
    }
    if method == Method::HEAD {
        return Ok(response.body(Body::empty()).unwrap());
    }
    let hub = state.hub.clone();
    let stream = async_stream::try_stream! {
        let mut remaining = length;
        while remaining > 0 {
            let count = remaining.min(24 * 1024);
            let chunk = fetch(&hub, &route, json!({"op":"read","resource":resource,"offset":start,"length":count})).await.map_err(|e| io::Error::other(e.1))?;
            let encoded = chunk["data"].as_str().filter(|data| data.len() <= 32768).ok_or_else(|| io::Error::other("Invalid resource chunk"))?;
            let bytes = STANDARD.decode(encoded).map_err(io::Error::other)?;
            if bytes.len() as u64 != count || STANDARD.encode(&bytes) != encoded { Err(io::Error::other("Short resource read"))?; }
            yield axum::body::Bytes::from(bytes);
            start += count; remaining -= count;
        }
    };
    let stream: std::pin::Pin<
        Box<dyn futures_util::Stream<Item = Result<axum::body::Bytes, io::Error>> + Send>,
    > = Box::pin(stream);
    Ok(response.body(Body::from_stream(stream)).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboards::DashboardStore;
    use futures_util::{SinkExt, StreamExt};
    use std::{os::unix::fs::PermissionsExt, time::Duration};
    use tokio_util::codec::{Framed, LinesCodec};
    async fn assert_unsupported(client: &reqwest::Client, url: &str) {
        for method in [reqwest::Method::GET, reqwest::Method::HEAD] {
            let response = client.request(method.clone(), url).send().await.unwrap();
            assert_eq!(response.status(), 501, "{method}");
            if method == reqwest::Method::HEAD {
                assert!(response.bytes().await.unwrap().is_empty());
            }
        }
    }

    #[tokio::test]
    async fn stdio_resources_preserve_unsupported_and_unavailable_statuses() {
        let root = tempfile::tempdir().unwrap();
        let (sdk, bridge) = tokio::io::duplex(65536);
        let (input, output) = tokio::io::split(bridge);
        let (hub, attached) = Hub::attach_stdio(input, output);
        let mut sdk = Framed::new(sdk, LinesCodec::new());
        sdk.send(
            json!({"version":"v0.9","createSurface":{"surfaceId":"legacy","catalogId":"test"}})
                .to_string(),
        )
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while hub.subscribe().await.snapshot.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let (address, http) = crate::http::serve(
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
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::new();
        let url = format!("http://{address}/resources/_/{}", "a".repeat(32));
        assert_unsupported(&client, &url).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(20), sdk.next())
                .await
                .is_err()
        );
        sdk.send(json!({"omochi":"resources/1","kind":"hello"}).to_string())
            .await
            .unwrap();
        sdk.send(
            json!({"version":"v0.9","createSurface":{"surfaceId":"ready","catalogId":"test"}})
                .to_string(),
        )
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while hub.subscribe().await.snapshot.len() < 2 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let sdk = tokio::spawn(async move {
            while let Some(Ok(line)) = sdk.next().await {
                let request: Value = serde_json::from_str(&line).unwrap();
                let mut response = json!({"omochi":"resources/1","kind":"response","id":request["id"],"status":200});
                if request["op"] == "stat" {
                    response["size"] = 5.into();
                    response["mime"] = "image/png".into();
                    response["etag"] = format!("\"{}\"", "a".repeat(32)).into();
                } else {
                    let offset = request["offset"].as_u64().unwrap() as usize;
                    let length = request["length"].as_u64().unwrap() as usize;
                    response["data"] = STANDARD.encode(&b"hello"[offset..offset + length]).into();
                }
                sdk.send(response.to_string()).await.unwrap();
            }
        });
        let full = client.get(&url).send().await.unwrap();
        assert_eq!(full.status(), 200);
        assert_eq!(full.text().await.unwrap(), "hello");
        let range = client
            .get(&url)
            .header("Range", "bytes=1-3")
            .send()
            .await
            .unwrap();
        assert_eq!(range.status(), 206);
        assert_eq!(range.headers()["content-range"], "bytes 1-3/5");
        assert_eq!(range.text().await.unwrap(), "ell");
        let head = client.head(&url).send().await.unwrap();
        assert_eq!(head.status(), 200);
        assert_eq!(head.headers()["content-length"], "5");
        assert!(head.bytes().await.unwrap().is_empty());
        sdk.abort();
        let _ = sdk.await;
        attached.await.unwrap().unwrap();
        for method in [reqwest::Method::GET, reqwest::Method::HEAD] {
            assert_eq!(
                client.request(method, &url).send().await.unwrap().status(),
                503
            );
        }
        hub.shutdown();
        http.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn browser_resource_ranges_are_streamed_from_the_sdk() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        omochi_local::LocalRuntime::prepare(Some(root.path().to_path_buf())).unwrap();
        let directory = root.path().join("svc-app~");
        omochi_local::files::private_directory(&directory).unwrap();
        std::fs::write(
            directory.join("manifest.json"),
            r#"{"version":2,"serviceId":"app","name":"App","endpoint":{"kind":"unix","path":"a2ui.sock"}}"#,
        )
        .unwrap();
        let listener = tokio::net::UnixListener::bind(directory.join("a2ui.sock")).unwrap();
        let (announce, announced) = tokio::sync::oneshot::channel();
        let sdk = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut frames = Framed::new(socket, LinesCodec::new());
            announced.await.unwrap();
            frames
                .send(json!({"omochi":"resources/1","kind":"hello"}).to_string())
                .await
                .unwrap();
            frames.send(json!({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"}}).to_string()).await.unwrap();
            while let Some(Ok(line)) = frames.next().await {
                let request: Value = serde_json::from_str(&line).unwrap();
                let mut response = json!({"omochi":"resources/1","kind":"response","id":request["id"],"status":200});
                if request["op"] == "stat" {
                    response["size"] = 5.into();
                    response["mime"] = "image/png".into();
                    response["etag"] = format!("\"{}\"", "a".repeat(32)).into();
                } else {
                    let offset = request["offset"].as_u64().unwrap() as usize;
                    let length = request["length"].as_u64().unwrap() as usize;
                    response["data"] = STANDARD.encode(&b"hello"[offset..offset + length]).into();
                }
                if frames.send(response.to_string()).await.is_err() {
                    break;
                }
            }
        });
        let node = "a".repeat(64);
        let gateway = omochi_gateway::Gateway::new();
        let discovery = gateway.discover(root.path().to_owned());
        let server = gateway
            .listen_local(root.path().to_path_buf(), node.clone(), "gateway".into())
            .await
            .unwrap();
        let hub = Hub::new();
        let attached = hub.attach_local(root.path().to_path_buf());
        let (address, http) = crate::http::serve(
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
        tokio::time::timeout(Duration::from_secs(5), async {
            while hub.services().await.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let url = format!("http://{address}/resources/{node}~app/{}", "a".repeat(32));
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::new();
        assert_unsupported(&client, &url).await;
        announce.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while hub.subscribe().await.snapshot.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let full = client.get(&url).send().await.unwrap();
        assert_eq!(full.status(), 200);
        assert_eq!(full.text().await.unwrap(), "hello");
        let range = client
            .get(&url)
            .header("Range", "bytes=1-3")
            .send()
            .await
            .unwrap();
        assert_eq!(range.status(), 206);
        assert_eq!(range.headers()["content-range"], "bytes 1-3/5");
        assert_eq!(range.text().await.unwrap(), "ell");
        assert_eq!(
            client
                .get(&url)
                .header("Range", "bytes=99-")
                .send()
                .await
                .unwrap()
                .status(),
            416
        );
        let head = client.head(&url).send().await.unwrap();
        assert_eq!(head.headers()["content-length"], "5");
        assert!(head.bytes().await.unwrap().is_empty());
        assert_eq!(
            client
                .get(&url)
                .header("Sec-Fetch-Site", "cross-site")
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        sdk.abort();
        let _ = sdk.await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while !hub.services().await.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        for method in [reqwest::Method::GET, reqwest::Method::HEAD] {
            assert_eq!(
                client.request(method, &url).send().await.unwrap().status(),
                503
            );
        }
        hub.shutdown();
        attached.await.unwrap();
        http.await.unwrap().unwrap();
        gateway.shutdown();
        server.await.unwrap().unwrap();
        discovery.await.unwrap().unwrap();
    }
}
