//! Isolated real SDK, gateway and hub with a controlled TLS relay. Never a CLI mode.
use anyhow::{Context, Result, bail};
use image::ImageEncoder;
use iroh::{Endpoint, RelayMode, endpoint::presets};
use iroh_relay::{
    server::{CertConfig, RelayConfig, Server, ServerConfig, TlsConfig},
    tls::CaTlsConfig,
};
use omochi_gateway::Gateway;
use omochi_hub::{Hub, dashboards::DashboardStore};
use omochi_transport::{
    SecretKey,
    store::{Role, Store},
};
use omochi_ui::{
    App, ButtonOptions, ImageOptions, TextFieldOptions, UnixSocketServerOptions,
    UnixSocketServerTransport,
};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::mpsc;
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
    let key = rustls_pemfile::private_key(&mut io::BufReader::new(std::fs::File::open(key)?))?
        .context("Missing key")?;
    let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(certificates(cert)?, key)?;
    let mut relay = RelayConfig::new((std::net::Ipv4Addr::LOCALHOST, 0));
    relay.tls = Some(TlsConfig::new(
        (std::net::Ipv4Addr::LOCALHOST, port),
        CertConfig::Manual { server_config: tls },
    ));
    let mut config = ServerConfig::default();
    config.relay = Some(relay);
    Ok(Server::spawn(config).await?)
}
fn key() -> SecretKey {
    let mut key = [0; 32];
    key[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    key[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    SecretKey::from_bytes(&key)
}
#[derive(Default)]
struct State {
    count: u64,
    draft: String,
}
async fn run() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let argument = |name: &str| -> Result<String> {
        Ok(args
            .iter()
            .position(|v| v == name)
            .and_then(|i| args.get(i + 1))
            .context("Missing fixture argument")?
            .clone())
    };
    let cert = PathBuf::from(argument("--cert")?);
    let tls_key = PathBuf::from(argument("--key")?);
    let ca = PathBuf::from(argument("--ca")?);
    let web_url = argument("--web-url")?;
    let mut server = Some(relay(&cert, &tls_key, 0).await?);
    let port = server.as_ref().unwrap().https_addr().unwrap().port();
    let relay_url = format!("https://127.0.0.1:{port}/");
    let root = tempfile::Builder::new()
        .prefix("omochi-sharing-")
        .tempdir()?;
    let sockets = tempfile::Builder::new().prefix("osh-").tempdir_in("/tmp")?;
    let hub = Hub::new();
    let boards =
        DashboardStore::open(root.path().join("boards")).map_err(|e| anyhow::anyhow!(e.1))?;
    let mut gateways = vec![];
    let mut jobs = vec![];
    let mut sdk_jobs = vec![];
    let mut attachments = vec![];
    let mut ids = vec![];
    for index in 0..2 {
        let runtime = sockets.path().join(index.to_string());
        std::fs::create_dir_all(&runtime)?;
        let mut app = App::new(State::default());
        let mut png = vec![];
        image::codecs::png::PngEncoder::new(&mut png).write_image(
            &[255, 0, 0, 255, 255, 0, 0, 255],
            2,
            1,
            image::ExtendedColorType::Rgba8,
        )?;
        let image = app.resources.bytes(&png, "image/png")?;
        let action = app.action::<Value, _, _>("increment", |input, context| async move {
            context
                .update(move |state| {
                    state.count += 1;
                    state.draft = input["draft"].as_str().unwrap_or("").to_owned();
                })
                .await?;
            Ok(())
        })?;
        app.surface("main", move |ui, state| {
            ui.text(format!("App {index} count: {}", state.count))?;
            ui.text(format!("Received: {}", state.draft))?;
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
            ui.image(
                image.clone(),
                ImageOptions {
                    description: Some(json!(format!("Red PNG {index}"))),
                    ..Default::default()
                },
            )?;
            Ok(())
        })?;
        app.surface("private", |ui, _| {
            ui.text("PRIVATE CANARY: never share this Surface")?;
            Ok(())
        })?;
        let transport = UnixSocketServerTransport::bind(
            UnixSocketServerOptions::new("app")
                .with_name(format!("Fixture app {index}"))
                .with_runtime_dir(&runtime),
        )
        .await?;
        sdk_jobs.push(tokio::task::spawn_local(app.start_with(transport)));
        let gateway = Gateway::new();
        let node = key().public().to_string();
        let socket = runtime.join("gateway.sock");
        jobs.push(gateway.listen_unix(
            socket.clone(),
            node.clone(),
            format!("Fixture gateway {index}"),
        )?);
        jobs.push(gateway.discover(runtime));
        attachments.push(hub.attach_unix(socket));
        gateways.push(gateway);
        ids.push(node);
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if hub.services().await.len() == 2
                && hub
                    .subscribe()
                    .await
                    .snapshot
                    .iter()
                    .filter(|v| v.get("createSurface").is_some())
                    .count()
                    == 4
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    let panel = |index: usize| json!({"id":format!("p{index}"),"title":format!("App {index}"),"reference":{"nodeId":ids[index],"serviceId":"app","surfaceId":"main"},"x":index*6,"y":0,"w":6,"h":5});
    let board = boards
        .mutate(
            None,
            json!({"name":"Shared fixture board","panels":[panel(0),panel(1)]}),
            false,
        )
        .map_err(|e| anyhow::anyhow!(e.1))?
        .unwrap();
    let store = Store::open(root.path().join("identity"))?;
    let endpoint = Endpoint::builder(presets::Minimal)
        .secret_key(store.identity(Role::Hub)?)
        .relay_mode(RelayMode::custom([relay_url.parse()?]))
        .ca_tls_config(CaTlsConfig::custom_roots(certificates(&ca)?))
        .bind()
        .await?;
    tokio::time::timeout(Duration::from_secs(30), endpoint.online()).await?;
    let remote =
        omochi_hub::dashboard_remote::listen(hub.clone(), boards.clone(), endpoint.clone())?;
    hub.set_dashboard_host(endpoint, web_url)?;
    let (http_addr, http) = omochi_hub::http::serve(
        omochi_hub::http::HttpState {
            hub: hub.clone(),
            dashboards: boards.clone(),
            allow_origins: vec![],
            assets: Some(PathBuf::from("packages/hub-ui/dist")),
            presets: vec![],
        },
        "127.0.0.1:0".parse()?,
    )
    .await?;
    output(
        json!({"kind":"ready","ownerUrl":format!("http://{http_addr}/"),"boardId":board.id,"nodes":ids}),
    )?;
    let (tx, mut rx) = mpsc::channel(8);
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines().map_while(Result::ok) {
            if tx.blocking_send(line).is_err() {
                break;
            }
        }
    });
    while let Some(line) = rx.recv().await {
        let command: Value = serde_json::from_str(&line)?;
        match command["op"].as_str() {
            Some("relay-stop") => {
                if let Some(server) = server.take() {
                    server.shutdown().await?;
                }
                output(json!({"kind":"relay-stopped"}))?;
            }
            Some("relay-start") => {
                server = Some(relay(&cert, &tls_key, port).await?);
                output(json!({"kind":"relay-started"}))?;
            }
            Some("stop") => break,
            _ => bail!("Unknown fixture command"),
        }
    }
    hub.shutdown();
    for gateway in &gateways {
        gateway.shutdown();
    }
    remote.await??;
    http.await??;
    for job in jobs {
        job.await??;
    }
    for job in attachments {
        job.await?;
    }
    for job in sdk_jobs {
        job.abort();
        let _ = job.await;
    }
    if let Some(server) = server {
        server.shutdown().await?;
    }
    Ok(())
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tokio::task::LocalSet::new().run_until(run()).await
}
