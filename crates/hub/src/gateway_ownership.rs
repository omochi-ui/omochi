use tokio::net::UnixStream;
// One node can be reachable through both automatic Unix and registered iroh.
use super::*;
use futures_util::{SinkExt, StreamExt};
use omochi_transport::store::{Role, Store};
use std::os::unix::fs::PermissionsExt;
use tokio_util::codec::{Framed, LinesCodec};

async fn update_through_action(
    hub: &Hub,
    sdk: &mut Framed<UnixStream, LinesCodec>,
    events: &mut scoped::EventReceiver,
    surface: &str,
    sequence: u64,
) {
    hub.action(
        json!({"version":"v0.9","action":{"surfaceId":surface,"name":"click","sequence":sequence}}),
    )
    .await
    .unwrap();
    let action: Value = serde_json::from_str(&sdk.next().await.unwrap().unwrap()).unwrap();
    assert_eq!(action["action"]["surfaceId"], "main");
    assert_eq!(action["action"]["sequence"], sequence);
    sdk.send(json!({"version":"v0.9","updateDataModel":{"surfaceId":"main","path":"/","value":{"sequence":sequence}}}).to_string())
        .await.unwrap();
    // This acknowledged update is the boundary: no intervening replay or delete
    // may appear, and the retained snapshot must contain the same update.
    let update = events.recv().await.unwrap();
    assert_eq!(update["updateDataModel"]["surfaceId"], surface);
    assert_eq!(update["updateDataModel"]["value"]["sequence"], sequence);
    let snapshot = hub.subscribe().await.snapshot;
    assert_eq!(snapshot.len(), 2);
    assert_eq!(
        snapshot
            .iter()
            .find(|message| message.get("updateDataModel").is_some()),
        Some(update.as_ref())
    );
}

