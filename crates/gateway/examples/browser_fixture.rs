//! Isolated real SDK apps and gateways for Chromium integration. Never a CLI mode.
#[path = "browser_fixture/faults.rs"]
mod faults;
#[path = "browser_fixture/sdk_gate.rs"]
mod sdk_gate;
use anyhow::{Context, Result, bail};
use iroh::{Endpoint, RelayMode, endpoint::presets};
use iroh_relay::{
    server::{CertConfig, RelayConfig, Server, ServerConfig, TlsConfig},
    tls::CaTlsConfig,
};
use omochi_gateway::Gateway;
use omochi_protocol::{ALPN, PAIR_ALPN};
use omochi_transport::{
    pairing::Invite,
    store::{Role, Store},
};
use omochi_ui::{
    App, ButtonOptions, ImageOptions, TextFieldOptions, Transport, UnixSocketServerOptions,
    UnixSocketServerTransport,
};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinHandle};

struct ObservedSdk {
    inner: UnixSocketServerTransport,
    gateway: usize,
    service: String,
    load: mpsc::Receiver<u64>,
    emitted: Arc<AtomicU64>,
}
impl Transport for ObservedSdk {
    fn set_resources(&mut self, resources: omochi_ui::resources::ResourceRegistry) {
        self.inner.set_resources(resources);
    }
    fn capabilities(&self) -> std::collections::BTreeSet<String> {
        self.inner.capabilities()
    }
    // Match Transport's boxed async-trait ABI without another fixture dependency.
    fn send<'a, 'b, 'future>(
        &'a mut self,
        message: &'b Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = omochi_ui::Result<()>> + Send + 'future>>
    where
        'a: 'future,
        'b: 'future,
        Self: 'future,
    {
        Box::pin(async move {
            self.inner.send(message).await?;
            if message["updateComponents"]["components"]
                .as_array()
                .is_some_and(|components| {
                    components.iter().any(|component| {
                        component["text"].as_str().is_some_and(|text| {
                            text.starts_with("LOAD ") && !text.starts_with("LOAD 00000 ")
                        })
                    })
                })
            {
                let emitted = self.emitted.fetch_add(1, Ordering::SeqCst) + 1;
                if emitted % 1000 == 0 {
                    output(json!({"kind":"load-progress","emitted":emitted}))
                        .map_err(|_| omochi_ui::Error::Protocol("fixture output closed".into()))?;
                }
            }
            Ok(())
        })
    }
    fn receive<'a, 'future>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = omochi_ui::Result<Option<Value>>> + Send + 'future>,
    >
    where
        'a: 'future,
        Self: 'future,
    {
        Box::pin(async move {
            let message = tokio::select! {
                message = self.inner.receive() => message?,
                Some(sequence) = self.load.recv() => return Ok(Some(json!({"type":"invoke","action":"load","input":sequence}))),
            };
            if let Some(value) = &message
                && value.get("action").is_some()
            {
                let surfaces = value["metadata"]["a2uiClientDataModel"]["surfaces"].as_object();
                let scoped = surfaces.is_some_and(|surfaces| {
                    surfaces.len() == 1 && surfaces.contains_key("main/nested")
                });
                let draft = value["metadata"]["a2uiClientDataModel"]["surfaces"]["main/nested"]["$local"]
                    ["form"]["text"]
                    == "unfinished draft";
                // Report only fixed booleans/IDs; never raw form models or frames.
                output(json!({"kind":"action-observed","gateway":self.gateway,"service":self.service,"scoped":scoped,
                    "bareTarget":value["action"]["surfaceId"] == "main/nested","draftPresent":draft,
                    "tag":value["action"]["context"]["input"]["tag"].as_str().filter(|tag| ["dispatched","revoked","local"].contains(tag))}))
                    .map_err(|_| omochi_ui::Error::Protocol("fixture output closed".into()))?;
            }
            Ok(message)
        })
    }
}

