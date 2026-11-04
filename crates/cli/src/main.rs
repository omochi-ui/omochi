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

const DEFAULT_WEB_URL: &str = "https://app.omochi.fyi/";

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
        /// HTTPS dashboard base used only when printing a new invitation.
        #[arg(long, requires = "remote", default_value = DEFAULT_WEB_URL, value_parser = validate_web_url)]
        web_url: String,
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
        /// Accept shared dashboard clients over iroh without exposing local HTTP.
        #[arg(long, conflicts_with = "offline")]
        remote: bool,
        /// HTTPS base of the separately hosted dashboard client.
        #[arg(long, requires = "remote", default_value = DEFAULT_WEB_URL, value_parser = validate_web_url)]
        web_url: String,
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
fn validate_web_url(text: &str) -> std::result::Result<String, String> {
    let invalid = || {
        "--web-url requires an absolute HTTPS URL without credentials, query or fragment (maximum 2048 characters)".to_owned()
    };
    let parsed = url::Url::parse(text).map_err(|_| invalid())?;
    if text.chars().count() > 2048
        || text.trim() != text
        || !text.to_ascii_lowercase().starts_with("https://")
        || text
            .split('/')
            .nth(2)
            .is_some_and(|host| host.contains('@'))
        || parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(text.to_owned())
}
fn web_invitation(base: &str, invitation: &str) -> Result<String> {
    let payload = invitation
        .strip_prefix("omochi://invite/")
        .context("invalid invitation link")?;
    Ok(format!("{base}#g:{payload}"))
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
struct ShutdownSignal {
    term: tokio::signal::unix::Signal,
    interrupt: tokio::signal::unix::Signal,
}
impl ShutdownSignal {
    // Register before advertising readiness; constructing an async future alone
    // does not install handlers and leaves a window for the default SIGTERM exit.
    fn new() -> Result<Self> {
        use tokio::signal::unix::{SignalKind, signal};
        Ok(Self {
            term: signal(SignalKind::terminate())?,
            interrupt: signal(SignalKind::interrupt())?,
        })
    }
    async fn wait(&mut self) {
        tokio::select! { _ = self.interrupt.recv() => {}, _ = self.term.recv() => {} }
    }
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
            web_url,
            offline,
            relay,
            name,
        } => {
            let mut shutdown = ShutdownSignal::new()?;
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
                    {
                        let base = &web_url;
                        tokio::io::stdout()
                            .write_all(
                                format!("{}\n", web_invitation(base, &invitation)?).as_bytes(),
                            )
                            .await?;
                        let url = web_invitation(base, &invitation)?;
                        match qrcode::QrCode::new(url.as_bytes()) {
                            Ok(code) => eprintln!(
                                "{}\nInvitation expires in 180 seconds.",
                                code.render::<qrcode::render::unicode::Dense1x2>().build()
                            ),
                            Err(_) => eprintln!(
                                "[omochi-ui] invitation is too large for a QR code; use the URL"
                            ),
                        }
                    }
                }
                Some(gateway.listen_remote_async(endpoint, store, name).await?)
            } else {
                None
            };
            eprintln!(
                "[omochi-ui] gateway {node} at {}",
                runtime.join("gateway.sock").display()
            );
            shutdown.wait().await;
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
            remote,
            web_url,
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
                    remote,
                    web_url,
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
                    remote: false,
                    web_url: DEFAULT_WEB_URL.to_owned(),
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
    remote: bool,
    web_url: String,
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
    let mut shutdown = ShutdownSignal::new()?;
    let HubOptions {
        host,
        port,
        allow_origin,
        no_auto_gateway,
        assets,
        remote: remote_dashboard,
        web_url,
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
    let dashboards =
        omochi_hub::dashboards::DashboardStore::open(root).map_err(|e| anyhow::anyhow!(e.1))?;
    let remote_boards = dashboards.clone();
    let remote_hub = hub.clone();
    let remote_store = store.clone();
    let remote = tokio::spawn(async move {
        let stop = remote_hub.cancellation_token();
        loop {
            if remote_dashboard
                || remote_store
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
        remote_hub.configure_pairing(endpoint.clone(), remote_store.clone());
        let listener = if remote_dashboard {
            let listener = omochi_hub::dashboard_remote::listen(
                remote_hub.clone(),
                remote_boards,
                endpoint.clone(),
            )?;
            remote_hub.set_dashboard_host(endpoint.clone(), web_url)?;
            Some(listener)
        } else {
            None
        };
        remote_hub
            .attach_registered(endpoint, remote_store)
            .0
            .await?;
        if let Some(listener) = listener {
            listener.await??;
        }
        Ok(())
    });
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
    shutdown.wait().await;
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

#[cfg(test)]
mod shutdown_tests {
    use super::*;

    #[test]
    fn hosted_invitation_requires_valid_https_base_and_remote_mode() {
        for base in [
            "http://example.com/",
            "https://u:p@example.com/",
            "https://example.com/?",
            "https://example.com/#",
            "relative",
            " https://example.com/",
        ] {
            assert!(validate_web_url(base).is_err());
        }
        assert!(validate_web_url(&format!("https://example.com/{}", "x".repeat(2048))).is_err());
        assert!(
            Cli::try_parse_from(["omochi-ui", "gateway", "--web-url", "https://example.com/"])
                .is_err()
        );
        let base = "https://example.com/ui/";
        assert_eq!(validate_web_url(base).unwrap(), base);
        assert_eq!(
            web_invitation(base, "omochi://invite/YQ").unwrap(),
            "https://example.com/ui/#g:YQ"
        );
        assert!(
            Cli::try_parse_from([
                "omochi-ui",
                "gateway",
                "--remote",
                "--pair",
                "--web-url",
                base
            ])
            .is_ok()
        );
    }

    #[test]
    fn downstream_dashboard_listener_is_explicit_and_keeps_offline_ws_separate() {
        assert!(Cli::try_parse_from(["omochi-ui", "hub", "--remote"]).is_ok());
        assert!(Cli::try_parse_from(["omochi-ui", "hub", "--remote", "--offline"]).is_err());
        assert!(
            Cli::try_parse_from(["omochi-ui", "hub", "--web-url", "https://ui.example/"]).is_err()
        );
        assert!(
            Cli::try_parse_from([
                "omochi-ui",
                "hub",
                "--remote",
                "--web-url",
                "https://ui.example/",
                "--offline"
            ])
            .is_err()
        );
        assert!(Cli::try_parse_from(["omochi-ui", "hub", "--offline"]).is_ok());
        assert!(
            Cli::try_parse_from([
                "omochi-ui",
                "hub",
                "--remote",
                "--web-url",
                "https://ui.example/dashboard/"
            ])
            .is_ok()
        );
    }

    #[test]
    fn hosted_dashboard_defaults_allow_local_mode_and_custom_urls() {
        for command in ["hub", "gateway"] {
            for remote in [false, true] {
                let mut args = vec!["omochi-ui", command];
                if remote {
                    args.push("--remote");
                }
                let cli = Cli::try_parse_from(args).unwrap();
                match cli.command {
                    Command::Hub {
                        web_url,
                        remote: actual_remote,
                        ..
                    }
                    | Command::Gateway {
                        web_url,
                        remote: actual_remote,
                        ..
                    } => {
                        assert_eq!(web_url, "https://app.omochi.fyi/");
                        assert_eq!(actual_remote, remote);
                    }
                    _ => unreachable!(),
                }
            }
            let cli = Cli::try_parse_from([
                "omochi-ui",
                command,
                "--remote",
                "--web-url",
                "https://ui.example/dashboard/",
            ])
            .unwrap();
            match cli.command {
                Command::Hub { web_url, .. } | Command::Gateway { web_url, .. } => {
                    assert_eq!(web_url, "https://ui.example/dashboard/");
                }
                _ => unreachable!(),
            }
        }
    }

    #[tokio::test]
    async fn shutdown_is_registered_before_wait_is_polled() {
        let mut shutdown = ShutdownSignal::new().unwrap();
        // Deliver while no wait future exists, just as a caller may signal as
        // soon as it observes the startup announcement.
        assert_eq!(unsafe { libc::raise(libc::SIGTERM) }, 0);
        tokio::time::timeout(Duration::from_secs(3), shutdown.wait())
            .await
            .expect("signal delivered before wait was lost");
    }
}
