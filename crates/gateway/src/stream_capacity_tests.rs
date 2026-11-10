//! Real SDK JSONL sockets and framed hub streams, synchronized by snapshots/replies.
use super::*;
use omochi_protocol::{ALPN, OpenStream, VERSION, read_frame, session::ServiceFrame, write_frame};
use omochi_transport::store::{Role, Store};
use serde_json::json;
use std::{future::Future, os::unix::fs::PermissionsExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{UnixListener, UnixStream},
    task::JoinSet,
};

async fn fixture(root: &std::path::Path) -> (Gateway, JoinHandle<Result<()>>, JoinSet<()>) {
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700)).unwrap();
    omochi_local::LocalRuntime::prepare(Some(root.to_path_buf())).unwrap();
    let mut sdks = JoinSet::new();
    for index in 0..MAX_SERVICES {
        let dir = root.join(format!("svc-app{index:03}~"));
        omochi_local::files::private_directory(&dir).unwrap();
        std::fs::write(
            dir.join("manifest.json"),
            json!({"version":2,"serviceId":format!("app{index:03}"),"name":"App","endpoint":{"kind":"unix","path":"a2ui.sock"}}).to_string(),
        )
        .unwrap();
        let listener = UnixListener::bind(dir.join("a2ui.sock")).unwrap();
        sdks.spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut frames = Framed::new(socket, LinesCodec::new());
            frames.send(json!({"omochi":"resources/1","kind":"hello"}).to_string()).await.unwrap();
            frames.send(json!({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"}}).to_string()).await.unwrap();
            while let Some(Ok(line)) = frames.next().await {
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["op"], "stat");
                frames.send(json!({"omochi":"resources/1","kind":"response","id":request["id"],"ok":true,"size":42}).to_string()).await.unwrap();
            }
        });
    }
    let gateway = Gateway::new();
    let discovery = gateway.discover(root.to_owned());
    // Snapshot readiness also confirms the earlier resources/1 hello was processed.
    loop {
        let services = gateway.list().await;
        if services.len() == MAX_SERVICES {
            let mut ready = true;
            for service in services {
                ready &= !gateway
                    .subscribe(&service.id, service.generation)
                    .await
                    .unwrap()
                    .snapshot
                    .is_empty();
            }
            if ready {
                break;
            }
        }
        tokio::task::yield_now().await;
    }
    (gateway, discovery, sdks)
}

struct HubStreams<R, W> {
    control: (R, W),
    services: Vec<(R, W)>,
    roster: Vec<Service>,
}
async fn subscribe_all<R, W, F, Fut>(open: &mut F) -> HubStreams<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    F: FnMut() -> Fut,
    Fut: Future<Output = (R, W)>,
{
    let (mut reader, mut writer) = open().await;
    write_frame(&mut writer, &OpenStream::Control { protocol: VERSION })
        .await
        .unwrap();
    assert!(matches!(
        read_frame::<_, Control>(&mut reader).await.unwrap(),
        Some(Control::Hello { .. })
    ));
    let Some(Control::Services { services: roster }) = read_frame(&mut reader).await.unwrap()
    else {
        panic!("missing roster")
    };
    assert_eq!(roster.len(), MAX_SERVICES);
    let mut services = Vec::new();
    for service in &roster {
        let (mut reader, mut writer) = open().await;
        write_frame(
            &mut writer,
            &OpenStream::Service {
                service: service.id.clone(),
                generation: service.generation,
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            read_frame::<_, ServiceFrame>(&mut reader).await.unwrap(),
            Some(ServiceFrame::SnapshotStart)
        ));
        assert!(
            matches!(read_frame::<_, ServiceFrame>(&mut reader).await.unwrap(), Some(ServiceFrame::Message { message }) if message["createSurface"]["surfaceId"] == "main")
        );
        assert!(matches!(
            read_frame::<_, ServiceFrame>(&mut reader).await.unwrap(),
            Some(ServiceFrame::SnapshotEnd)
        ));
        services.push((reader, writer));
    }
    HubStreams {
        control: (reader, writer),
        services,
        roster,
    }
}
async fn resource_and_ping<R, W, F, Fut>(hub: &mut HubStreams<R, W>, open: &mut F)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    F: FnMut() -> Fut,
    Fut: Future<Output = (R, W)>,
{
    assert_eq!(hub.services.len(), MAX_SERVICES);
    // Keep every subscription alive while using the independent resource budget.
    for service in [&hub.roster[0], &hub.roster[MAX_SERVICES - 1]] {
        let (mut reader, mut writer) = open().await;
        write_frame(
            &mut writer,
            &OpenStream::Resource {
                service: service.id.clone(),
                generation: service.generation,
            },
        )
        .await
        .unwrap();
        write_frame(&mut writer, &json!({"op":"stat","resource":"a".repeat(32)}))
            .await
            .unwrap();
        let response: Value = read_frame(&mut reader).await.unwrap().unwrap();
        assert_eq!(response["ok"], true);
        assert_eq!(response["size"], 42);
    }
    write_frame(&mut hub.control.1, &Control::Ping { id: 42 })
        .await
        .unwrap();
    assert!(matches!(
        read_frame::<_, Control>(&mut hub.control.0).await.unwrap(),
        Some(Control::Pong { id: 42 })
    ));
}

