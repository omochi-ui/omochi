mod catalog;
mod composition;
mod dev;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use omochi_transport::{
    pairing::Invite,
    store::{Role, Store},
};
use std::{
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Parser)]
#[command(
    name = "omochi-ui",
    version,
    about = "Attach browser UIs to local and paired applications"
)]
struct Cli {
    #[arg(long, global = true, env = "OMOCHI_CONFIG_DIR")]
    config_dir: Option<PathBuf>,
    #[arg(long, global = true, env = "OMOCHI_RUNTIME_ROOT")]
    runtime_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Show registered gateway connection diagnostics from a running local hub.
    Status {
        #[arg(long, default_value = "127.0.0.1")]
        host: std::net::IpAddr,
        #[arg(long, default_value_t = 5555)]
        port: u16,
    },
    /// Bridge one application's JSONL stdio to the browser preview.
    Dev {
        #[arg(long, default_value = "127.0.0.1")]
        host: std::net::IpAddr,
        #[arg(long, default_value_t = 5555)]
        port: u16,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Generate typed catalog bindings for SDK applications.
    Generate {
        #[command(subcommand)]
        command: GenerateCommand,
    },
    /// Run local discovery and gateway routing; remote networking is opt-in.
    Gateway {
        #[arg(long)]
        remote: bool,
        #[arg(long, requires = "remote")]
        pair: bool,
        #[arg(long)]
        offline: bool,
        #[arg(long)]
        relay: Vec<String>,
        #[arg(long, default_value = "gateway")]
        name: String,
    },
    /// Serve the dashboard on loopback, starting a local gateway if needed.
    Hub {
        #[arg(long, default_value = "127.0.0.1")]
        host: std::net::IpAddr,
        #[arg(long, default_value_t = 5555)]
        port: u16,
        #[arg(long)]
        allow_origin: Vec<String>,
        #[arg(long)]
        no_auto_gateway: bool,
        #[arg(long)]
        assets: Option<PathBuf>,
        #[arg(long)]
        offline: bool,
        #[arg(long)]
        relay: Vec<String>,
    },
    /// Start services and a dashboard from an omochi.toml composition.
    Up {
        #[arg(default_value = "omochi.toml")]
        file: PathBuf,
        #[arg(long, default_value = "127.0.0.1")]
        host: std::net::IpAddr,
        #[arg(long, default_value_t = 5555)]
        port: u16,
        #[arg(long)]
        allow_origin: Vec<String>,
        #[arg(long)]
        open: bool,
    },
    /// Import an invitation on this hub PC (reads stdin when omitted).
    Pair {
        invitation: Option<String>,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "hub")]
        hub_name: String,
        #[arg(long)]
        offline: bool,
        #[arg(long)]
        relay: Vec<String>,
    },
    /// Show this installation's persistent endpoint identity.
    Id {
        #[arg(long)]
        hub: bool,
    },
    /// Inspect registered peer identities.
    Trust {
        #[command(subcommand)]
        command: TrustCommand,
    },
}
#[derive(Subcommand)]
enum GenerateCommand {
    Catalog(catalog::Options),
}
#[derive(Subcommand)]
enum TrustCommand {
    List,
    /// Revoke a peer and wait for every live process to tear down its connections.
    Forget {
        peer: String,
    },
}
fn config_root(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .context("set XDG_CONFIG_HOME or --config-dir")?;
    Ok(base.join("omochi/rust-v1"))
}
fn private_directory(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    let meta = std::fs::symlink_metadata(path)?;
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    if !meta.is_dir() || meta.mode() & 0o077 != 0 || meta.uid() != unsafe { getuid() } {
        bail!(
            "runtime directory must be owned and private: {}",
            path.display()
        )
    }
    Ok(())
}
async fn shutdown_signal() -> Result<()> {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! { result = tokio::signal::ctrl_c() => result?, _ = term.recv() => {} }
    Ok(())
}
async fn probe(path: &Path) -> Result<()> {
    use omochi_protocol::{OpenStream, VERSION, read_frame, session::Control, write_frame};
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut socket = tokio::net::UnixStream::connect(path).await?;
        write_frame(&mut socket, &OpenStream::Control { protocol: VERSION }).await?;
        match read_frame::<_, Control>(&mut socket).await? {
            Some(Control::Hello {
                protocol: VERSION, ..
            }) => Ok(()),
            _ => bail!("incompatible gateway; stop the old gateway before starting this version"),
        }
    })
    .await
    .context("gateway did not respond; stop the old gateway before retrying")?
}
#[tokio::main]
async fn main() {
    if let Err(error) = run(Cli::parse()).await {
        eprintln!("[omochi-ui] {error:#}");
        std::process::exit(1);
    }
}
async fn run(cli: Cli) -> Result<()> {
    if let Command::Generate {
        command: GenerateCommand::Catalog(options),
    } = cli.command
    {
        return catalog::generate(options);
    }
    if let Command::Status { host, port } = cli.command {
        // Workspace feature unification may enable reqwest's provider-free TLS
        // backend via utils, even though this endpoint is plain loopback HTTP.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let address = std::net::SocketAddr::new(host, port);
        let status: serde_json::Value = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()?
            .get(format!("http://{address}/api/connections"))
            .send()
            .await
            .context("cannot reach the hub; start omochi-ui hub first")?
            .error_for_status()?
            .json()
            .await?;
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }
    let root = config_root(cli.config_dir)?;
    let store = Store::open(&root)?;
    let runtime = cli
        .runtime_dir
        .unwrap_or_else(omochi_ui::omochi_runtime_root);
    match cli.command {
        Command::Dev {
            host,
            port,
            command,
        } => {
            let code = dev::run(command, std::net::SocketAddr::new(host, port), root).await?;
            if code != 0 {
                std::process::exit(code);
            }
        }
        Command::Generate { .. } | Command::Status { .. } => unreachable!(),
        Command::Id { hub } => println!(
            "{}",
            store
                .identity(if hub { Role::Hub } else { Role::Gateway })?
                .public()
        ),
        Command::Trust {
            command: TrustCommand::List,
        } => {
            for (id, peer) in store.peers()?.1 {
                println!("{id}\t{:?}\t{}", peer.role, peer.name);
            }
        }
        Command::Trust {
            command: TrustCommand::Forget { peer },
        } => {
            let id = match peer.parse::<omochi_transport::EndpointId>() {
                Ok(id) => id,
                Err(_) => {
                    let matches: Vec<_> = store
                        .peers()?
                        .1
                        .into_iter()
                        .filter(|(_, p)| p.name == peer)
                        .collect();
                    if matches.len() != 1 {
                        anyhow::bail!("peer name must identify exactly one registered peer");
                    }
                    matches[0].0.parse()?
                }
            };
            omochi_transport::live::forget(store, id, Duration::from_secs(10)).await?;
            println!("forgot {id}");
        }
        Command::Pair {
            invitation,
            name,
            hub_name,
            offline,
            relay,
        } => {
            let text = if let Some(text) = invitation {
                text
            } else {
                eprintln!("[omochi-ui] paste the invitation and end input (Ctrl+D):");
                let mut bytes = Vec::new();
                tokio::io::stdin()
                    .take(16385)
                    .read_to_end(&mut bytes)
                    .await?;
                String::from_utf8(bytes).context("invitation must be UTF-8")?
            };
            let invite = Invite::decode(&text)?;
            let relays = if relay.is_empty() {
                None
            } else {
                Some(
                    relay
                        .into_iter()
                        .map(|s| s.parse())
                        .collect::<std::result::Result<Vec<_>, _>>()?,
                )
            };
            let endpoint =
                omochi_transport::bind(store.identity(Role::Hub)?, offline, relays).await?;
            let result =
                omochi_transport::pairing::redeem(&endpoint, store, invite, &name, &hub_name).await;
            endpoint.close().await;
            result?;
            eprintln!("[omochi-ui] registered {name}");
        }
        Command::Gateway {
            remote,
            pair,
            offline,
            relay,
            name,
        } => {
            private_directory(&runtime)?;
            let gateway = omochi_gateway::Gateway::new();
            let node = store.identity(Role::Gateway)?.public().to_string();
            let local =
                gateway.listen_unix(runtime.join("gateway.sock"), node.clone(), name.clone())?;
            let discovery = gateway.discover(runtime.clone());
            let remote_task = if remote {
                let relays = if relay.is_empty() {
                    None
                } else {
                    Some(
                        relay
                            .into_iter()
                            .map(|s| s.parse())
                            .collect::<std::result::Result<Vec<_>, _>>()?,
                    )
                };
                let endpoint =
                    omochi_transport::bind(store.identity(Role::Gateway)?, offline, relays).await?;
                if pair {
                    let invitation = Invite::issue(&store, &endpoint)?.encode()?;
                    tokio::io::stdout()
                        .write_all(format!("{invitation}\n").as_bytes())
                        .await?;
                }
                Some(gateway.listen_remote(endpoint, store, name)?)
            } else {
                None
            };
            eprintln!(
                "[omochi-ui] gateway {node} at {}",
                runtime.join("gateway.sock").display()
            );
            shutdown_signal().await?;
            gateway.shutdown();
            local.await??;
            discovery.await??;
            if let Some(task) = remote_task {
                task.await?;
            }
        }
        Command::Hub {
            host,
            port,
            allow_origin,
            no_auto_gateway,
            assets,
            offline,
            relay,
        } => {
            serve_hub(
                store,
                root,
                runtime,
                HubOptions {
                    host,
                    port,
                    allow_origin,
                    no_auto_gateway,
                    assets,
                    offline,
                    relay,
                    presets: vec![],
                    open: false,
                },
            )
            .await?;
        }
        Command::Up {
            file,
            host,
            port,
            allow_origin,
            open,
        } => {
            let node = store.identity(Role::Gateway)?.public().to_string();
            let composition = composition::load(&file, &node)?;
            private_directory(&runtime)?;
            let runtime = runtime.canonicalize()?;
            let mut children = composition::Supervisor::start(&composition.services, &runtime)?;
            let result = serve_hub(
                store,
                root,
                runtime,
                HubOptions {
                    host,
                    port,
                    allow_origin,
                    no_auto_gateway: false,
                    assets: None,
                    offline: false,
                    relay: vec![],
                    presets: vec![composition.preset],
                    open,
                },
            )
            .await;
            children.close().await;
            result?;
        }
    }
    Ok(())
}

