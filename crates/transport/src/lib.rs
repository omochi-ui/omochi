//! Iroh transport integration. Application authorization is separate from TLS identity.
#[cfg(feature = "desktop")]
pub mod live;
pub mod pairing;
#[cfg(feature = "desktop")]
pub mod store;
pub mod trust;
use anyhow::Result;
pub use iroh::endpoint::Connection as EndpointConnection;
pub use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use iroh::{RelayMode, RelayUrl, endpoint::presets};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use omochi_protocol::{ALPN, PAIR_ALPN};

/// Construct only when remote access has explicitly been enabled.
/// Offline mode never creates public DNS/relay clients. Neither mode awaits online().
pub async fn bind(
    key: SecretKey,
    offline: bool,
    relays: Option<Vec<RelayUrl>>,
) -> Result<Endpoint> {
    let mut builder = if offline {
        Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled)
    } else {
        Endpoint::builder(presets::N0)
    };
    if !offline && let Some(relays) = relays {
        builder = builder.relay_mode(RelayMode::custom(relays));
    }
    let endpoint = builder
        .secret_key(key)
        .alpns(vec![ALPN.to_vec(), PAIR_ALPN.to_vec()])
        .bind()
        .await?;
    let mdns = MdnsAddressLookup::builder().build(endpoint.id())?;
    endpoint.address_lookup()?.add(mdns);
    Ok(endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use omochi_protocol::{read_frame, write_frame};
    use serde_json::{Value, json};
    use std::time::Duration;
    #[tokio::test]
    async fn direct_connection_without_external_services() {
        let a = bind(SecretKey::from_bytes(&rand::random()), true, None)
            .await
            .unwrap();
        let b = bind(SecretKey::from_bytes(&rand::random()), true, None)
            .await
            .unwrap();
        let target = b.addr();
        let expected = a.id();
        let server = tokio::spawn(async move {
            let connection = b.accept().await.unwrap().await.unwrap();
            assert_eq!(connection.remote_id(), expected);
            let (mut tx, mut rx) = connection.accept_bi().await.unwrap();
            assert_eq!(
                read_frame::<_, Value>(&mut rx).await.unwrap(),
                Some(json!({"hello": 4}))
            );
            write_frame(&mut tx, &json!({"ready": true})).await.unwrap();
            tx.finish().unwrap();
            let _ = tx.stopped().await;
            b.close().await;
        });
        tokio::time::timeout(Duration::from_secs(15), async {
            let connection = a.connect(target, ALPN).await.unwrap();
            let (mut tx, mut rx) = connection.open_bi().await.unwrap();
            write_frame(&mut tx, &json!({"hello": 4})).await.unwrap();
            assert_eq!(
                read_frame::<_, Value>(&mut rx).await.unwrap(),
                Some(json!({"ready": true}))
            );
            tx.finish().unwrap();
            server.await.unwrap();
        })
        .await
        .unwrap();
        a.close().await;
    }
    #[tokio::test]
    #[ignore = "requires multicast; run explicitly on a LAN-capable host"]
    async fn mdns_resolves_only_an_endpoint_id_without_internet() {
        let a = bind(SecretKey::from_bytes(&rand::random()), true, None)
            .await
            .unwrap();
        let b = bind(SecretKey::from_bytes(&rand::random()), true, None)
            .await
            .unwrap();
        let id = b.id();
        let server = tokio::spawn(async move {
            let connection = b.accept().await.unwrap().await.unwrap();
            connection.closed().await;
            b.close().await;
        });
        let connection = tokio::time::timeout(Duration::from_secs(30), a.connect(id, ALPN))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(connection.remote_id(), id);
        connection.close(0u32.into(), b"test complete");
        a.close().await;
        server.await.unwrap();
    }
}
