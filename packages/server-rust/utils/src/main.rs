use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use omochi_ui::UnixSocketServerOptions;
use omochi_utils::{publisher::Publisher, surface::TextSurface};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(
    name = "omochi-utils",
    version,
    about = "Publish utilities through the local Omochi gateway"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Args)]
struct Common {
    #[arg(long, env = "OMOCHI_SERVICE")]
    service_id: Option<String>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long, env = "OMOCHI_RUNTIME_ROOT")]
    runtime_dir: Option<PathBuf>,
}
#[derive(Subcommand)]
enum Command {
    Mcp {
        #[command(flatten)]
        common: Common,
    },
    Http {
        #[command(flatten)]
        common: Common,
        #[arg(long,default_value="10s",value_parser=duration)]
        interval: std::time::Duration,
        #[arg(long,default_value="10s",value_parser=duration)]
        timeout: std::time::Duration,
        #[arg(long,default_value="200-299",value_parser=status_range)]
        expect_status: (u16, u16),
        #[arg(long)]
        expect_body: Option<String>,
        #[arg(value_parser=http_url)]
        url: reqwest::Url,
    },
    Watch {
        #[command(flatten)]
        common: Common,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long,default_value="2s",value_parser=duration)]
        interval: std::time::Duration,
        #[arg(long,value_parser=duration)]
        timeout: Option<std::time::Duration>,
        #[arg(long)]
        diff: bool,
        #[arg(long)]
        on_change: bool,
        #[arg(long,value_parser=history_limit)]
        history: Option<usize>,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    Process {
        #[command(flatten)]
        common: Common,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "never")]
        restart: omochi_utils::process::Restart,
        #[arg(long,default_value="1s",value_parser=duration)]
        restart_delay: std::time::Duration,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    Button {
        #[command(flatten)]
        common: Common,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    Stream {
        #[command(flatten)]
        common: Common,
    },
    Tail {
        #[command(flatten)]
        common: Common,
        #[arg(long,default_value_t=100,value_parser=positive)]
        lines: usize,
        file: PathBuf,
    },
}
fn http_url(value: &str) -> std::result::Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|e| e.to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("URL must use http or https".into());
    }
    Ok(url)
}
fn status_range(value: &str) -> std::result::Result<(u16, u16), String> {
    let (min, max) = value.split_once('-').unwrap_or((value, value));
    let min = min.parse::<u16>().map_err(|_| "invalid status range")?;
    let max = max.parse::<u16>().map_err(|_| "invalid status range")?;
    if min < 100 || max > 599 || min > max {
        return Err("status range must be ordered between 100 and 599".into());
    }
    Ok((min, max))
}
fn history_limit(value: &str) -> std::result::Result<usize, String> {
    positive(value).and_then(|n| {
        if n <= 100 {
            Ok(n)
        } else {
            Err("history must be between 1 and 100".into())
        }
    })
}
fn positive(value: &str) -> std::result::Result<usize, String> {
    value
        .parse()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| "lines must be a positive integer".into())
}
fn duration(value: &str) -> std::result::Result<std::time::Duration, String> {
    let (number, multiplier) = if let Some(s) = value.strip_suffix("ms") {
        (s, 1.0)
    } else if let Some(s) = value.strip_suffix('s') {
        (s, 1000.0)
    } else if let Some(s) = value.strip_suffix('m') {
        (s, 60000.0)
    } else {
        return Err("duration needs ms, s or m suffix".into());
    };
    let millis = number.parse::<f64>().map_err(|_| "invalid duration")? * multiplier;
    if !millis.is_finite() || !(1.0..=2147483647.0).contains(&millis) {
        return Err("duration must be between 1 and 2147483647 ms".into());
    }
    Ok(std::time::Duration::from_millis(millis.round() as u64))
}
#[tokio::main]
async fn main() {
    if let Err(error) = run(Cli::parse()).await {
        eprintln!("[omochi-utils] {error:#}");
        std::process::exit(1);
    }
}
async fn run(cli: Cli) -> Result<()> {
    let (common, fallback) = match &cli.command {
        Command::Mcp { common } => (common, "MCP".into()),
        Command::Http { common, url, .. } => (
            common,
            format!(
                "{}{}",
                url.host_str().unwrap_or("HTTP"),
                url.port()
                    .map(|port| format!(":{port}"))
                    .unwrap_or_default()
            ),
        ),
        Command::Watch { common, .. } => (common, "Watch".into()),
        Command::Process { common, .. } => (common, "Process".into()),
        Command::Button { common, .. } => (common, "Command".into()),
        Command::Stream { common } => (common, "Stream".into()),
        Command::Tail { common, file, .. } => (
            common,
            file.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        ),
    };
    let name = common.name.clone().unwrap_or(fallback);
    let id = common
        .service_id
        .clone()
        .unwrap_or_else(|| format!("utils-{}", uuid::Uuid::new_v4()));
    let mut options = UnixSocketServerOptions::new(&id).with_name(&name);
    options.runtime_dir = common.runtime_dir.clone();
    let (publisher, owner) = Publisher::open(options).await?;
    let stop = CancellationToken::new();
    let signal_stop = stop.clone();
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let signals = tokio::spawn(async move {
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
        signal_stop.cancel();
    });
    eprintln!("Published service {id} ({name})");
    let surface = TextSurface::new(publisher.clone(), name.clone());
    let result = match cli.command {
        Command::Mcp { .. } => omochi_utils::mcp::run(publisher, stop).await,
        Command::Http {
            url,
            interval,
            timeout,
            expect_status,
            expect_body,
            ..
        } => {
            omochi_utils::http::run(
                publisher,
                name,
                omochi_utils::http::Options {
                    url,
                    interval,
                    timeout,
                    expect_status,
                    expect_body,
                },
                stop,
            )
            .await
        }
        Command::Watch {
            cwd,
            command,
            interval,
            timeout,
            diff,
            on_change,
            history,
            ..
        } => {
            omochi_utils::watch::run(
                publisher,
                name,
                omochi_utils::watch::WatchOptions {
                    command: omochi_utils::command::Options { argv: command, cwd },
                    interval,
                    timeout,
                    diff,
                    on_change,
                    history: history.unwrap_or(0),
                },
                stop,
            )
            .await
        }
        Command::Process {
            cwd,
            command,
            restart,
            restart_delay,
            ..
        } => {
            omochi_utils::process::run(
                publisher,
                name,
                omochi_utils::command::Options { argv: command, cwd },
                restart,
                restart_delay,
                stop,
            )
            .await
        }
        Command::Button { cwd, command, .. } => {
            omochi_utils::button::run(
                publisher,
                name,
                omochi_utils::command::Options { argv: command, cwd },
                stop,
            )
            .await
        }
        Command::Stream { .. } => {
            omochi_utils::stream::run(omochi_utils::input::Input::stdin()?, surface, stop).await
        }
        Command::Tail { file, lines, .. } => {
            omochi_utils::tail::run(file, lines, surface, stop).await
        }
    };
    signals.abort();
    let _ = signals.await;
    owner.close().await?;
    result
}
