use crate::{
    output::{Decoder, OUTPUT_LIMIT, OutputBuffer},
    publisher::Publisher,
    surface::TextSurface,
};
use anyhow::{Result, bail};
use std::time::Duration;
use tokio::{task::JoinHandle, time::Instant};
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
pub struct Options {
    pub url: reqwest::Url,
    pub interval: Duration,
    pub timeout: Duration,
    pub expect_status: (u16, u16),
    pub expect_body: Option<String>,
}
pub struct Matcher {
    expected: String,
    tail: String,
    decoder: Decoder,
    pub matched: bool,
}
impl Matcher {
    pub fn new(expected: Option<&str>) -> Self {
        Self {
            expected: expected.unwrap_or("").into(),
            tail: String::new(),
            decoder: Decoder::default(),
            matched: expected.is_none_or(str::is_empty),
        }
    }
    pub fn write(&mut self, bytes: &[u8]) {
        if !self.matched {
            let text = self.decoder.write(bytes);
            self.feed(&text);
        }
    }
    pub fn finish(&mut self) {
        if !self.matched {
            let text = self.decoder.finish();
            self.feed(&text);
        }
    }
    fn feed(&mut self, text: &str) {
        self.tail.push_str(text);
        self.matched = self.tail.contains(&self.expected);
        if self.matched {
            self.tail.clear();
            return;
        }
        let mut start = self
            .tail
            .len()
            .saturating_sub(self.expected.len().saturating_sub(1));
        while !self.tail.is_char_boundary(start) {
            start += 1;
        }
        self.tail.drain(..start);
    }
}
pub struct Response {
    pub healthy: bool,
    pub status: String,
    pub metadata: String,
    pub body: OutputBuffer,
}
pub fn client() -> Result<reqwest::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(20))
        .build()?)
}
pub async fn request(client: reqwest::Client, options: Options) -> Response {
    let started = Instant::now();
    let result = tokio::time::timeout(options.timeout,async {
        let mut response = client.get(options.url.clone()).send().await?;
        let code = response.status().as_u16();
        let content_type = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("unknown").to_owned();
        let length = response.headers().get("content-length").and_then(|v| v.to_str().ok()).map(str::to_owned);
        let url = response.url().to_string();
        let media = content_type.split(';').next().unwrap().trim().to_ascii_lowercase();
        let is_json = media == "application/json" || media.ends_with("+json");
        let textual = media.starts_with("text/") || is_json || media.ends_with("+xml") || matches!(media.as_str(),"application/javascript"|"application/xml"|"application/x-www-form-urlencoded");
        let mut body = OutputBuffer::default(); let mut matcher = Matcher::new(options.expect_body.as_deref());
        let mut visible = Vec::new(); let mut truncated = false;
        if textual {
            while let Some(chunk) = response.chunk().await? {
                let available = OUTPUT_LIMIT.saturating_sub(visible.len());
                visible.extend_from_slice(&chunk[..chunk.len().min(available)]);
                matcher.write(&chunk);
                if chunk.len() > available {
                    truncated = true;
                    if matcher.matched { break; }
                }
            }
            matcher.finish();
            let raw = Decoder::default().write(&visible);
            let display = if is_json && !truncated {
                match serde_json::from_str::<serde_json::Value>(&raw) {
                    Ok(value)=>serde_json::to_string_pretty(&value)?,
                    Err(_)=>format!("Invalid JSON response\n\n{raw}"),
                }
            } else { raw };
            body.reset(&display);
            if truncated { body.append("\n\n[Response body truncated]"); }
        }
        let healthy = (options.expect_status.0..=options.expect_status.1).contains(&code) && (options.expect_body.is_none() || textual && matcher.matched);
        let size = length.map(|n| format!("{n} bytes")).unwrap_or_else(|| if textual && !truncated { format!("{} bytes",visible.len()) } else { "unknown".into() });
        let expectation = format!("status {}–{}{}",options.expect_status.0,options.expect_status.1,options.expect_body.as_ref().map(|body| format!(" and body contains {}",serde_json::to_string(body).unwrap())).unwrap_or_default());
        Ok::<_,anyhow::Error>(Response {
            healthy,status:format!("{} · HTTP {code} · {} ms · {}",if healthy { "Healthy" } else { "Unhealthy" },started.elapsed().as_millis(),chrono::Utc::now().to_rfc3339()),
            metadata:format!("URL: {url}  \nContent-Type: {content_type}  \nSize: {size}  \nExpectation: {expectation}"),body,
        })
    }).await;
    match result {
        Ok(Ok(response)) => response,
        other => Response {
            healthy: false,
            status: format!(
                "Unhealthy · {} · {} ms",
                match other {
                    Ok(Err(e)) => e.to_string(),
                    _ => "request timed out".into(),
                },
                started.elapsed().as_millis()
            ),
            metadata: format!("URL: {}", options.url),
            body: OutputBuffer::default(),
        },
    }
}
pub async fn run(
    publisher: Publisher,
    name: String,
    options: Options,
    stop: CancellationToken,
) -> Result<()> {
    if !matches!(options.url.scheme(), "http" | "https") {
        bail!("http URL must use http or https");
    }
    let client = client()?;
    let mut surface = TextSurface::new(publisher.clone(), name);
    let mut current: Option<JoinHandle<Response>> = None;
    let mut body = OutputBuffer::default();
    let mut status = "Starting…".to_owned();
    let mut metadata = String::new();
    let mut previous = None;
    let mut failures = 0u64;
    let mut transition = String::new();
    let mut cursor = 0;
    let mut dirty = true;
    let mut paused = false;
    let mut next = Some(Instant::now());
    let events_cancel = CancellationToken::new();
    let mut interval = tokio::time::interval(Duration::from_millis(50));
    let result = async {
        loop {
            tokio::select! {
                _ = stop.cancelled()=>break,
                _ = interval.tick(), if dirty => {
                    let mut controls = vec![if paused { ("resume","Resume") } else { ("pause","Pause") }];
                    if current.is_none() { controls.push(("run","Run now")); }
                    let sections = if metadata.is_empty() { vec![] } else { vec![("metadata","Response",metadata.clone())] };
                    surface.render_sections(&format!("{status}{}",if paused { " · Paused" } else { "" }),&body,&controls,&sections).await?; dirty = false;
                },
                _ = async { match next { Some(at)=>tokio::time::sleep_until(at).await,None=>std::future::pending().await } } => {
                    next = None; status = "Requesting…".into(); dirty = true;
                    current = Some(tokio::spawn(request(client.clone(),options.clone())));
                },
                page = publisher.events.read(cursor,30000,&events_cancel) => {
                    let page = page?; cursor = page["nextCursor"].as_u64().unwrap();
                    for event in page["events"].as_array().unwrap() {
                        let action = &event["message"]["action"]; if action["surfaceId"] != "main" { continue; }
                        match action["name"].as_str() {
                            Some("pause") if !paused=>{ paused = true; next = None; dirty = true; },
                            Some("resume") if paused=>{ paused = false; if current.is_none() { next = Some(Instant::now()); } dirty = true; },
                            Some("run") if current.is_none()=>{ next = Some(Instant::now()); },
                            _=>{},
                        }
                    }
                    if page["closed"] == true { break; }
                },
                response = async { match current.as_mut() { Some(task)=>task.await,None=>std::future::pending().await } } => {
                    current = None; let response = response?;
                    failures = if response.healthy { 0 } else { failures.saturating_add(1) };
                    if previous.is_none() { transition = format!("Initial result: {}",if response.healthy { "healthy" } else { "unhealthy" }); }
                    else if previous != Some(response.healthy) { transition = if response.healthy { "Recovered" } else { "Became unhealthy" }.into(); }
                    previous = Some(response.healthy); status = response.status; body = response.body;
                    metadata = format!("{}  \nConsecutive failures: {failures}  \nState: {transition}",response.metadata);
                    dirty = true; if !paused { next = Some(Instant::now()+options.interval); }
                },
            }
        }
        Ok(())
    }.await;
    if let Some(task) = current {
        task.abort();
        let _ = task.await;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matcher_bounds_overlap_and_handles_split_utf8_and_final_invalid_bytes() {
        for expected in ["!", "needle", "日本語"] {
            let mut matcher = Matcher::new(Some(expected));
            for _ in 0..200 {
                matcher.write(&[b'x'; 8192]);
                assert!(matcher.tail.len() < expected.len());
            }
            for byte in expected.bytes() {
                matcher.write(&[byte]);
            }
            matcher.finish();
            assert!(matcher.matched);
            assert!(matcher.tail.is_empty());
        }
        let mut matcher = Matcher::new(Some("�"));
        matcher.write(&[0xe3]);
        matcher.finish();
        assert!(matcher.matched);
    }
}

#[cfg(test)]
mod integration {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    async fn server(
        response: Vec<u8>,
        delay: Duration,
    ) -> (reqwest::Url, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut headers = [0u8; 8192];
            let _ = socket.read(&mut headers).await;
            tokio::time::sleep(delay).await;
            let _ = socket.write_all(&response).await;
        });
        (url, task)
    }
    fn options(url: reqwest::Url) -> Options {
        Options {
            url,
            interval: Duration::from_secs(60),
            timeout: Duration::from_secs(2),
            expect_status: (200, 299),
            expect_body: None,
        }
    }
    fn response(code: u16, media: &str, body: &[u8]) -> Vec<u8> {
        let mut bytes = format!("HTTP/1.1 {code} Test\r\nContent-Type: {media}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }
    #[tokio::test]
    async fn late_body_match_json_binary_expectation_and_timeout() {
        let mut body = vec![b'x'; 1600000];
        body.extend_from_slice("日本語".as_bytes());
        let (url, task) = server(response(200, "text/plain", &body), Duration::ZERO).await;
        let mut config = options(url);
        config.expect_body = Some("日本語".into());
        let result = request(client().unwrap(), config).await;
        assert!(result.healthy);
        assert!(result.body.text.contains("Response body truncated"));
        assert!(result.body.text.len() <= OUTPUT_LIMIT);
        task.await.unwrap();
        let (url, task) = server(
            response(200, "application/problem+json", br#"{"ok":true}"#),
            Duration::ZERO,
        )
        .await;
        let result = request(client().unwrap(), options(url)).await;
        assert!(result.healthy);
        assert!(result.body.text.contains("\n  \"ok\": true"));
        task.await.unwrap();
        let (url, task) = server(
            response(200, "application/octet-stream", b"needle"),
            Duration::ZERO,
        )
        .await;
        let mut config = options(url);
        config.expect_body = Some("needle".into());
        let result = request(client().unwrap(), config).await;
        assert!(!result.healthy);
        assert!(result.body.text.is_empty());
        task.await.unwrap();
        let (url, task) = server(response(200, "text/plain", b"ok"), Duration::from_secs(10)).await;
        let mut config = options(url);
        config.timeout = Duration::from_millis(20);
        let result = request(client().unwrap(), config).await;
        assert!(!result.healthy);
        assert!(result.status.contains("timed out"));
        task.abort();
    }
}