fn output(value: Value) -> Result<()> {
    println!("{value}");
    io::stdout().flush()?;
    Ok(())
}
fn certificates(path: &Path) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
    Ok(
        rustls_pemfile::certs(&mut io::BufReader::new(std::fs::File::open(path)?))
            .collect::<std::result::Result<Vec<_>, _>>()?,
    )
}
async fn relay(cert: &Path, key: &Path, port: u16) -> Result<Server> {
    let certificates = certificates(cert)?;
    let key = rustls_pemfile::private_key(&mut io::BufReader::new(std::fs::File::open(key)?))?
        .context("TLS key missing")?;
    let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(certificates, key)?;
    let mut relay = RelayConfig::new((std::net::Ipv4Addr::LOCALHOST, 0));
    relay.tls = Some(TlsConfig::new(
        (std::net::Ipv4Addr::LOCALHOST, port),
        CertConfig::Manual { server_config: tls },
    ));
    let mut config = ServerConfig::default();
    config.relay = Some(relay);
    Ok(Server::spawn(config).await?)
}
#[derive(Default)]
struct State {
    count: u64,
    draft: String,
    image: usize,
    sequence: u64,
}
fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(data);
    let mut crc = u32::MAX;
    for byte in &bytes[4..] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    bytes.extend_from_slice(&(!crc).to_be_bytes());
    bytes
}
fn fixture_png(width: u32, height: u32, depth: u8, color: u8, raw: &[u8]) -> Vec<u8> {
    assert!(raw.len() <= 65535);
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[depth, color, 0, 0, 0]);
    // A single stored DEFLATE block with zlib Adler-32, so no compression dependency is needed.
    let length = raw.len() as u16;
    let mut deflate = vec![0x78, 0x01, 1];
    deflate.extend_from_slice(&length.to_le_bytes());
    deflate.extend_from_slice(&(!length).to_le_bytes());
    deflate.extend_from_slice(raw);
    let (mut a, mut b) = (1u32, 0u32);
    for byte in raw {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    deflate.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend(png_chunk(b"IHDR", &ihdr));
    bytes.extend(png_chunk(b"IDAT", &deflate));
    bytes.extend(png_chunk(b"IEND", &[]));
    bytes
}
fn image_cases() -> Vec<(&'static str, Vec<u8>)> {
    let red = fixture_png(1, 1, 8, 6, &[0, 255, 0, 0, 255]);
    let mut bad_signature = red.clone();
    bad_signature[0] = 0;
    let mut bad_crc = red.clone();
    *bad_crc.last_mut().unwrap() ^= 1;
    let mut bad_idat = red.clone();
    let idat_len = u32::from_be_bytes(bad_idat[33..37].try_into().unwrap()) as usize;
    bad_idat[41] = 0;
    let replacement = png_chunk(b"IDAT", &bad_idat[41..41 + idat_len]);
    bad_idat.splice(33..45 + idat_len, replacement);
    let mut apng = red.clone();
    apng.splice(33..33, png_chunk(b"acTL", &[0, 0, 0, 1, 0, 0, 0, 0]));
    let mut many_chunks = red.clone();
    let mut chunks = Vec::new();
    for _ in 0..4094 {
        chunks.extend(png_chunk(b"tEXt", &[]));
    }
    many_chunks.splice(33..33, chunks);
    let mut wide = vec![0; 1 + 8192 * 4];
    for pixel in 0..8192 {
        wide[1 + pixel * 4 + 1] = 255;
        wide[1 + pixel * 4 + 3] = 255;
    }
    vec![
        ("RGBA red", red),
        ("RGB green", fixture_png(1, 1, 8, 2, &[0, 0, 255, 0])),
        ("wide multi-chunk PNG", fixture_png(8192, 1, 8, 6, &wide)),
        ("bad signature", bad_signature),
        ("bad CRC", bad_crc),
        ("invalid IHDR", fixture_png(0, 1, 8, 6, &[0, 0, 0, 0, 0])),
        ("bad IDAT", bad_idat),
        ("inflation excess", fixture_png(1, 1, 8, 6, &[0; 65535])),
        ("inflation deficit", fixture_png(1, 1, 8, 6, &[0; 4])),
        ("APNG", apng),
        ("16-bit PNG", fixture_png(1, 1, 16, 6, &[0; 9])),
        ("side limit", fixture_png(8193, 1, 8, 6, &[0; 32773])),
        ("chunk limit", many_chunks),
        ("encoded limit", vec![0; 8 * 1024 * 1024 + 1]),
    ]
}
struct Source {
    gateway: Gateway,
    store: Store,
    listener: JoinHandle<()>,
    discovery: JoinHandle<Result<()>>,
    apps: Vec<JoinHandle<omochi_ui::Result<()>>>,
    invitation: String,
    id: String,
    relay: String,
    load: Vec<mpsc::Sender<u64>>,
    faults: Option<faults::Faults>,
    sdk_gate: Option<(sdk_gate::Gate, JoinHandle<()>)>,
}
async fn source(
    root: &Path,
    sockets: &Path,
    index: usize,
    custom: Option<(&str, &Path)>,
    media: bool,
    load: bool,
    emitted: Arc<AtomicU64>,
    protocol_faults: bool,
    gated_sdk: bool,
) -> Result<Source> {
    let directory = root.join(format!("gateway-{index}"));
    std::fs::create_dir_all(&directory)?;
    // A valid 64-byte service ID must still fit sockaddr_un's pathname limit.
    let runtime = sockets.join(index.to_string());
    let store = Store::open(directory.join("state"))?;
    let mut apps = Vec::new();
    let mut load_inputs = Vec::new();
    let mut sdk_gate = None;
    for service_id in [
        "s".to_string(),
        "s".repeat(16),
        "s".repeat(17),
        "s".repeat(64),
    ] {
        let mut app = App::new(State::default());
        let load_images = if load && index == 0 && service_id == "s" {
            [0, 1]
                .into_iter()
                .map(|_| {
                    Ok((
                        app.resources
                            .bytes(&fixture_png(1, 1, 8, 6, &[0, 255, 0, 0, 255]), "image/png")?,
                        app.resources
                            .bytes(&fixture_png(1, 1, 8, 2, &[0, 0, 255, 0]), "image/png")?,
                    ))
                })
                .collect::<omochi_ui::Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        app.action::<u64, _, _>("load", |sequence, context| async move {
            context
                .update(move |state| state.sequence = sequence)
                .await?;
            Ok(())
        })?;
        let media = media && index == 0 && service_id == "s";
        let images = if media {
            image_cases()
                .into_iter()
                .map(|(name, bytes)| Ok((name, app.resources.bytes(&bytes, "image/png")?)))
                .collect::<omochi_ui::Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        let next_image = app.action::<Value, _, _>("next_image", |_, context| async move {
            context
                .update(|state| {
                    state.image += 1;
                })
                .await?;
            Ok(())
        })?;
        let action = app.action::<Value, _, _>("increment", |input, context| async move {
            context
                .update(move |state| {
                    state.count += 1;
                    state.draft = input["draft"].as_str().unwrap_or("").to_owned();
                })
                .await?;
            Ok(())
        })?;
        app.surface("main/nested", move |ui, state| {
            if load {
                // Exactly 1024 ASCII payload bytes; stable declaration identity emits an incremental update.
                let payload = format!("LOAD {:05} ", state.sequence);
                ui.text(format!("{payload}{}", "x".repeat(1024 - payload.len())))?;
                if !load_images.is_empty() {
                    let (red, green) = &load_images[(state.sequence / 160) as usize % 2];
                    for (uri, description) in [(red, "Load red PNG"), (green, "Load green PNG")] {
                        ui.image(
                            uri.clone(),
                            ImageOptions {
                                description: Some(json!(description)),
                                ..Default::default()
                            },
                        )?;
                    }
                }
            }
            ui.text(format!("Gateway {index} count: {}", state.count))?;
            ui.text(format!("Received draft: {}", state.draft))?;
            let draft = ui.local("form", json!({"text":""}))?;
            ui.text_field(
                "draft",
                TextFieldOptions {
                    label: json!(format!("Draft {index}")),
                    bind: Some(draft.bind("/text")?),
                    ..Default::default()
                },
            )?;
            ui.button(
                format!("Increment {index}"),
                action.call(json!({"draft":draft.value("/text")?}))?,
                ButtonOptions::default(),
            )?;
            if media {
                let (name, uri) = &images[state.image % images.len()];
                ui.text(format!("PNG case: {name}"))?;
                ui.image(
                    uri.clone(),
                    ImageOptions {
                        description: Some(json!("SDK PNG case")),
                        ..Default::default()
                    },
                )?;
                ui.button(
                    "Next PNG case",
                    next_image.call(json!({}))?,
                    ButtonOptions::default(),
                )?;
                ui.text(format!(
                    "Markdown ![SDK Markdown PNG]({}) and [safe link](https://example.com/)",
                    images[1].1
                ))?;
                ui.image(
                    "https://media.invalid/image.png",
                    ImageOptions {
                        description: Some(json!("External image")),
                        ..Default::default()
                    },
                )?;
                ui.text("Markdown ![External Markdown](https://media.invalid/markdown.png)")?;
                ui.video("https://media.invalid/video.mp4", None)?;
                ui.audio_player(
                    "https://media.invalid/audio.mp3",
                    Some(json!("External audio")),
                    None,
                )?;
            }
            Ok(())
        })?;
        let transport = UnixSocketServerTransport::bind(
            UnixSocketServerOptions::new(&service_id)
                .with_name(format!("SDK {index} {service_id}"))
                .with_runtime_dir(&runtime),
        )
        .await?;
        if gated_sdk && index == 0 && service_id == "s" {
            sdk_gate = Some(sdk_gate::install(
                &runtime.join(&service_id).join("a2ui.sock"),
            )?);
        }
        let (load_tx, load_rx) = mpsc::channel(8);
        load_inputs.push(load_tx);
        apps.push(tokio::task::spawn_local(app.start_with(ObservedSdk {
            inner: transport,
            gateway: index,
            service: service_id,
            load: load_rx,
            emitted: emitted.clone(),
        })));
    }
    let gateway = Gateway::new();
    let discovery = gateway.discover(runtime);
    tokio::time::timeout(Duration::from_secs(10), async {
        while gateway.list().await.len() != 4 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    let endpoint = if let Some((relay, cert)) = custom {
        Endpoint::builder(presets::Minimal)
            .secret_key(store.identity(Role::Gateway)?)
            .relay_mode(RelayMode::custom([relay.parse()?]))
            .ca_tls_config(CaTlsConfig::custom_roots(certificates(cert)?))
            .alpns(vec![ALPN.to_vec(), PAIR_ALPN.to_vec()])
            .bind()
            .await?
    } else {
        omochi_transport::bind(store.identity(Role::Gateway)?, false, None).await?
    };
    tokio::time::timeout(Duration::from_secs(30), endpoint.online()).await?;
    let address = endpoint.addr();
    let relay = address
        .relay_urls()
        .next()
        .context("no TLS relay")?
        .to_string();
    let invitation = Invite::issue(&store, &endpoint)?.encode()?;
    let id = endpoint.id().to_string();
    let faults = (protocol_faults && index == 0).then(faults::Faults::new);
    let listener = if let Some(faults) = &faults {
        faults::listen(
            endpoint,
            store.clone(),
            gateway.clone(),
            format!("Fixture {index}"),
            faults.clone(),
        )
    } else {
        gateway.listen_remote(endpoint, store.clone(), format!("Fixture {index}"))?
    };
    Ok(Source {
        gateway,
        store,
        listener,
        discovery,
        apps,
        invitation,
        id,
        relay,
        load: load_inputs,
        faults,
        sdk_gate,
    })
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tokio::task::LocalSet::new().run_until(run()).await
}
async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let public = args.iter().any(|arg| arg == "--public-relay");
    let media = args.iter().any(|arg| arg == "--media");
    let load = args.iter().any(|arg| arg == "--load");
    let protocol_faults = args.iter().any(|arg| arg == "--faults");
    let gated_sdk = args.iter().any(|arg| arg == "--sdk-gate");
    let emitted = Arc::new(AtomicU64::new(0));
    let argument = |name: &str| -> Result<PathBuf> {
        Ok(args
            .iter()
            .position(|arg| arg == name)
            .and_then(|index| args.get(index + 1))
            .context("fixture TLS paths required")?
            .into())
    };
    let cert = if public {
        None
    } else {
        Some(argument("--cert")?)
    };
    let key = if public {
        None
    } else {
        Some(argument("--key")?)
    };
    let ca = if public {
        None
    } else {
        Some(argument("--ca")?)
    };
    let mut server = if public {
        None
    } else {
        Some(relay(cert.as_ref().unwrap(), key.as_ref().unwrap(), 0).await?)
    };
    let relay_port = server
        .as_ref()
        .and_then(Server::https_addr)
        .map(|addr| addr.port());
    let relay_url = server
        .as_ref()
        .map(|server| format!("https://{}/", server.https_addr().unwrap()));
    let parent = std::env::var_os("BROWSER_HUB_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let root = tempfile::Builder::new()
        .prefix("browser-gateway-")
        .tempdir_in(parent)?;
    let sockets = tempfile::Builder::new().prefix("obs-").tempdir_in("/tmp")?;
    let mut sources = Vec::new();
    for index in 0..2 {
        sources.push(
            source(
                root.path(),
                sockets.path(),
                index,
                relay_url
                    .as_deref()
                    .map(|url| (url, ca.as_deref().unwrap())),
                media,
                load,
                emitted.clone(),
                protocol_faults,
                gated_sdk,
            )
            .await?,
        );
    }
    output(
        json!({"kind":"ready","gateways":sources.iter().enumerate().map(|(index, source)| json!({
        "nodeId":source.id,"name":format!("Fixture {index}"),"relayUrls":[source.relay],"invitation":source.invitation,
    })).collect::<Vec<_>>()}),
    )?;
    let (tx, mut rx) = mpsc::channel(8);
    let mut load_job: Option<JoinHandle<Result<()>>> = None;
    let mut operations = Vec::new();
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines().map_while(Result::ok) {
            if tx.blocking_send(line).is_err() {
                break;
            }
        }
    });
    while let Some(command) = rx.recv().await {
        let command: Value = serde_json::from_str(&command)?;
        match command["op"].as_str() {
            Some("trust-status") => {
                let peer = command["peer"].as_str().context("public peer ID")?;
                output(
                    json!({"kind":"trust-status","registered":sources[0].store.peers()?.1.contains_key(peer)}),
                )?;
            }
            Some("sdk-arm") => {
                sources[0]
                    .sdk_gate
                    .as_ref()
                    .context("SDK gate disabled")?
                    .0
                    .arm();
                output(json!({"kind":"sdk-armed"}))?;
            }
            Some("sdk-release") => {
                sources[0]
                    .sdk_gate
                    .as_ref()
                    .context("SDK gate disabled")?
                    .0
                    .release();
                output(json!({"kind":"sdk-released"}))?;
            }
            Some("local-action") => {
                let generation = sources[0]
                    .gateway
                    .list()
                    .await
                    .into_iter()
                    .find(|service| service.id == "s")
                    .context("SDK missing")?
                    .generation;
                sources[0].gateway.action("s", generation, json!({"version":"v0.9","action":{"surfaceId":"main/nested","name":"increment","context":{"input":{"draft":"local","tag":"local"}}}})).await?;
                output(json!({"kind":"local-enqueued"}))?;
            }
            Some("fault") => {
                sources[0]
                    .faults
                    .as_ref()
                    .context("protocol faults not enabled")?
                    .set(command["kind"].as_str().context("fault kind")?)?;
                output(json!({"kind":"fault-configured"}))?;
            }
            Some("load") if load && load_job.is_none() => {
                let inputs: Vec<_> = sources
                    .iter()
                    .flat_map(|source| source.load.iter().cloned())
                    .collect();
                load_job = Some(tokio::spawn(async move {
                    let start = tokio::time::Instant::now();
                    for sequence in 1u64..=10000 {
                        tokio::time::sleep_until(
                            start + Duration::from_millis((sequence - 1) * 60),
                        )
                        .await;
                        inputs[(sequence as usize - 1) % inputs.len()]
                            .send(sequence)
                            .await
                            .context("load SDK stopped")?;
                    }
                    // Keep the observation window at ten full minutes.
                    tokio::time::sleep_until(start + Duration::from_secs(600)).await;
                    output(json!({"kind":"load-queued","updates":10000}))?;
                    Ok(())
                }));
                output(json!({"kind":"load-started"}))?;
            }
            Some("invite") => {
                let index = command["gateway"].as_u64().context("gateway index")? as usize;
                let source = sources.get(index).context("gateway index")?;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs();
                let invitation = format!(
                    "omochi://invite/{}",
                    serde_json::to_vec(&json!({
                        "version":4,"address":{"id":source.id,"addrs":[{"Relay":source.relay}]},
                        "secret":source.store.invite(now, 180)?,"expires":now+180,
                    }))?
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
                );
                output(json!({"kind":"invitation","gateway":index,"invitation":invitation}))?;
            }
            Some("forget") => {
                let index = command["gateway"].as_u64().context("gateway index")? as usize;
                let peer = command["peer"]
                    .as_str()
                    .context("public peer ID")?
                    .parse()?;
                if operations.len() >= 8 {
                    bail!("fixture operation capacity")
                }
                let store = sources.get(index).context("gateway index")?.store.clone();
                operations.push(tokio::spawn(async move {
                    let success = omochi_transport::live::forget(store, peer, Duration::from_secs(5)).await.is_ok();
                    let _ = output(json!({"kind":if success { "forgotten" } else { "forget-failed" },"gateway":index}));
                }));
            }
            Some("stop-relay") if server.is_some() => {
                server.take().unwrap().shutdown().await?;
                output(json!({"kind":"relay-stopped"}))?;
            }
            Some("restart-relay") if !public && server.is_none() => {
                server = Some(
                    relay(
                        cert.as_ref().unwrap(),
                        key.as_ref().unwrap(),
                        relay_port.unwrap(),
                    )
                    .await?,
                );
                output(json!({"kind":"relay-restarted"}))?;
            }
            Some("stop") => break,
            _ => bail!("unsupported fixture operation"),
        }
    }
    if let Some(job) = load_job {
        job.abort();
        let _ = job.await;
    }
    for source in sources {
        if let Some(faults) = &source.faults {
            faults.stop();
        }
        source.gateway.shutdown();
        source.listener.await?;
        source.discovery.await??;
        for app in source.apps {
            app.abort();
            let _ = app.await;
        }
        if let Some((gate, job)) = source.sdk_gate {
            gate.stop();
            let _ = job.await;
        }
    }
    for job in operations {
        job.abort();
        let _ = job.await;
    }
    if let Some(server) = server {
        server.shutdown().await?;
    }
    Ok(())
}