#[tokio::test]
async fn unix_multiple_hubs_keep_full_rosters_and_resource_capacity() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let root = tempfile::tempdir().unwrap();
        let (gateway, discovery, mut sdks) = fixture(root.path()).await;
        let path = root.path().join("gateway.sock");
        let server = gateway
            .listen_local(
                (path.clone()).parent().unwrap().to_path_buf(),
                "a".repeat(64),
                "test".into(),
            )
            .await
            .unwrap();
        let mut open = || async { UnixStream::connect(&path).await.unwrap().into_split() };
        let mut first = subscribe_all(&mut open).await;
        let mut second = subscribe_all(&mut open).await;
        resource_and_ping(&mut first, &mut open).await;
        resource_and_ping(&mut second, &mut open).await;
        gateway.shutdown();
        server.await.unwrap().unwrap();
        discovery.await.unwrap().unwrap();
        while let Some(result) = sdks.join_next().await {
            result.unwrap();
        }
    })
    .await
    .expect("Unix full-roster exchange timed out");
}

#[tokio::test]
async fn iroh_full_roster_keeps_resource_capacity_and_ping() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let root = tempfile::tempdir().unwrap();
        let runtime = root.path().join("runtime");
        std::fs::create_dir(&runtime).unwrap();
        let (gateway, discovery, mut sdks) = fixture(&runtime).await;
        let store = Store::open(root.path().join("gateway")).unwrap();
        let hub_store = Store::open(root.path().join("hub")).unwrap();
        let endpoint = omochi_transport::bind(store.identity(Role::Gateway).unwrap(), true, None)
            .await
            .unwrap();
        let hub = omochi_transport::bind(hub_store.identity(Role::Hub).unwrap(), true, None)
            .await
            .unwrap();
        let token = store.invite(0, u64::MAX).unwrap();
        store.redeem(&token, hub.id(), "hub", 1).unwrap();
        let address = endpoint.addr();
        let server = gateway
            .listen_remote(endpoint, store, "test".into())
            .unwrap();
        let connection = hub.connect(address, ALPN).await.unwrap();
        let mut open = || async {
            let (writer, reader) = connection.open_bi().await.unwrap();
            (reader, writer)
        };
        let mut streams = subscribe_all(&mut open).await;
        // A duplicate service over the per-session cap is rejected without
        // consuming the resource/control reservations or closing the session.
        let (mut reader, mut writer) = open().await;
        write_frame(
            &mut writer,
            &OpenStream::Service {
                service: streams.roster[0].id.clone(),
                generation: streams.roster[0].generation,
            },
        )
        .await
        .unwrap();
        assert!(!matches!(
            read_frame::<_, ServiceFrame>(&mut reader).await,
            Ok(Some(_))
        ));
        resource_and_ping(&mut streams, &mut open).await;
        gateway.shutdown();
        server.await.unwrap();
        discovery.await.unwrap().unwrap();
        while let Some(result) = sdks.join_next().await {
            result.unwrap();
        }
        hub.close().await;
    })
    .await
    .expect("iroh full-roster exchange timed out");
}
