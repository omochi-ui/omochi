//! Ephemeral, LAN-only probe of the production adapter and authenticated QUIC.
//! Usage: lan_probe serve [seconds] | lan_probe connect <EndpointId>
//! Add --rust to exercise interoperability with swarm-discovery 0.6.5.
use anyhow::{Context, Result, ensure};
use omochi_mdns::LanDiscovery;
use omochi_transport::{EndpointId, SecretKey};
use serde_json::json;
use std::time::Duration;
const ALPN: &[u8] = b"omochi/lan-probe/1";
#[tokio::main]
async fn main() -> Result<()> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["lifecycle"] {
        return lifecycle().await;
    }
    let raw = args.first().is_some_and(|s| s == "--rust");
    if raw {
        args.remove(0);
    }
    let key = SecretKey::from_bytes(&rand::random());
    let endpoint = if raw {
        let endpoint = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
            .secret_key(key)
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await?;
        endpoint
            .address_lookup()?
            .add(iroh_mdns_address_lookup::MdnsAddressLookup::builder().build(endpoint.id())?);
        endpoint
    } else {
        let endpoint = omochi_transport::bind(key, true, None).await?;
        endpoint.set_alpns(vec![ALPN.to_vec()]);
        endpoint
    };
    let diagnostic = LanDiscovery::new(endpoint.id(), endpoint.bound_sockets()[0].port()).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!(
        "{}",
        json!({"event":"ready","endpoint":endpoint.id().to_string(),"backend":if raw {"rust-mdns"} else {diagnostic.backend()},"addresses":endpoint.addr().ip_addrs().collect::<Vec<_>>()})
    );
    drop(diagnostic);
    let result = async {
        match args.first().map(String::as_str) {
            Some("serve") => {
                let seconds = args.get(1).map(|s| s.parse()).transpose()?.unwrap_or(90u64);
                let deadline = tokio::time::sleep(Duration::from_secs(seconds));
                tokio::pin!(deadline);
                loop {
                    tokio::select! {
                        _=&mut deadline=>break,
                        incoming=endpoint.accept()=> {
                            let connection=incoming.context("endpoint closed")?.await?;
                            let peer=connection.remote_id();
                            let (mut send,mut recv)=connection.accept_bi().await?;
                            ensure!(recv.read_to_end(32).await?==b"ping","invalid probe request");
                            send.write_all(b"pong").await?; send.finish()?; send.stopped().await?;
                            println!("{}",json!({"event":"accepted","peer":peer.to_string()}));
                        }
                    }
                }
            }
            Some("connect") => {
                let id: EndpointId = args.get(1).context("missing peer ID")?.parse()?;
                use n0_future::StreamExt;
                let mut stream=Box::pin(endpoint.address_lookup()?.resolve(id));
                if let Ok(Some(item))=tokio::time::timeout(Duration::from_secs(12),stream.next()).await {
                    match item {Ok(Ok(item))=>println!("{}",json!({"event":"resolved","addresses":item.to_endpoint_addr().ip_addrs().collect::<Vec<_>>()})), error=>eprintln!("{error:?}")}
                }
                let connection =
                    tokio::time::timeout(Duration::from_secs(25), endpoint.connect(id, ALPN))
                        .await??;
                ensure!(connection.remote_id() == id, "authenticated ID mismatch");
                let (mut send, mut recv) = connection.open_bi().await?;
                send.write_all(b"ping").await?;
                send.finish()?;
                ensure!(
                    recv.read_to_end(32).await? == b"pong",
                    "invalid probe reply"
                );
                println!("{}", json!({"event":"connected","peer":id.to_string()}));
            }
            _ => anyhow::bail!("usage: lan_probe [--rust] serve [seconds] | connect <EndpointId>"),
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    endpoint.close().await;
    // Allow OS workers to withdraw before this test process exits.
    tokio::time::sleep(Duration::from_millis(300)).await;
    result
}

async fn lifecycle() -> Result<()> {
    use iroh::address_lookup::{AddressLookup, EndpointData};
    use n0_future::StreamExt;
    use std::{collections::BTreeSet, net::SocketAddr};
    let own = SecretKey::from_bytes(&rand::random()).public();
    let peer = SecretKey::from_bytes(&rand::random()).public();
    let listener = LanDiscovery::new(own, 43122).await?;
    let data =
        |port| EndpointData::from(BTreeSet::from([SocketAddr::from(([192, 0, 2, 1], port))]));
    for port in [43123u16, 43124, 43125] {
        let publisher = LanDiscovery::new(peer, port).await?;
        // A changed relay TXT field exercises an update without rebinding the
        // endpoint's local socket or changing the native SRV port.
        for revision in 0..2 {
            let relay: iroh::RelayUrl =
                format!("https://relay-{port}-{revision}.example.invalid/").parse()?;
            let mut updated = data(port);
            updated.add_relay_url(relay.clone());
            publisher.publish(&updated);
            let mut stream = listener.resolve(peer).context("lookup unavailable")?;
            tokio::time::timeout(Duration::from_secs(10), async {
                while let Some(item) = stream.next().await {
                    let item = item?;
                    let addr = item.to_endpoint_addr();
                    if addr.ip_addrs().next().is_some()
                        && addr.ip_addrs().all(|a| a.port() == port)
                        && addr.relay_urls().any(|url| url == &relay)
                    {
                        println!(
                            "{}",
                            json!({"event":"updated","backend":listener.backend(),"port":port})
                        );
                        return Ok::<_, anyhow::Error>(());
                    }
                }
                anyhow::bail!("updated advertisement missing")
            })
            .await??;
        }
        drop(publisher);
        tokio::time::sleep(Duration::from_secs(2)).await;
        let mut absent = listener.resolve(peer).context("lookup unavailable")?;
        ensure!(
            tokio::time::timeout(Duration::from_millis(500), absent.next())
                .await
                .is_err(),
            "withdrawn service remained discoverable"
        );
        println!(
            "{}",
            json!({"event":"withdrawn","backend":listener.backend(),"port":port})
        );
    }
    drop(listener);
    tokio::time::sleep(Duration::from_millis(300)).await;
    Ok(())
}
