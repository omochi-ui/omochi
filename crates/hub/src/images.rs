//! The common image resolver. No cookies, ambient proxy, credentials, or UI fallback.
use crate::{host::HostFuture, scoped::SurfaceHandle};
use async_compression::tokio::bufread::{BrotliDecoder, GzipDecoder, ZlibDecoder};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, BufReader},
    sync::Semaphore,
};
use tokio_util::{io::StreamReader, sync::CancellationToken, task::TaskTracker};
use url::{Host, Url};

pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_IMAGE_PIXELS: u64 = 4 * 1024 * 1024;
pub const MAX_IMAGE_DIMENSION: u32 = 8192;
pub const IMAGE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageError {
    InvalidUri,
    ForbiddenDestination,
    Network,
    RedirectLimit,
    InvalidImage,
    TooLarge,
    Busy,
    Cancelled,
    Timeout,
    Unavailable,
}
impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidUri => "unsupported image URI",
            Self::ForbiddenDestination => "image destination is not permitted",
            Self::Network => "image transfer failed",
            Self::RedirectLimit => "too many image redirects",
            Self::InvalidImage => "unsupported or invalid raster image",
            Self::TooLarge => "image exceeds size or pixel limits",
            Self::Busy => "image reader is busy; retry later",
            Self::Cancelled => "image request cancelled",
            Self::Timeout => "image request timed out",
            Self::Unavailable => "image source is no longer available",
        })
    }
}
impl std::error::Error for ImageError {}
type Result<T> = std::result::Result<T, ImageError>;
type Body = Pin<Box<dyn AsyncRead + Send>>;

pub struct ImageData {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
}

struct HttpResponse {
    status: u16,
    location: Option<String>,
    mime: String,
    encoding: String,
    length: Option<u64>,
    body: Body,
}
trait Https: Send + Sync {
    fn resolve<'a>(&'a self, host: &'a str) -> HostFuture<'a, Result<Vec<SocketAddr>>>;
    fn get<'a>(
        &'a self,
        url: &'a Url,
        addresses: &'a [SocketAddr],
    ) -> HostFuture<'a, Result<HttpResponse>>;
}

struct PinnedHttps;
impl Https for PinnedHttps {
    fn resolve<'a>(&'a self, host: &'a str) -> HostFuture<'a, Result<Vec<SocketAddr>>> {
        Box::pin(async move {
            let answers = tokio::net::lookup_host((host, 443))
                .await
                .map_err(|_| ImageError::Network)?;
            let addresses: Vec<_> = answers.take(65).collect();
            if addresses.is_empty() || addresses.len() > 64 {
                return Err(ImageError::ForbiddenDestination);
            }
            Ok(addresses)
        })
    }

    fn get<'a>(
        &'a self,
        url: &'a Url,
        addresses: &'a [SocketAddr],
    ) -> HostFuture<'a, Result<HttpResponse>> {
        Box::pin(async move {
            // Explicit roots also avoid a platform verifier that needs a JVM callback
            // on Android. TLS still verifies the original URL's hostname.
            let roots =
                rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .map_err(|_| ImageError::Network)?
            .with_root_certificates(roots)
            .with_no_client_auth();
            let host = url.host_str().ok_or(ImageError::InvalidUri)?;
            let client = reqwest::Client::builder()
                .use_preconfigured_tls(tls)
                .https_only(true)
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .no_gzip()
                .no_brotli()
                .no_deflate()
                .no_zstd()
                .resolve_to_addrs(host, addresses)
                .connect_timeout(Duration::from_secs(10))
                .timeout(IMAGE_TIMEOUT)
                .pool_max_idle_per_host(0)
                .build()
                .map_err(|_| ImageError::Network)?;
            let response = client
                .get(url.clone())
                .header(
                    "Accept",
                    "image/png,image/jpeg,image/webp,image/gif,image/bmp",
                )
                .header("Accept-Encoding", "gzip, deflate, br")
                .send()
                .await
                .map_err(|_| ImageError::Network)?;
            if response
                .remote_addr()
                .is_none_or(|peer| !addresses.contains(&peer))
            {
                return Err(ImageError::ForbiddenDestination);
            }
            let status = response.status().as_u16();
            let header = |name| {
                response
                    .headers()
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned)
            };
            let location = header("location");
            let mime = header("content-type").unwrap_or_default();
            let encoding = header("content-encoding").unwrap_or_default();
            let length = response.content_length();
            let mut received = 0usize;
            let stream = response.bytes_stream().map(move |chunk| {
                let chunk = chunk.map_err(|_| io::Error::other("image transfer failed"))?;
                received = received.saturating_add(chunk.len());
                if received > MAX_IMAGE_BYTES {
                    return Err(io::Error::new(
                        io::ErrorKind::FileTooLarge,
                        "encoded image too large",
                    ));
                }
                Ok(chunk)
            });
            Ok(HttpResponse {
                status,
                location,
                mime,
                encoding,
                length,
                body: Box::pin(StreamReader::new(stream)),
            })
        })
    }
}

