use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use omochi_transport::{
    pairing::Invite,
    store::{Role, Store},
};
use omochi_ui::{
    App, ButtonOptions, ImageOptions, UnixSocketServerOptions, UnixSocketServerTransport,
};
use serde_json::json;
use std::{
    io::{self, BufRead, Write},
    time::Duration,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tokio::task::LocalSet::new().run_until(run()).await
}

async fn run() -> Result<()> {
    let parent = std::env::var_os("OMOCHI_BROWSER_SPIKE_TMPDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let root = tempfile::Builder::new().prefix("obh-").tempdir_in(parent)?;
    let runtime = root.path().join("run");
    let store = Store::open(root.path().join("state"))?;
    let mut app = App::new(0u64);
    // A 1×1 PNG registered by the real SDK, not served by HTTP.
    let image = app.resources.bytes(&STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==")?, "image/png")?;
    let increment =
        app.action::<serde_json::Value, _, _>("increment", |_, context| async move {
            context.update(|count| *count += 1).await?;
            Ok(())
        })?;
    app.surface("main", move |ui, count| {
        ui.text(format!("Browser gateway count: {count}"))?;
        ui.button(
            "Increment",
            increment.call(json!({}))?,
            ButtonOptions::default(),
        )?;
        ui.image(
            image.clone(),
            ImageOptions {
                description: Some("SDK resource pixel".into()),
                ..Default::default()
            },
        )?;
        Ok(())
    })?;
    let transport = UnixSocketServerTransport::bind(
        UnixSocketServerOptions::new("browser-spike")
            .with_name("Browser spike SDK")
            .with_runtime_dir(&runtime),
    )
    .await?;
    let app_task = tokio::task::spawn_local(app.start_with(transport));
    let gateway = omochi_gateway::Gateway::new();
    let discovery = gateway.discover(runtime);
    tokio::time::timeout(Duration::from_secs(10), async {
        while gateway.list().await.is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    let endpoint = omochi_transport::bind(store.identity(Role::Gateway)?, false, None).await?;
    tokio::time::timeout(Duration::from_secs(30), endpoint.online()).await?;
    let address = endpoint.addr();
    let relay = address
        .relay_urls()
        .next()
        .context("gateway has no home relay")?
        .to_string();
    let invite = Invite::issue(&store, &endpoint)?.encode()?;
    let listener =
        gateway.listen_remote(endpoint, store.clone(), "Browser spike gateway".into())?;
    println!(
        "{}",
        json!({"kind":"ready","gatewayId":address.id.to_string(),"relay":relay,"invitation":invite})
    );
    io::stdout().flush()?;
    // Harness controls lifecycle over stdin; secrets stay in its temporary directory.
    let (tx, mut commands) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    while let Some(line) = commands.recv().await {
        let value: serde_json::Value = serde_json::from_str(&line)?;
        match value["op"].as_str() {
            Some("forget") => {
                let id = value["peer"].as_str().context("peer")?.parse()?;
                omochi_transport::live::forget(store.clone(), id, Duration::from_secs(5)).await?;
                println!("{}", json!({"kind":"forgotten"}));
                io::stdout().flush()?;
            }
            Some("stop") => break,
            _ => anyhow::bail!("unsupported harness operation"),
        }
    }
    gateway.shutdown();
    listener.await?;
    discovery.await??;
    app_task.abort();
    let _ = app_task.await;
    Ok(())
}
