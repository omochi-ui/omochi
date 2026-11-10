use anyhow::{Context, Result};
use omochi_hub::{
    Hub,
    dashboards::DashboardStore,
    http::{HttpState, serve},
};
use omochi_process::{Command, Stdio};
use std::{net::SocketAddr, path::PathBuf, time::Duration};
pub async fn run(command: Vec<String>, address: SocketAddr, root: PathBuf) -> Result<i32> {
    let mut shutdown = super::ShutdownSignal::new()?;
    let mut child = Command::new(command.first().context("command required after --")?)
        .args(&command[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let group = child.group();
    let (hub, mut stream) =
        Hub::attach_stdio(child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let dashboards = DashboardStore::open(root).map_err(|e| anyhow::anyhow!(e.1))?;
    let server = serve(
        HttpState {
            hub: hub.clone(),
            dashboards,
            allow_origins: vec![],
            assets: None,
            presets: vec![],
        },
        address,
    )
    .await;
    let (address, http) = match server {
        Ok(server) => server,
        Err(error) => {
            hub.shutdown();
            group.signal(omochi_process::SIGKILL);
            let _ = child.wait().await;
            let _ = stream.await;
            return Err(error);
        }
    };
    let url = format!("ws://{address}/");
    let mut preview = url::Url::parse("http://127.0.0.1:5173/")?;
    preview.query_pairs_mut().append_pair("ws", &url);
    eprintln!(
        "[omochi-ui] bridge: {url}\nStart the browser preview in another terminal: bun run --filter @omochi-ui/dev-server dev\nOpen: {preview}"
    );
    let mut stream_finished = false;
    let mut protocol_error = None;
    let exit = tokio::select! {
        result = child.wait()=>Some(result?),
        _ = shutdown.wait()=>None,
        result = &mut stream=>{
            stream_finished = true;
            match result {
                Ok(Ok(()))=>tokio::select! { result=child.wait()=>Some(result?),_=shutdown.wait()=>None },
                other=>{ protocol_error = Some(format!("child protocol failed: {other:?}"));None },
            }
        },
    };
    if !stream_finished && exit.is_some() {
        match tokio::time::timeout(Duration::from_secs(1), &mut stream).await {
            Ok(Ok(Ok(()))) => {}
            Ok(other) => protocol_error = Some(format!("child protocol failed: {other:?}")),
            Err(_) => {}
        }
    }
    let code = if protocol_error.is_some() {
        1
    } else {
        exit.map(|s| {
            s.code()
                .unwrap_or_else(|| 128 + omochi_process::exit_signal(&s).unwrap_or(1))
        })
        .unwrap_or(0)
    };
    hub.shutdown_with_code(if protocol_error.is_some() {
        1002
    } else if exit.is_none() {
        1001
    } else if code == 0 {
        1000
    } else {
        1011
    });
    group.signal(omochi_process::SIGTERM);
    if tokio::time::timeout(Duration::from_secs(1), child.wait())
        .await
        .is_err()
    {
        group.signal(omochi_process::SIGKILL);
        let _ = child.wait().await;
    }
    if !stream_finished && !stream.is_finished() {
        stream.abort();
        let _ = stream.await;
    }
    http.await??;
    if let Some(error) = protocol_error {
        eprintln!("[omochi-ui] {error}");
    }
    Ok(code)
}