#[derive(Clone)]
pub(crate) struct ImageResolver {
    permits: Arc<Semaphore>,
    https: Arc<dyn Https>,
    tasks: Arc<Mutex<BTreeMap<ImageScope, ScopeTasks>>>,
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ImageScope {
    gateway: String,
    // Both counters come from the Hub's monotonic epoch allocator; stdio uses
    // instance zero and its own monotonic surface generation allocator.
    instance: u64,
    generation: u64,
}
struct ScopeTasks {
    stop: CancellationToken,
    tracker: TaskTracker,
}
impl Default for ImageResolver {
    fn default() -> Self {
        Self {
            permits: Arc::new(Semaphore::new(2)),
            https: Arc::new(PinnedHttps),
            tasks: Arc::default(),
        }
    }
}
impl ImageResolver {
    pub(crate) async fn fetch(
        &self,
        handle: &SurfaceHandle,
        uri: &str,
        cancel: CancellationToken,
    ) -> Result<ImageData> {
        if uri.len() > 16 * 1024 {
            return Err(ImageError::InvalidUri);
        }
        let state = handle.hub.state.lock().await;
        let stdio = if let Some(backend) = &handle.hub.standalone {
            Some(backend.state.lock().await)
        } else {
            None
        };
        if let Some(stdio) = &stdio {
            handle.validate_stdio(stdio)
        } else {
            handle.validate(&state).map(|_| ())
        }
        .map_err(|_| ImageError::Unavailable)?;
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| ImageError::Busy)?;
        let gateway = handle
            .scope
            .route
            .split_once('~')
            .map_or("_", |(gateway, _)| gateway)
            .to_owned();
        let scope = ImageScope {
            gateway,
            instance: handle.scope.instance,
            generation: handle.scope.generation,
        };
        let cancel = cancel.child_token();
        let guard = cancel.clone().drop_guard();
        let resolver = self.clone();
        let handle = handle.clone();
        let uri = uri.to_owned();
        // Register under the same state lock used by revocation. An acknowledgement
        // cannot race past a request which has been authorized but not registered.
        let task = {
            let mut tasks = self.tasks.lock().unwrap();
            // Also collect work whose caller was dropped before it could prune.
            tasks.retain(|_, group| !group.tracker.is_empty());
            let group = tasks.entry(scope).or_insert_with(|| ScopeTasks {
                stop: handle.scope.stop.clone(),
                tracker: TaskTracker::new(),
            });
            group.tracker.spawn(async move {
                let _permit = permit;
                resolver.fetch_inner(&handle, &uri, cancel).await
            })
        };
        drop(stdio);
        drop(state);
        let result = task.await.map_err(|_| ImageError::Unavailable);
        self.tasks
            .lock()
            .unwrap()
            .retain(|_, group| !group.tracker.is_empty());
        drop(guard);
        result?
    }