#[tokio::test]
async fn unix_and_registered_iroh_share_services_and_fail_over_after_cleanup() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let root = tempfile::tempdir().unwrap();
        let runtime = root.path().join("runtime");
        std::fs::create_dir(&runtime).unwrap();
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
        omochi_local::LocalRuntime::prepare(Some(runtime.to_path_buf())).unwrap();
        let app = runtime.join("svc-app~");
        omochi_local::files::private_directory(&app).unwrap();
        std::fs::write(
            app.join("manifest.json"),
            r#"{"version":2,"serviceId":"app","name":"App","endpoint":{"kind":"unix","path":"a2ui.sock"}}"#,
        )
        .unwrap();
        let listener = tokio::net::UnixListener::bind(app.join("a2ui.sock")).unwrap();
        let gateway = omochi_gateway::Gateway::new();
        let discovery = gateway.discover(runtime.clone());
        let (sdk, _) = listener.accept().await.unwrap();
        let mut sdk = Framed::new(sdk, LinesCodec::new());
        sdk.send(
            json!({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"}})
                .to_string(),
        )
        .await
        .unwrap();

        let store = Store::open(root.path().join("state")).unwrap();
        let gateway_endpoint =
            omochi_transport::bind(store.identity(Role::Gateway).unwrap(), true, None)
                .await
                .unwrap();
        let hub_endpoint = omochi_transport::bind(store.identity(Role::Hub).unwrap(), true, None)
            .await
            .unwrap();
        let node = gateway_endpoint.id().to_string();
        let invitation = store.invite(0, u64::MAX).unwrap();
        store
            .redeem(&invitation, hub_endpoint.id(), "hub", 1)
            .unwrap();
        store
            .register_gateway(gateway_endpoint.id(), "gateway")
            .unwrap();
        // Seed the registered endpoint's direct address without multicast.
        let address = gateway_endpoint.addr();
        let remote_server = gateway
            .listen_remote(gateway_endpoint, store.clone(), "gateway".into())
            .unwrap();
        let warm = hub_endpoint
            .connect(address, omochi_protocol::ALPN)
            .await
            .unwrap();
        warm.close(0u32.into(), b"address seeded");
        let socket = runtime.join("gateway.sock");
        let unix_server = gateway
            .listen_local((socket.clone()).parent().unwrap().to_path_buf(), node.clone(), "gateway".into()).await
            .unwrap();

        let mut hub = Hub::new();
        hub.images = images::tests::pending_resolver();
        let mut subscription = hub.subscribe().await;
        let local_stop = hub.stop.child_token();
        let local = tokio::spawn({
            let hub = hub.clone();
            let stop = local_stop.clone();
            async move {
                hub.run_until(Connection::Local(omochi_local::LocalEndpoint::Unix { path: socket }), None, true, stop)
                    .await
            }
        });
        let surface = format!("{node}~app/main");
        let route = format!("{node}~app");
        assert_eq!(
            subscription.events.recv().await.unwrap()["createSurface"]["surfaceId"],
            surface
        );
        let original_epoch = hub.state.lock().await.channels[&route].epoch;
        let original_lifetime = hub.state.lock().await.channels[&route].lifetime.clone();

        let (registered, _) = hub.attach_registered(hub_endpoint, store.clone());
        loop {
            if hub
                .diagnostics
                .lock()
                .await
                .get(&node)
                .is_some_and(|status| status == "connected")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // "connected" is published after roster validation/admission. The
        // duplicate path must preserve the existing channel and Surface scope.
        assert_eq!(
            hub.state.lock().await.channels[&route].epoch,
            original_epoch
        );
        assert_eq!(hub.services().await[0]["node"]["local"], true);
        assert!(!original_lifetime.is_cancelled());
        update_through_action(&hub, &mut sdk, &mut subscription.events, &surface, 1).await;

        let mut scoped = hub.subscribe_scoped().await.unwrap();
        let handle = scoped.next_batch().await.unwrap().handle.clone().unwrap();
        let image = tokio::spawn(async move {
            handle
                .image("https://image.example/local", CancellationToken::new())
                .await
        });
        images::tests::wait_for_requests(&hub.images, 1).await;

        // Revoking a registered standby joins its connection before acknowledging
        // the store change, while the independently trusted Unix owner stays live.
        let peer = node.parse().unwrap();
        omochi_transport::live::forget(store.clone(), peer, Duration::from_secs(3))
            .await
            .unwrap();
        assert_eq!(
            hub.state.lock().await.channels[&route].epoch,
            original_epoch
        );
        update_through_action(&hub, &mut sdk, &mut subscription.events, &surface, 2).await;
        assert!(!original_lifetime.is_cancelled());
        assert!(!image.is_finished());
        assert!(!hub.stop.is_cancelled());
        store.register_gateway(peer, "gateway").unwrap();
        loop {
            if hub
                .diagnostics
                .lock()
                .await
                .get(&node)
                .is_some_and(|status| status == "connected")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            hub.state.lock().await.channels[&route].epoch,
            original_epoch
        );

        // Loss of the owning path transfers ownership only after its service
        // tasks finish. The standby replays a fresh snapshot in delete/create order.
        local_stop.cancel();
        local.await.unwrap().unwrap();
        assert!(matches!(
            image.await.unwrap(),
            Err(images::ImageError::Unavailable)
        ));
        assert!(original_lifetime.is_cancelled());
        assert_eq!(
            subscription.events.recv().await.unwrap()["deleteSurface"]["surfaceId"],
            surface
        );
        assert_eq!(
            subscription.events.recv().await.unwrap()["createSurface"]["surfaceId"],
            surface
        );
        assert_eq!(
            subscription.events.recv().await.unwrap()["updateDataModel"]["value"]["sequence"],
            2
        );
        assert_ne!(
            hub.state.lock().await.channels[&route].epoch,
            original_epoch
        );
        assert_eq!(hub.services().await[0]["node"]["local"], false);
        update_through_action(&hub, &mut sdk, &mut subscription.events, &surface, 3).await;

        let mut scoped = hub.subscribe_scoped().await.unwrap();
        let handle = scoped.next_batch().await.unwrap().handle.clone().unwrap();
        let remote_image = tokio::spawn(async move {
            handle
                .image("https://image.example/remote", CancellationToken::new())
                .await
        });
        images::tests::wait_for_requests(&hub.images, 1).await;
        // This time iroh owns the source: its image task must finish destruction
        // before forget receives the supervisor's acknowledgement.
        omochi_transport::live::forget(store, peer, Duration::from_secs(3))
            .await
            .unwrap();
        assert_eq!(images::tests::active_requests(&hub.images), 0);
        assert!(matches!(
            remote_image.await.unwrap(),
            Err(images::ImageError::Unavailable)
        ));

        hub.shutdown();
        registered.await.unwrap();
        assert_eq!(
            subscription.events.recv().await.unwrap()["deleteSurface"]["surfaceId"],
            surface
        );
        assert!(hub.subscribe().await.snapshot.is_empty());
        assert!(hub.services().await.is_empty());
        assert!(hub.state.lock().await.gateway_owners.is_empty());
        gateway.shutdown();
        unix_server.await.unwrap().unwrap();
        remote_server.await.unwrap();
        discovery.await.unwrap().unwrap();
    })
    .await
    .expect("gateway ownership test timed out");
}
