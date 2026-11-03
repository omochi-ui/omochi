//! Iroh transport integration. Application authorization is separate from TLS identity.
#[cfg(feature = "desktop")]
pub mod live;
pub mod pairing;
#[cfg(feature = "desktop")]
pub mod store;
pub mod trust;
use anyhow::Result;
#[cfg(test)]
use iroh::RelayConfig;
pub use iroh::endpoint::{Connection as EndpointConnection, RecvStream, SendStream};
pub use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use iroh::{RelayMode, RelayUrl, endpoint::presets};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use omochi_protocol::{ALPN, PAIR_ALPN};

mod relay;
#[cfg(test)]
use relay::{PRODUCTION_RELAY_TOKEN, PRODUCTION_RELAY_URL};
use relay::{production_relay, relay_config};

fn relay_mode(offline: bool, relays: Option<Vec<RelayUrl>>) -> RelayMode {
    if offline {
        return RelayMode::Disabled;
    }
    let relays = match relays {
        Some(relays) => relays,
        None if cfg!(debug_assertions) => return iroh::endpoint::default_relay_mode(),
        None => vec![production_relay().url],
    };
    RelayMode::Custom(relays.into_iter().map(relay_config).collect())
}

/// Construct only when remote access has explicitly been enabled.
/// Offline mode never creates public DNS/relay clients. Neither mode awaits online().
/// Release builds default to the authenticated Omochi relay; debug builds use n0.
pub async fn bind(
    key: SecretKey,
    offline: bool,
    relays: Option<Vec<RelayUrl>>,
) -> Result<Endpoint> {
    let builder = if offline {
        Endpoint::builder(presets::Minimal)
    } else {
        Endpoint::builder(presets::N0)
    };
    let endpoint = builder
        .relay_mode(relay_mode(offline, relays))
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

    #[test]
    fn default_relays_follow_build_profile() {
        let mode = relay_mode(false, None);
        if cfg!(debug_assertions) {
            assert!(matches!(mode, RelayMode::Default | RelayMode::Staging));
        } else {
            let relays = mode.relay_map().relays::<Vec<_>>();
            assert_eq!(relays.len(), 1);
            assert_eq!(relays[0].url, PRODUCTION_RELAY_URL.parse().unwrap());
            assert_eq!(
                relays[0].auth_token.as_deref(),
                Some(PRODUCTION_RELAY_TOKEN)
            );
        }
    }

    #[test]
    fn offline_disables_even_explicit_relays() {
        assert!(matches!(relay_mode(true, None), RelayMode::Disabled));
        assert!(matches!(
            relay_mode(true, Some(vec![production_relay().url])),
            RelayMode::Disabled
        ));
    }

    #[test]
    fn custom_relays_replace_defaults_and_only_production_gets_the_token() {
        let custom = "https://relay.example.invalid".parse::<RelayUrl>().unwrap();
        let production = production_relay();
        let map = relay_mode(false, Some(vec![custom.clone(), production.url.clone()])).relay_map();
        assert_eq!(map.relays::<Vec<_>>().len(), 2);
        assert_eq!(map.get(&custom).unwrap().auth_token, None);
        assert_eq!(map.get(&production.url).unwrap().as_ref(), &production);
        assert_eq!(
            relay_mode(false, Some(vec![custom.clone()]))
                .relay_map()
                .relays::<Vec<_>>()
                .len(),
            1
        );
    }

    #[test]
    fn production_admission_is_scoped_to_the_exact_relay_url() {
        let production = relay_config("https://relay.omochi.fyi/".parse().unwrap());
        assert!(production.auth_token.as_deref() == Some(PRODUCTION_RELAY_TOKEN));
        for url in [
            "http://relay.omochi.fyi/",
            "https://relay.omochi.fyi:444/",
            "https://relay.omochi.fyi/other",
            "https://relay.omochi.fyi/?other=1",
            "https://relay.omochi.fyi.example/",
        ] {
            assert!(
                relay_config(url.parse().unwrap()).auth_token.is_none(),
                "{url}"
            );
        }
    }

    async fn exchange_over_relay(config: RelayConfig) {
        let relay = config.url.clone();
        let mode = RelayMode::Custom(config.into());
        let mut endpoints = Vec::new();
        for _ in 0..2 {
            let endpoint = Endpoint::builder(presets::Minimal)
                .alpns(vec![ALPN.to_vec()])
                .relay_mode(mode.clone())
                .clear_ip_transports()
                .bind()
                .await
                .unwrap();
            endpoint.online().await;
            endpoints.push(endpoint);
        }
        let client = endpoints.pop().unwrap();
        let server = endpoints.pop().unwrap();
        let target = EndpointAddr::new(server.id()).with_relay_url(relay);
        let serving = tokio::spawn(async move {
            let connection = server.accept().await.unwrap().await.unwrap();
            assert!(connection.paths().iter().all(|path| path.is_relay()));
            let (mut tx, mut rx) = connection.accept_bi().await.unwrap();
            let value = read_frame::<_, Value>(&mut rx).await.unwrap().unwrap();
            write_frame(&mut tx, &value).await.unwrap();
            tx.finish().unwrap();
            let _ = tx.stopped().await;
            server.close().await;
        });
        let connection = client.connect(target, ALPN).await.unwrap();
        assert!(connection.paths().iter().any(|path| path.is_relay()));
        assert!(connection.paths().iter().all(|path| !path.is_ip()));
        let (mut tx, mut rx) = connection.open_bi().await.unwrap();
        let expected = json!({"relay": "authenticated"});
        write_frame(&mut tx, &expected).await.unwrap();
        tx.finish().unwrap();
        assert_eq!(
            read_frame::<_, Value>(&mut rx).await.unwrap(),
            Some(expected)
        );
        serving.await.unwrap();
        client.close().await;
    }

    #[tokio::test]
    async fn production_token_authenticates_relay_traffic() {
        use iroh_relay::server::{Access, AccessControl, ClientRequest, Server, ServerConfig};
        #[derive(Debug)]
        struct SharedToken;
        impl AccessControl for SharedToken {
            async fn on_connect(&self, request: &ClientRequest) -> Access {
                if request.auth_token().as_deref() == Some(PRODUCTION_RELAY_TOKEN) {
                    Access::Allow
                } else {
                    Access::Deny { reason: None }
                }
            }
        }
        let mut relay = iroh_relay::server::RelayConfig::new(
            "127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap(),
        );
        relay.access = std::sync::Arc::new(SharedToken);
        let mut config = ServerConfig::default();
        config.relay = Some(relay);
        let server = Server::spawn(config).await.unwrap();
        let mut client_config = production_relay();
        client_config.url = format!("http://{}", server.http_addr().unwrap())
            .parse()
            .unwrap();
        client_config.quic = None;
        let result =
            tokio::time::timeout(Duration::from_secs(30), exchange_over_relay(client_config)).await;
        server.shutdown().await.unwrap();
        result.expect("authenticated relay exchange timed out");
    }

    #[tokio::test]
    #[ignore = "requires the deployed production relay and Internet access"]
    async fn deployed_production_relay_carries_traffic() {
        tokio::time::timeout(
            Duration::from_secs(45),
            exchange_over_relay(production_relay()),
        )
        .await
        .expect("production relay exchange timed out");
    }

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