    pub(crate) async fn join_revoked_gateway(&self, gateway: &str) -> Result<()> {
        // A registered standby may share a node with a live Unix owner. Only
        // cancelled scopes belong to teardown, including older owner generations
        // whose replacement has already taken over. Their state-locked validation
        // prevents any new image registration after cancellation.
        let trackers: Vec<_> = self
            .tasks
            .lock()
            .unwrap()
            .iter()
            .filter(|(scope, group)| scope.gateway == gateway && group.stop.is_cancelled())
            .map(|(scope, group)| (scope.clone(), group.tracker.clone()))
            .collect();
        for (_, tracker) in &trackers {
            tracker.close();
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            for (_, tracker) in &trackers {
                tracker.wait().await;
            }
        })
        .await
        .map_err(|_| ImageError::Timeout)?;
        let mut tasks = self.tasks.lock().unwrap();
        for (scope, _) in trackers {
            tasks.remove(&scope);
        }
        Ok(())
    }

    async fn fetch_inner(
        &self,
        handle: &SurfaceHandle,
        uri: &str,
        cancel: CancellationToken,
    ) -> Result<ImageData> {
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(ImageError::Cancelled),
            _ = handle.cancelled() => return Err(ImageError::Unavailable),
            result = tokio::time::timeout(IMAGE_TIMEOUT, async {
                if let Some(id) = uri.strip_prefix("omochi://resources/") { read_resource(handle, id).await }
                else { self.public_image(uri).await }
            }) => result.map_err(|_| ImageError::Timeout)??,
        };
        handle
            .validate_current()
            .await
            .map_err(|_| ImageError::Unavailable)?;
        if cancel.is_cancelled() {
            return Err(ImageError::Cancelled);
        }
        Ok(result)
    }

    async fn public_image(&self, uri: &str) -> Result<ImageData> {
        if uri.len() > 16 * 1024 {
            return Err(ImageError::InvalidUri);
        }
        let mut url = Url::parse(uri).map_err(|_| ImageError::InvalidUri)?;
        for redirects in 0..=3 {
            validate_url(&url)?;
            url.set_fragment(None);
            let addresses = match url.host().ok_or(ImageError::InvalidUri)? {
                Host::Ipv4(ip) => vec![SocketAddr::new(ip.into(), 443)],
                Host::Ipv6(ip) => vec![SocketAddr::new(ip.into(), 443)],
                Host::Domain(host) => self.https.resolve(host).await?,
            };
            if addresses.is_empty()
                || addresses.len() > 64
                || addresses
                    .iter()
                    .any(|a| a.port() != 443 || !public_ip(a.ip()))
            {
                return Err(ImageError::ForbiddenDestination);
            }
            let response = self.https.get(&url, &addresses).await?;
            if [301, 302, 303, 307, 308].contains(&response.status) {
                if redirects == 3 {
                    return Err(ImageError::RedirectLimit);
                }
                let next = response.location.ok_or(ImageError::Network)?;
                if next.len() > 16 * 1024 {
                    return Err(ImageError::InvalidUri);
                }
                url = url.join(&next).map_err(|_| ImageError::InvalidUri)?;
                continue;
            }
            if response.status != 200 {
                return Err(ImageError::Network);
            }
            let mime = response.mime.clone();
            return validate_image(read_body(response).await?, &mime);
        }
        Err(ImageError::RedirectLimit)
    }
}

fn validate_url(url: &Url) -> Result<()> {
    if url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ImageError::ForbiddenDestination);
    }
    Ok(())
}

