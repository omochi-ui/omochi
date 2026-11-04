//! Ephemeral LAN-only endpoint for apps/mobile/tool/bonjour_probe.py.
//! Prints public addressing information only; never uses an on-disk identity.
use anyhow::{Context, Result, ensure};
use iroh::address_lookup::AddressLookup;
use iroh_mdns_address_lookup::MdnsAddressLookup;
use omochi_transport::SecretKey;
use serde_json::json;
use std::{future::poll_fn, time::Duration};
use tokio::io::{AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() -> Result<()> {
    let endpoint =
        omochi_transport::bind(SecretKey::from_bytes(&rand::random()), true, None).await?;
    let result = probe(&endpoint).await;
    endpoint.close().await;
    result
}

async fn probe(endpoint: &omochi_transport::Endpoint) -> Result<()> {
    let remote = SecretKey::from_bytes(&rand::random()).public();
    let lookup = MdnsAddressLookup::builder()
        .advertise(false)
        .build(endpoint.id())?;
    let mut stream = lookup.resolve(remote).context("mDNS lookup unavailable")?;
    // bind() can return before local interface enumeration has completed.
    tokio::time::timeout(Duration::from_secs(5), async {
        while endpoint.addr().ip_addrs().next().is_none() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    println!(
        "{}",
        json!({
            "event": "ready", "linux": endpoint.id().to_string(),
            "mac": remote.to_string(),
            "addresses": endpoint.addr().ip_addrs().map(ToString::to_string).collect::<Vec<_>>()
        })
    );
    let mut stdin = BufReader::new(tokio::io::stdin());
    let mut command = String::new();
    let mut resolved = false;
    let deadline = tokio::time::sleep(Duration::from_secs(40));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            item = poll_fn(|cx| stream.as_mut().poll_next(cx)), if !resolved => {
                let item = item.context("mDNS lookup ended without a Bonjour peer")??;
                ensure!(item.endpoint_id() == remote, "wrong EndpointId from lookup");
                let addr = item.to_endpoint_addr();
                let addresses: Vec<_> = addr.ip_addrs().map(ToString::to_string).collect();
                if !addresses.is_empty() {
                    println!("{}", json!({"event":"resolved", "endpoint":remote.to_string(), "addresses":addresses}));
                    resolved = true;
                }
            }
            count = stdin.read_line(&mut command) => {
                let eof = count? == 0;
                ensure!(eof || command.trim() == "stop", "unexpected probe command");
                ensure!(resolved, "stopped before resolving the Bonjour peer");
                return Ok(());
            }
            _ = &mut deadline => anyhow::bail!("probe timed out"),
        }
    }
}
