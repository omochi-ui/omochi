//! No UDP/IP transports or external discovery: success requires the local relay.
use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr, RelayMode, RelayUrl};
use iroh_relay::server::{RelayConfig, Server, ServerConfig};
use omochi_protocol::{ALPN, PAIR_ALPN, read_frame, write_frame};
use omochi_transport::{
    pairing::{self, Invite},
    store::{Role, Store},
};
use serde_json::{Value, json};
use std::time::Duration;

#[tokio::test]
async fn enrollment_and_application_frames_cross_relay_without_direct_paths() {
    tokio::time::timeout(Duration::from_secs(45), async {
        let mut config = ServerConfig::default();
        // Plain HTTP is restricted to loopback in this test; production uses TLS.
        config.relay = Some(RelayConfig::new(
            "127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let server = Server::spawn(config).await.unwrap();
        let relay: RelayUrl = format!("http://{}", server.http_addr().unwrap())
            .parse()
            .unwrap();
        let root = tempfile::tempdir().unwrap();
        let gateway_store = Store::open(root.path().join("gateway")).unwrap();
        let hub_store = Store::open(root.path().join("hub")).unwrap();
        let mut endpoints = Vec::new();
        for (store, role) in [(&gateway_store, Role::Gateway), (&hub_store, Role::Hub)] {
            let endpoint = Endpoint::builder(presets::Minimal)
                .secret_key(store.identity(role).unwrap())
                .alpns(vec![ALPN.to_vec(), PAIR_ALPN.to_vec()])
                .relay_mode(RelayMode::custom([relay.clone()]))
                .clear_ip_transports()
                .bind()
                .await
                .unwrap();
            endpoint.online().await;
            endpoints.push(endpoint);
        }
        let hub = endpoints.pop().unwrap();
        let gateway = endpoints.pop().unwrap();
        let target = EndpointAddr::new(gateway.id()).with_relay_url(relay);
        let invite = Invite::decode(
            &Invite::issue(&gateway_store, &gateway)
                .unwrap()
                .encode()
                .unwrap(),
        )
        .unwrap();
        let peer = hub.id();
        let remote = gateway.clone();
        let trust = gateway_store.clone();
        let serving = tokio::spawn(async move {
            let pairing = remote.accept().await.unwrap().await.unwrap();
            assert!(pairing.paths().iter().all(|path| !path.is_ip()));
            pairing::accept(pairing, trust.clone()).await.unwrap();
            let connection = remote.accept().await.unwrap().await.unwrap();
            assert_eq!(connection.remote_id(), peer);
            assert_eq!(trust.peers().unwrap().1[&peer.to_string()].role, Role::Hub);
            assert!(connection.paths().iter().all(|path| !path.is_ip()));
            assert!(connection.paths().iter().any(|path| path.is_relay()));
            let (mut send, mut recv) = connection.accept_bi().await.unwrap();
            let value: Value = read_frame(&mut recv).await.unwrap().unwrap();
            write_frame(&mut send, &value).await.unwrap();
            send.finish().unwrap();
            let _ = send.stopped().await;
        });
        pairing::redeem(&hub, hub_store.clone(), invite, "remote", "PC")
            .await
            .unwrap();
        assert_eq!(
            hub_store.peers().unwrap().1[&gateway.id().to_string()].role,
            Role::Gateway
        );
        let connection = hub.connect(target, ALPN).await.unwrap();
        assert!(connection.paths().iter().all(|path| !path.is_ip()));
        let (mut send, mut recv) = connection.open_bi().await.unwrap();
        let expected = json!({"payload":"x".repeat(128 * 1024)});
        write_frame(&mut send, &expected).await.unwrap();
        send.finish().unwrap();
        assert_eq!(
            read_frame::<_, Value>(&mut recv).await.unwrap(),
            Some(expected)
        );
        serving.await.unwrap();
        connection.close(0u32.into(), b"test complete");
        hub.close().await;
        gateway.close().await;
        server.shutdown().await.unwrap();
    })
    .await
    .expect("relay-only exchange timed out");
}