fn v4_in(ip: Ipv4Addr, network: [u8; 4], bits: u32) -> bool {
    u32::from(ip) >> (32 - bits) == u32::from(Ipv4Addr::from(network)) >> (32 - bits)
}
fn public_ip(ip: IpAddr) -> bool {
    // Conservative policy based on IANA's special-purpose registries. Protocol
    // assignment and transition ranges are excluded even where individual anycast
    // addresses are reachable: https://www.iana.org/assignments/iana-ipv4-special-registry/
    // and https://www.iana.org/assignments/iana-ipv6-special-registry/ (2026-11-02).
    match ip {
        IpAddr::V4(ip) => ![
            ([0, 0, 0, 0], 8),
            ([10, 0, 0, 0], 8),
            ([100, 64, 0, 0], 10),
            ([127, 0, 0, 0], 8),
            ([169, 254, 0, 0], 16),
            ([172, 16, 0, 0], 12),
            ([192, 0, 0, 0], 24),
            ([192, 0, 2, 0], 24),
            ([192, 88, 99, 0], 24),
            ([192, 168, 0, 0], 16),
            ([198, 18, 0, 0], 15),
            ([198, 51, 100, 0], 24),
            ([203, 0, 113, 0], 24),
            ([224, 0, 0, 0], 4),
            ([240, 0, 0, 0], 4),
        ]
        .iter()
        .any(|(network, bits)| v4_in(ip, *network, *bits)),
        IpAddr::V6(ip) => {
            // Reject mapped/translated/tunnel addresses as well as local ranges.
            // Only native global-unicast space is eligible for external image reads.
            let s = ip.segments();
            s[0] & 0xe000 == 0x2000
                && !(s[0] == 0x2001 && s[1] < 0x0200)
                && !(s[0] == 0x2001 && s[1] == 0x0db8)
                && s[0] != 0x2002
                && !(s[0] == 0x3fff && s[1] & 0xf000 == 0)
        }
    }
}

async fn read_body(response: HttpResponse) -> Result<Vec<u8>> {
    if response
        .length
        .is_some_and(|length| length > MAX_IMAGE_BYTES as u64)
    {
        return Err(ImageError::TooLarge);
    }
    let buffer = BufReader::new(response.body);
    let mut reader: Body = match response.encoding.trim().to_ascii_lowercase().as_str() {
        "" | "identity" => Box::pin(buffer),
        "gzip" => Box::pin(GzipDecoder::new(buffer)),
        "deflate" => Box::pin(ZlibDecoder::new(buffer)),
        "br" => Box::pin(BrotliDecoder::new(buffer)),
        _ => return Err(ImageError::InvalidImage),
    };
    let mut result = Vec::new();
    let mut block = [0u8; 16 * 1024];
    loop {
        let size = reader.read(&mut block).await.map_err(|e| {
            if e.kind() == io::ErrorKind::FileTooLarge {
                ImageError::TooLarge
            } else {
                ImageError::Network
            }
        })?;
        if size == 0 {
            break;
        }
        if result.len().saturating_add(size) > MAX_IMAGE_BYTES {
            return Err(ImageError::TooLarge);
        }
        result.extend_from_slice(&block[..size]);
    }
    Ok(result)
}

async fn read_resource(handle: &SurfaceHandle, id: &str) -> Result<ImageData> {
    if id.len() != 32
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ImageError::InvalidUri);
    }
    let metadata = handle
        .resource(json!({"op":"stat","resource":id}))
        .await
        .map_err(|_| ImageError::Unavailable)?;
    if metadata["status"] != 200 {
        return Err(ImageError::Unavailable);
    }
    let length = metadata["size"].as_u64().ok_or(ImageError::InvalidImage)?;
    if length > MAX_IMAGE_BYTES as u64 {
        return Err(ImageError::TooLarge);
    }
    let mime = metadata["mime"].as_str().ok_or(ImageError::InvalidImage)?;
    let mut bytes = Vec::with_capacity(length as usize);
    while bytes.len() < length as usize {
        let count = (length as usize - bytes.len()).min(24 * 1024);
        let response = handle
            .resource(json!({"op":"read","resource":id,"offset":bytes.len(),"length":count}))
            .await
            .map_err(|_| ImageError::Unavailable)?;
        if response["status"] != 200 {
            return Err(ImageError::Unavailable);
        }
        let encoded = response["data"]
            .as_str()
            .filter(|value| value.len() <= 32768)
            .ok_or(ImageError::InvalidImage)?;
        let chunk = STANDARD
            .decode(encoded)
            .map_err(|_| ImageError::InvalidImage)?;
        if chunk.len() != count || STANDARD.encode(&chunk) != encoded {
            return Err(ImageError::InvalidImage);
        }
        bytes.extend_from_slice(&chunk);
    }
    validate_image(bytes, mime)
}