struct HubOptions {
    host: std::net::IpAddr,
    port: u16,
    allow_origin: Vec<String>,
    no_auto_gateway: bool,
    assets: Option<PathBuf>,
    offline: bool,
    relay: Vec<String>,
    presets: Vec<serde_json::Value>,
    open: bool,
}
async fn serve_hub(
    store: Store,
    root: PathBuf,
    runtime: PathBuf,
    options: HubOptions,
) -> Result<()> {
    let HubOptions {
        host,
        port,
        allow_origin,
        no_auto_gateway,
        assets,
        offline,
        relay,
        presets,
        open,
    } = options;
    private_directory(&runtime)?;
    let socket = runtime.join("gateway.sock");
    let owned = if socket.exists() {
        probe(&socket).await?;
        None
    } else if !no_auto_gateway {
        let gateway = omochi_gateway::Gateway::new();
        let listener = gateway.listen_unix(
            socket.clone(),
            store.identity(Role::Gateway)?.public().to_string(),
            "local".into(),
        )?;
        let discovery = gateway.discover(runtime);
        Some((gateway, listener, discovery))
    } else {
        None
    };
    let hub = omochi_hub::Hub::new();
    let attached = hub.attach_unix(socket);
    let remote_hub = hub.clone();
    let remote_store = store.clone();
    let remote = tokio::spawn(async move {
        let stop = remote_hub.cancellation_token();
        loop {
            if remote_store
                .peers()?
                .1
                .values()
                .any(|p| p.role == Role::Gateway)
            {
                break;
            }
            tokio::select! { _ = stop.cancelled() => return Ok::<(), anyhow::Error>(()), _ = tokio::time::sleep(Duration::from_millis(250)) => {} }
        }
        let relays = if relay.is_empty() {
            None
        } else {
            Some(
                relay
                    .into_iter()
                    .map(|s| s.parse())
                    .collect::<std::result::Result<Vec<_>, _>>()?,
            )
        };
        let endpoint =
            omochi_transport::bind(remote_store.identity(Role::Hub)?, offline, relays).await?;
        remote_hub
            .attach_registered(endpoint, remote_store)
            .0
            .await?;
        Ok(())
    });
    let dashboards =
        omochi_hub::dashboards::DashboardStore::open(root).map_err(|e| anyhow::anyhow!(e.1))?;
    let selected_preset = presets
        .first()
        .and_then(|preset| preset["id"].as_str())
        .map(str::to_owned);
    let (address, http) = omochi_hub::http::serve(
        omochi_hub::http::HttpState {
            hub: hub.clone(),
            dashboards,
            allow_origins: allow_origin,
            assets,
            presets,
        },
        std::net::SocketAddr::new(host, port),
    )
    .await?;
    let mut dashboard_url = url::Url::parse(&format!("http://{address}/"))?;
    if let Some(id) = selected_preset {
        dashboard_url.query_pairs_mut().append_pair("preset", &id);
    }
    eprintln!("[omochi-ui] dashboard {dashboard_url}");
    if open {
        let browser_address = if address.ip().is_unspecified() {
            std::net::SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), address.port())
        } else {
            address
        };
        dashboard_url.set_host(Some(&browser_address.ip().to_string()))?;
        composition::open_browser(dashboard_url.as_str());
    }
    shutdown_signal().await?;
    hub.shutdown();
    attached.await?;
    remote.await??;
    http.await??;
    if let Some((gateway, listener, discovery)) = owned {
        gateway.shutdown();
        listener.await??;
        discovery.await??;
    }
    Ok(())
}
