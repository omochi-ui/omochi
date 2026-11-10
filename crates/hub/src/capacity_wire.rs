//! Wire-level capacity regression: two Gateway controllers, independent service streams.
use super::*;
use tokio::net::UnixStream;
use tokio::{net::UnixListener, sync::mpsc, task::JoinSet};

type Update = (String, Arc<Value>);
struct Fixture {
    stop: CancellationToken,
    task: JoinHandle<()>,
}
impl Fixture {
    async fn start(
        path: PathBuf,
        node: String,
        delay: Duration,
        updates: broadcast::Sender<Update>,
        actions: mpsc::Sender<(String, Value)>,
        opens: Arc<Mutex<Vec<String>>>,
    ) -> Self {
        let listener = UnixListener::bind(path).unwrap();
        let stop = CancellationToken::new();
        let cancel = stop.clone();
        let task = tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            loop {
                let socket = tokio::select! {
                    _=cancel.cancelled()=>break,
                    socket=listener.accept()=>socket.unwrap().0,
                };
                let node = node.clone();
                let updates = updates.subscribe();
                let actions = actions.clone();
                let opens = opens.clone();
                tasks.spawn(async move {
                    let _ = serve_stream(socket, node, delay, updates, actions, opens).await;
                });
            }
            tasks.shutdown().await;
        });
        Self { stop, task }
    }
    async fn close(self) {
        self.stop.cancel();
        self.task.await.unwrap();
    }
}
async fn serve_stream(
    socket: UnixStream,
    node: String,
    delay: Duration,
    mut updates: broadcast::Receiver<Update>,
    actions: mpsc::Sender<(String, Value)>,
    opens: Arc<Mutex<Vec<String>>>,
) -> Result<()> {
    let (mut reader, mut writer) = socket.into_split();
    match read_frame::<_, OpenStream>(&mut reader)
        .await?
        .context("header")?
    {
        OpenStream::Control { .. } => {
            write_frame(
                &mut writer,
                &Control::Hello {
                    node_id: node,
                    name: "capacity fixture".into(),
                    protocol: VERSION,
                },
            )
            .await?;
            tokio::time::sleep(delay).await;
            let services = (0..6)
                .map(|i| Service {
                    id: format!("app{i}"),
                    name: format!("App {i}"),
                    generation: 1,
                    icon: None,
                })
                .collect();
            write_frame(&mut writer, &Control::Services { services }).await?;
            while let Some(Control::Ping { id }) = read_frame(&mut reader).await? {
                write_frame(&mut writer, &Control::Pong { id }).await?;
            }
        }
        OpenStream::Service { service, .. } => {
            let route = format!("{node}~{service}");
            opens.lock().await.push(route.clone());
            write_frame(&mut writer, &ServiceFrame::SnapshotStart).await?;
            write_frame(&mut writer,&ServiceFrame::Message { message:json!({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"}}) }).await?;
            write_frame(&mut writer, &ServiceFrame::SnapshotEnd).await?;
            loop {
                // Keep partial action reads intact while injecting updates.
                let action = read_frame::<_, Value>(&mut reader);
                tokio::pin!(action);
                let action = loop {
                    tokio::select! {
                        action=&mut action=>break action?,
                        update=updates.recv()=>{
                            let (target,message)=update?;
                            if target==route {
                                write_frame(&mut writer,&ServiceFrame::Message { message:(*message).clone() }).await?;
                            }
                        }
                    }
                };
                let Some(action) = action else {
                    break;
                };
                actions.send((route.clone(), action)).await?;
            }
        }
        _ => bail!("unexpected stream"),
    }
    Ok(())
}
async fn wait_for(hub: &Hub, predicate: impl Fn(&[Value]) -> bool) -> Vec<Value> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let services = hub.services().await;
            if predicate(&services) {
                return services;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("capacity state")
}

#[tokio::test]
async fn two_gateway_streams_admit_eight_stably_and_withdraw_only_the_oversized_service() {
    let dir = tempfile::tempdir().unwrap();
    let ids = ["a".repeat(64), "b".repeat(64)];
    let hub = Hub::embedded();
    hub.capacity
        .as_ref()
        .unwrap()
        .gateways(&ids.iter().cloned().collect());
    let (updates, _) = broadcast::channel::<Update>(16);
    let (actions, mut received) = mpsc::channel(4);
    let opens = Arc::new(Mutex::new(Vec::new()));
    let mut fixtures = Vec::new();
    let mut attached = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        let path = dir.path().join(format!("{index}.sock"));
        fixtures.push(
            Fixture::start(
                path.clone(),
                id.clone(),
                Duration::from_millis(if index == 0 { 30 } else { 0 }),
                updates.clone(),
                actions.clone(),
                opens.clone(),
            )
            .await,
        );
        let hub = hub.clone();
        let id = id.clone();
        attached.push(tokio::spawn(async move {
            hub.run_until(
                Connection::Local(omochi_local::LocalEndpoint::Unix { path }),
                Some(id),
                false,
                hub.stop.child_token(),
            )
            .await
        }));
    }
    let services = wait_for(&hub, |services| {
        services.len() == 12
            && services
                .iter()
                .filter(|s| s["status"] == "connected")
                .count()
                == 8
    })
    .await;
    let expected: Vec<_> = ids
        .iter()
        .flat_map(|id| (0..6).map(move |i| format!("{id}~app{i}")))
        .take(8)
        .collect();
    let admitted: Vec<_> = services
        .iter()
        .filter(|s| s["status"] == "connected")
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(admitted, expected);
    assert!(
        services
            .iter()
            .filter(|s| s["status"] == "error")
            .all(|s| s["detail"] == "service capacity reached")
    );
    let bad = format!("{}~app0", ids[0]);
    // Six independently valid 3 MiB Surface models exceed one service's 16 MiB
    // retained snapshot. Neither a whole wire frame nor a single model is oversized.
    for index in 0..6 {
        updates.send((bad.clone(),Arc::new(json!({"version":"v0.9","createSurface":{"surfaceId":format!("large{index}"),"catalogId":"test"}})))).unwrap();
        updates.send((bad.clone(),Arc::new(json!({"version":"v0.9","updateDataModel":{"surfaceId":format!("large{index}"),"path":"/","value":"x".repeat(3*1024*1024)}})))).unwrap();
    }
    wait_for(&hub, |services| {
        services
            .iter()
            .any(|s| s["id"] == bad && s["detail"] == "snapshot capacity reached")
            && services
                .iter()
                .filter(|s| s["status"] == "connected")
                .count()
                == 8
    })
    .await;
    assert_eq!(
        opens
            .lock()
            .await
            .iter()
            .filter(|route| **route == bad)
            .count(),
        1
    );
    let healthy = format!("{}~app1/main", ids[0]);
    let mut subscription = hub.subscribe_scoped().await.unwrap();
    let handle = loop {
        let batch = subscription.next_batch().await.unwrap();
        subscription.acknowledge(batch.sequence).unwrap();
        if let Some(handle) = &batch.handle
            && handle.surface_id() == healthy
        {
            break handle.clone();
        }
        assert!(!batch.snapshot_end, "healthy service disappeared");
    };
    handle
        .action(json!({"version":"v0.9","action":{"name":"click"}}))
        .await
        .unwrap();
    let (route, action) = tokio::time::timeout(Duration::from_secs(3), received.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(route, format!("{}~app1", ids[0]));
    assert_eq!(action["action"]["surfaceId"], "main");
    drop(subscription);
    hub.shutdown();
    for task in attached {
        task.await.unwrap().unwrap();
    }
    for fixture in fixtures {
        fixture.close().await;
    }
    assert!(hub.services().await.is_empty());
}