pub fn validate_image(bytes: Vec<u8>, mime: &str) -> Result<ImageData> {
    use image::ImageFormat;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(ImageError::TooLarge);
    }
    let format = image::guess_format(&bytes).map_err(|_| ImageError::InvalidImage)?;
    let expected = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::WebP => "image/webp",
        ImageFormat::Bmp => "image/bmp",
        _ => return Err(ImageError::InvalidImage),
    };
    if mime
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        != expected
    {
        return Err(ImageError::InvalidImage);
    }
    let mut reader = image::ImageReader::with_format(io::Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_IMAGE_PIXELS * 4);
    reader.limits(limits);
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| ImageError::InvalidImage)?;
    if width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
    {
        return Err(ImageError::TooLarge);
    }
    Ok(ImageData {
        bytes,
        mime: expected,
        width,
        height,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::VecDeque;
    use tokio::io::AsyncWriteExt;

    fn png() -> Vec<u8> {
        let mut bytes = io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 3)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }
    fn response(bytes: Vec<u8>, mime: &str, encoding: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            location: None,
            mime: mime.into(),
            encoding: encoding.into(),
            length: Some(bytes.len() as u64),
            body: Box::pin(io::Cursor::new(bytes)),
        }
    }
    struct ScriptedHttps {
        answers: Mutex<VecDeque<Vec<SocketAddr>>>,
        responses: Mutex<VecDeque<HttpResponse>>,
        requests: Mutex<Vec<(String, Vec<SocketAddr>)>>,
    }
    impl Https for ScriptedHttps {
        fn resolve<'a>(&'a self, _: &'a str) -> HostFuture<'a, Result<Vec<SocketAddr>>> {
            Box::pin(async {
                Ok(self
                    .answers
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or_else(|| vec!["8.8.8.8:443".parse().unwrap()]))
            })
        }
        fn get<'a>(
            &'a self,
            url: &'a Url,
            addresses: &'a [SocketAddr],
        ) -> HostFuture<'a, Result<HttpResponse>> {
            Box::pin(async move {
                self.requests
                    .lock()
                    .unwrap()
                    .push((url.to_string(), addresses.to_vec()));
                self.responses
                    .lock()
                    .unwrap()
                    .pop_front()
                    .ok_or(ImageError::Network)
            })
        }
    }
    fn scripted(
        responses: Vec<HttpResponse>,
        answers: Vec<Vec<SocketAddr>>,
    ) -> (ImageResolver, Arc<ScriptedHttps>) {
        let https = Arc::new(ScriptedHttps {
            answers: Mutex::new(answers.into()),
            responses: Mutex::new(responses.into()),
            requests: Mutex::default(),
        });
        (
            ImageResolver {
                https: https.clone(),
                ..Default::default()
            },
            https,
        )
    }

    #[test]
    fn private_special_and_transition_addresses_are_not_public_destinations() {
        for address in [
            "0.0.0.0",
            "10.1.2.3",
            "127.0.0.1",
            "100.64.1.1",
            "169.254.169.254",
            "172.31.255.255",
            "192.168.1.1",
            "192.0.0.170",
            "198.18.1.1",
            "203.0.113.4",
            "224.1.1.1",
            "255.255.255.255",
            "::",
            "::1",
            "::ffff:8.8.8.8",
            "::ffff:127.0.0.1",
            "64:ff9b::a00:1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "2001::1",
            "2001:db8::1",
            "2002:7f00:1::",
            "3fff::1",
        ] {
            assert!(!public_ip(address.parse().unwrap()), "{address}");
        }
        for address in [
            "8.8.8.8",
            "1.1.1.1",
            "172.32.0.1",
            "192.0.3.1",
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
        ] {
            assert!(public_ip(address.parse().unwrap()), "{address}");
        }
    }

    #[tokio::test]
    async fn forbids_credentials_schemes_ports_and_private_literal_spellings_before_connecting() {
        let (resolver, network) = scripted(vec![], vec![]);
        for url in [
            "http://example.com/x.png",
            "file:///etc/passwd",
            "data:image/png,abc",
            "https://u:secret@example.com/x.png",
            "https://example.com:8443/x.png",
            "https://127.0.0.1/x",
            "https://2130706433/x",
            "https://0x7f000001/x",
            "https://[::ffff:127.0.0.1]/x",
        ] {
            assert!(resolver.public_image(url).await.is_err(), "{url}");
        }
        assert!(network.requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn redirects_revalidate_dns_and_never_connect_to_rebound_private_answers() {
        let mut redirect = response(vec![], "", "");
        redirect.status = 302;
        redirect.location = Some("/second.png".into());
        let (resolver, network) = scripted(
            vec![redirect],
            vec![
                vec!["8.8.8.8:443".parse().unwrap()],
                vec!["127.0.0.1:443".parse().unwrap()],
            ],
        );
        assert!(matches!(
            resolver
                .public_image("https://image.example/first.png?token=secret#hidden")
                .await,
            Err(ImageError::ForbiddenDestination)
        ));
        let calls = network.requests.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].1,
            vec!["8.8.8.8:443".parse::<SocketAddr>().unwrap()]
        );
        assert!(!calls[0].0.contains('#'));
    }

    #[tokio::test]
    async fn mixed_dns_answers_downgrades_and_redirect_loops_are_rejected() {
        let (resolver, network) = scripted(
            vec![],
            vec![vec![
                "8.8.8.8:443".parse().unwrap(),
                "10.0.0.1:443".parse().unwrap(),
            ]],
        );
        assert!(matches!(
            resolver.public_image("https://image.example/a").await,
            Err(ImageError::ForbiddenDestination)
        ));
        assert!(network.requests.lock().unwrap().is_empty());
        for target in [
            "http://image.example/a",
            "https://127.0.0.1/a",
            "https://image.example/a",
        ] {
            let responses = (0..4)
                .map(|_| {
                    let mut r = response(vec![], "", "");
                    r.status = 307;
                    r.location = Some(target.into());
                    r
                })
                .collect();
            let (resolver, network) = scripted(responses, vec![]);
            assert!(
                resolver
                    .public_image("https://image.example/a")
                    .await
                    .is_err()
            );
            assert!(network.requests.lock().unwrap().len() <= 4);
        }
    }

    #[tokio::test]
    async fn mime_signatures_dimensions_and_decompressed_size_are_checked() {
        let bytes = png();
        let (resolver, _) = scripted(vec![response(bytes.clone(), "image/png", "")], vec![]);
        let image = resolver
            .public_image("https://image.example/a")
            .await
            .unwrap();
        assert_eq!((image.width, image.height), (2, 3));
        assert_eq!(image.bytes, bytes);
        assert!(matches!(
            validate_image(png(), "image/jpeg"),
            Err(ImageError::InvalidImage)
        ));
        assert!(matches!(
            validate_image(
                b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec(),
                "image/png"
            ),
            Err(ImageError::InvalidImage)
        ));
        let mut encoded = async_compression::tokio::write::GzipEncoder::new(Vec::new());
        encoded.write_all(&png()).await.unwrap();
        encoded.shutdown().await.unwrap();
        assert_eq!(
            read_body(response(encoded.into_inner(), "image/png", "gzip"))
                .await
                .unwrap(),
            png()
        );
        let mut bomb = async_compression::tokio::write::GzipEncoder::new(Vec::new());
        bomb.write_all(&vec![0; MAX_IMAGE_BYTES + 1]).await.unwrap();
        bomb.shutdown().await.unwrap();
        assert!(matches!(
            read_body(response(bomb.into_inner(), "image/png", "gzip")).await,
            Err(ImageError::TooLarge)
        ));
        let mut no_length = response(vec![0; MAX_IMAGE_BYTES + 1], "image/png", "");
        no_length.length = None;
        assert!(matches!(
            read_body(no_length).await,
            Err(ImageError::TooLarge)
        ));
        let mut big = io::Cursor::new(Vec::new());
        image::DynamicImage::new_luma8(4096, 2048)
            .write_to(&mut big, image::ImageFormat::Png)
            .unwrap();
        assert!(validate_image(big.into_inner(), "image/png").is_err());
    }

    /// Manual network smoke test: never part of the offline regression suite.
    #[tokio::test]
    #[ignore = "requires public HTTPS connectivity"]
    async fn public_https_uses_pinned_tls_and_returns_a_valid_raster() {
        let image = ImageResolver::default()
            .public_image(
                "https://www.google.com/images/branding/googlelogo/2x/googlelogo_color_92x30dp.png",
            )
            .await
            .unwrap();
        assert_eq!(image.mime, "image/png");
        assert!(image.width > 0 && image.height > 0);
        assert!(image.bytes.len() <= MAX_IMAGE_BYTES);
    }

    struct PendingHttps;
    pub(crate) fn pending_resolver() -> ImageResolver {
        ImageResolver {
            https: Arc::new(PendingHttps),
            ..Default::default()
        }
    }

    pub(crate) async fn wait_for_requests(resolver: &ImageResolver, count: usize) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while resolver.permits.available_permits() != 2 - count {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    pub(crate) fn active_requests(resolver: &ImageResolver) -> usize {
        2 - resolver.permits.available_permits()
    }

    struct TeardownGate {
        started: Option<tokio::sync::oneshot::Sender<()>>,
        release: std::sync::mpsc::Receiver<()>,
    }
    impl Drop for TeardownGate {
        fn drop(&mut self) {
            // Keep the actual fetch task in destruction until the test releases
            // it, so joining cannot pass merely because cancellation was sent.
            let _ = self.started.take().unwrap().send(());
            let _ = self.release.recv_timeout(Duration::from_secs(5));
        }
    }
    struct GatedHttps {
        gate: Mutex<Option<TeardownGate>>,
        requested: tokio::sync::Notify,
    }
    impl Https for GatedHttps {
        fn resolve<'a>(&'a self, _: &'a str) -> HostFuture<'a, Result<Vec<SocketAddr>>> {
            Box::pin(async { Ok(vec!["8.8.8.8:443".parse().unwrap()]) })
        }
        fn get<'a>(
            &'a self,
            _: &'a Url,
            _: &'a [SocketAddr],
        ) -> HostFuture<'a, Result<HttpResponse>> {
            Box::pin(async move {
                let _gate = self.gate.lock().unwrap().take();
                self.requested.notify_one();
                std::future::pending().await
            })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn teardown_joins_old_scope_destruction_without_waiting_for_its_live_replacement() {
        let (started, destroying) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let https = Arc::new(GatedHttps {
            gate: Mutex::new(Some(TeardownGate {
                started: Some(started),
                release: wait,
            })),
            requested: tokio::sync::Notify::new(),
        });
        let mut hub = crate::Hub::new();
        hub.images.https = https.clone();
        crate::scoped::tests::install(&hub, Default::default()).await;
        let mut old_subscription = hub.subscribe_scoped().await.unwrap();
        let old = old_subscription
            .next_batch()
            .await
            .unwrap()
            .handle
            .clone()
            .unwrap();
        let old_image = tokio::spawn(async move {
            old.image("https://image.example/old", CancellationToken::new())
                .await
        });
        https.requested.notified().await;

        // Replacing the source cancels its scope under the state lock, but its
        // network future has not finished destruction when the new owner starts.
        crate::scoped::tests::install(&hub, Default::default()).await;
        tokio::time::timeout(Duration::from_secs(1), destroying)
            .await
            .unwrap()
            .unwrap();
        let mut subscription = hub.subscribe_scoped().await.unwrap();
        let handle = subscription
            .next_batch()
            .await
            .unwrap()
            .handle
            .clone()
            .unwrap();
        let cancel = CancellationToken::new();
        let image = tokio::spawn({
            let cancel = cancel.clone();
            async move { handle.image("https://image.example/new", cancel).await }
        });
        https.requested.notified().await;
        let gateway = "a".repeat(64);
        let join = hub.images.join_revoked_gateway(&gateway);
        tokio::pin!(join);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut join)
                .await
                .is_err()
        );
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), join)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            old_image.await.unwrap(),
            Err(ImageError::Unavailable)
        ));
        assert!(!image.is_finished());
        assert_eq!(active_requests(&hub.images), 1);
        cancel.cancel();
        assert!(matches!(image.await.unwrap(), Err(ImageError::Cancelled)));
        assert!(hub.images.tasks.lock().unwrap().is_empty());
    }
    impl Https for PendingHttps {
        fn resolve<'a>(&'a self, _: &'a str) -> HostFuture<'a, Result<Vec<SocketAddr>>> {
            Box::pin(async { Ok(vec!["8.8.8.8:443".parse().unwrap()]) })
        }
        fn get<'a>(
            &'a self,
            _: &'a Url,
            _: &'a [SocketAddr],
        ) -> HostFuture<'a, Result<HttpResponse>> {
            Box::pin(std::future::pending())
        }
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_caller_abort_and_future_drop_release_network_slots() {
        let mut hub = crate::Hub::new();
        hub.images.https = Arc::new(PendingHttps);
        crate::scoped::tests::install(&hub, Default::default()).await;
        let mut subscription = hub.subscribe_scoped().await.unwrap();
        let handle = subscription
            .next_batch()
            .await
            .unwrap()
            .handle
            .clone()
            .unwrap();
        let cancel = CancellationToken::new();
        let request = tokio::spawn({
            let handle = handle.clone();
            let cancel = cancel.clone();
            async move {
                handle
                    .image("https://image.example/a?private=hidden", cancel)
                    .await
            }
        });
        while hub.images.permits.available_permits() == 2 {
            tokio::task::yield_now().await;
        }
        cancel.cancel();
        assert!(matches!(request.await.unwrap(), Err(ImageError::Cancelled)));
        assert_eq!(hub.images.permits.available_permits(), 2);
        let request = tokio::spawn({
            let handle = handle.clone();
            async move {
                handle
                    .image("https://image.example/b", CancellationToken::new())
                    .await
            }
        });
        while hub.images.permits.available_permits() == 2 {
            tokio::task::yield_now().await;
        }
        request.abort();
        let _ = request.await;
        while hub.images.permits.available_permits() != 2 {
            tokio::task::yield_now().await;
        }
        assert!(matches!(
            handle
                .image("https://image.example/c", CancellationToken::new())
                .await,
            Err(ImageError::Timeout)
        ));
        assert_eq!(hub.images.permits.available_permits(), 2);
    }

    #[tokio::test]
    async fn two_request_limit_and_revocation_wait_for_external_fetch_teardown() {
        let mut hub = crate::Hub::new();
        hub.images.https = Arc::new(PendingHttps);
        crate::scoped::tests::install(&hub, Default::default()).await;
        let mut subscription = hub.subscribe_scoped().await.unwrap();
        let handle = subscription
            .next_batch()
            .await
            .unwrap()
            .handle
            .clone()
            .unwrap();
        let first = tokio::spawn({
            let h = handle.clone();
            async move {
                h.image("https://image.example/a", CancellationToken::new())
                    .await
            }
        });
        let second = tokio::spawn({
            let h = handle.clone();
            async move {
                h.image("https://image.example/b", CancellationToken::new())
                    .await
            }
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while hub.images.permits.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(matches!(
            handle
                .image("https://image.example/c", CancellationToken::new())
                .await,
            Err(ImageError::Busy)
        ));
        hub.revoke_gateway(&"a".repeat(64)).await;
        hub.images
            .join_revoked_gateway(&"a".repeat(64))
            .await
            .unwrap();
        assert!(matches!(first.await.unwrap(), Err(ImageError::Unavailable)));
        assert!(matches!(
            second.await.unwrap(),
            Err(ImageError::Unavailable)
        ));
        assert_eq!(hub.images.permits.available_permits(), 2);
    }
}
