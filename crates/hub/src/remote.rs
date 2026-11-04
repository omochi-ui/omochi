//! Registered gateway supervision; an offline peer never blocks the local dashboard.
use crate::{
    Connection, Hub,
    host::{AuthorizationSource, RegisteredPeer},
};
use omochi_protocol::ALPN;
#[cfg(feature = "desktop")]
use omochi_transport::store::Store;
use omochi_transport::{Endpoint, EndpointId, trust::Role};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use tokio::{sync::Mutex, task::JoinHandle};
use tokio_util::sync::CancellationToken;

pub type Diagnostics = Arc<Mutex<BTreeMap<String, String>>>;
impl Hub {
    #[cfg(feature = "desktop")]
    pub fn attach_registered(
        &self,
        endpoint: Endpoint,
        store: Store,
    ) -> (JoinHandle<()>, Diagnostics) {
        let hub = self.clone();
        let diagnostics = self.diagnostics.clone();
        let task = tokio::spawn(async move {
            let source = match crate::host::DesktopAuthorization::open(store).await {
                Ok(source) => source,
                Err(error) => {
                    hub.diagnostics
                        .lock()
                        .await
                        .insert("authorization".into(), error.to_string());
                    endpoint.close().await;
                    return;
                }
            };
            if let Err(error) = hub.supervise(endpoint, Arc::new(source)).await {
                hub.diagnostics
                    .lock()
                    .await
                    .insert("authorization".into(), error.to_string());
            }
        });
        (task, diagnostics)
    }

    pub fn attach_authorization(
        &self,
        endpoint: Endpoint,
        source: Arc<dyn AuthorizationSource>,
    ) -> (JoinHandle<anyhow::Result<()>>, Diagnostics) {
        let hub = self.clone();
        let diagnostics = self.diagnostics.clone();
        let task = tokio::spawn(async move { hub.supervise(endpoint, source).await });
        (task, diagnostics)
    }

    async fn supervise(
        &self,
        endpoint: Endpoint,
        source: Arc<dyn AuthorizationSource>,
    ) -> anyhow::Result<()> {
        let hub = self.clone();
        let status = self.diagnostics.clone();
        let mut peers: BTreeMap<String, (RegisteredPeer, CancellationToken, JoinHandle<()>)> =
            BTreeMap::new();
        let mut revoked = 0;
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        let mut failure = None;
        loop {
            tokio::select! { _ = hub.stop.cancelled() => break, _ = interval.tick() => {} }
            let loaded = tokio::select! {
                _ = hub.stop.cancelled() => break,
                result = source.load() => result,
            };
            let (generation, next_revoked, next) = match loaded {
                Ok(snapshot) => (
                    Some(snapshot.generation),
                    snapshot.revocation,
                    snapshot
                        .peers
                        .into_iter()
                        .filter(|(_, p)| p.peer.role == Role::Gateway)
                        .collect::<BTreeMap<_, _>>(),
                ),
                _ => (None, revoked, BTreeMap::new()), // invalid/unreadable trust fails closed
            };
            status
                .lock()
                .await
                .retain(|id, _| id == "authorization" || next.contains_key(id));
            hub.peer_names
                .lock()
                .await
                .retain(|id, _| next.contains_key(id));
            let removed: Vec<_> = peers
                .iter()
                .filter(|(id, (peer, _, _))| next_revoked != revoked || next.get(*id) != Some(peer))
                .map(|(id, _)| id.clone())
                .collect();
            let mut teardown_failed = false;
            for id in removed {
                let (_, stop, task) = peers.remove(&id).unwrap();
                stop.cancel();
                if task.await.is_err() {
                    teardown_failed = true;
                }
                if hub.images.join_revoked_gateway(&id).await.is_err() {
                    teardown_failed = true;
                }
                status.lock().await.remove(&id);
                hub.peer_names.lock().await.remove(&id);
            }
            if teardown_failed {
                status.lock().await.insert(
                    "authorization".into(),
                    "peer teardown failed; Hub stopped".into(),
                );
                hub.shutdown();
                failure = Some(anyhow::anyhow!("peer teardown failed"));
                break;
            }
            revoked = next_revoked;
            if let Some(generation) = generation
                && {
                    omochi_transport::blocking::drain().await;
                    source.acknowledge(generation).await.is_err()
                }
            {
                failure = Some(anyhow::anyhow!("authorization acknowledgement failed"));
                break;
            }
            let mut admitted: BTreeSet<String> = peers.keys().cloned().collect();
            if let Some(capacity) = &hub.capacity {
                for id in next.keys() {
                    if admitted.len() >= crate::capacity::MAX_GATEWAYS {
                        break;
                    }
                    admitted.insert(id.clone());
                }
                capacity.gateways(&admitted);
            } else {
                admitted.extend(next.keys().cloned());
            }
            for (id, peer) in next {
                if peers.contains_key(&id) {
                    continue;
                }
                if !admitted.contains(&id) {
                    hub.peer_names
                        .lock()
                        .await
                        .insert(id.clone(), peer.peer.name.clone());
                    status
                        .lock()
                        .await
                        .insert(id, "gateway capacity reached".into());
                    continue;
                }
                let Ok(target) = id.parse::<EndpointId>() else {
                    continue;
                };
                if peer
                    .address
                    .as_ref()
                    .is_some_and(|address| address.id != target)
                {
                    status
                        .lock()
                        .await
                        .insert(id, "invalid registered address identity".into());
                    continue;
                }
                hub.peer_names
                    .lock()
                    .await
                    .insert(id.clone(), peer.peer.name.clone());
                let stop = hub.stop.child_token();
                let token = stop.clone();
                let hub = hub.clone();
                let ep = endpoint.clone();
                let status = status.clone();
                let key = id.clone();
                let address = peer.address.clone().unwrap_or_else(|| target.into());
                let task = tokio::spawn(async move {
                    let mut attempt: u32 = 0;
                    loop {
                        if token.is_cancelled() {
                            break;
                        }
                        status.lock().await.insert(key.clone(), "connecting".into());
                        let connection = tokio::select! { _ = token.cancelled() => break, result = tokio::time::timeout(Duration::from_secs(30), ep.connect(address.clone(), ALPN)) => result };
                        match connection {
                            Ok(Ok(connection)) => {
                                status
                                    .lock()
                                    .await
                                    .insert(key.clone(), "transport connected".into());
                                let result = hub
                                    .run_until(
                                        Connection::Iroh(connection.clone()),
                                        Some(key.clone()),
                                        false,
                                        token.child_token(),
                                    )
                                    .await;
                                connection.close(0u32.into(), b"hub session ended");
                                status.lock().await.insert(
                                    key.clone(),
                                    result
                                        .err()
                                        .map(|e| e.to_string())
                                        .unwrap_or_else(|| "disconnected".into()),
                                );
                            }
                            Ok(Err(error)) => {
                                status
                                    .lock()
                                    .await
                                    .insert(key.clone(), format!("connect failed: {error}"));
                            }
                            Err(_) => {
                                status
                                    .lock()
                                    .await
                                    .insert(key.clone(), "connection timed out".into());
                            }
                        }
                        if let Some(capacity) = &hub.capacity {
                            // A failed/revoked standby must not clear the roster
                            // of another live path to the same gateway.
                            let state = hub.state.lock().await;
                            if !state.gateway_owners.contains_key(&key) {
                                capacity.offline(&key);
                            }
                        }
                        attempt = attempt.saturating_add(1);
                        let jitter = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .subsec_nanos() as u64
                            % 200;
                        let delay = (100u64 << attempt.min(6)).min(5000) + jitter;
                        tokio::select! { _ = token.cancelled() => break, _ = tokio::time::sleep(Duration::from_millis(delay)) => {} }
                    }
                });
                peers.insert(id, (peer, stop, task));
            }
        }
        for (id, (_, stop, task)) in peers {
            stop.cancel();
            if task.await.is_err() {
                failure = Some(anyhow::anyhow!("peer supervisor failed during shutdown"));
            }
            if hub.images.join_revoked_gateway(&id).await.is_err() {
                status
                    .lock()
                    .await
                    .insert("authorization".into(), "image teardown failed".into());
                failure = Some(anyhow::anyhow!("image teardown failed"));
            }
        }
        endpoint.close().await;
        if let Some(error) = failure {
            Err(error)
        } else {
            Ok(())
        }
    }
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    use super::*;
    use omochi_transport::store::Role;
    #[tokio::test]
    async fn revocation_closes_active_resource_and_retracts_surfaces_before_ack() {
        use omochi_protocol::{
            OpenStream, VERSION, read_frame,
            session::{Control, Service, ServiceFrame},
            write_frame,
        };
        use serde_json::{Value, json};
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().join("hub")).unwrap();
        let other = Store::open(root.path().join("gateway")).unwrap();
        let endpoint = omochi_transport::bind(store.identity(Role::Hub).unwrap(), true, None)
            .await
            .unwrap();
        let gateway = omochi_transport::bind(other.identity(Role::Gateway).unwrap(), true, None)
            .await
            .unwrap();
        let peer = gateway.id();
        store.register_gateway(peer, "workstation").unwrap();
        // Seed a direct address without depending on multicast in ordinary CI.
        let address = gateway.addr();
        let warm = tokio::spawn({
            let endpoint = endpoint.clone();
            async move { endpoint.connect(address, ALPN).await.unwrap() }
        });
        let incoming = gateway.accept().await.unwrap().await.unwrap();
        let warmed = warm.await.unwrap();
        warmed.close(0u32.into(), b"address seeded");
        incoming.closed().await;
        let (requested, request_seen) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let connection = gateway.accept().await.unwrap().await.unwrap();
            let mut requested = Some(requested);
            let mut streams = tokio::task::JoinSet::new();
            loop {
                let stream = tokio::select! {
                    _ = connection.closed() => break,
                    stream = connection.accept_bi() => stream,
                };
                let Ok((mut tx, mut rx)) = stream else { break };
                match read_frame::<_, OpenStream>(&mut rx).await.unwrap().unwrap() {
                    OpenStream::Control { .. } => {
                        write_frame(
                            &mut tx,
                            &Control::Hello {
                                protocol: VERSION,
                                node_id: peer.to_string(),
                                name: "gateway".into(),
                            },
                        )
                        .await
                        .unwrap();
                        write_frame(
                            &mut tx,
                            &Control::Services {
                                services: vec![Service {
                                    id: "app".into(),
                                    name: "App".into(),
                                    generation: 1,
                                    icon: None,
                                }],
                            },
                        )
                        .await
                        .unwrap();
                        streams.spawn(async move {
                            while let Ok(Some(Control::Ping { id })) = read_frame(&mut rx).await {
                                if write_frame(&mut tx, &Control::Pong { id }).await.is_err() {
                                    break;
                                }
                            }
                        });
                    }
                    OpenStream::Service { .. } => {
                        write_frame(&mut tx, &ServiceFrame::SnapshotStart)
                            .await
                            .unwrap();
                        write_frame(&mut tx, &ServiceFrame::Message { message: json!({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"}}) }).await.unwrap();
                        write_frame(&mut tx, &ServiceFrame::SnapshotEnd)
                            .await
                            .unwrap();
                        streams.spawn(async move {
                            let _tx = tx;
                            let _ = read_frame::<_, Value>(&mut rx).await;
                        });
                    }
                    OpenStream::Resource { .. } => {
                        let _ = read_frame::<_, Value>(&mut rx).await.unwrap().unwrap();
                        requested.take().unwrap().send(()).unwrap();
                        streams.spawn(async move {
                            let _tx = tx;
                            let _ = read_frame::<_, Value>(&mut rx).await;
                        });
                    }
                }
            }
            streams.abort_all();
            while streams.join_next().await.is_some() {}
            gateway.close().await;
        });
        let hub = Hub::new();
        let (supervisor, _) = hub.attach_registered(endpoint, store.clone());
        tokio::time::timeout(Duration::from_secs(5), async {
            while hub.services().await.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let resource = tokio::spawn({
            let hub = hub.clone();
            async move {
                hub.resource(&format!("{peer}~app"), json!({"op":"stat"}))
                    .await
            }
        });
        tokio::time::timeout(Duration::from_secs(3), request_seen)
            .await
            .unwrap()
            .unwrap();
        omochi_transport::live::forget(store, peer, Duration::from_secs(3))
            .await
            .unwrap();
        assert!(hub.services().await.is_empty());
        assert!(hub.subscribe().await.snapshot.is_empty());
        assert!(
            tokio::time::timeout(Duration::from_secs(1), resource)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
        hub.shutdown();
        supervisor.await.unwrap();
    }
    #[tokio::test]
    async fn revocation_cancels_pending_dial_before_acknowledging() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().join("hub")).unwrap();
        let other = Store::open(root.path().join("other")).unwrap();
        let peer = other.identity(Role::Gateway).unwrap().public();
        store.register_gateway(peer, "offline").unwrap();
        let endpoint = omochi_transport::bind(store.identity(Role::Hub).unwrap(), true, None)
            .await
            .unwrap();
        let hub = Hub::new();
        let (task, status) = hub.attach_registered(endpoint, store.clone());
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if status.lock().await.contains_key(&peer.to_string()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let connections = hub.connection_status().await;
        assert_eq!(connections[0]["name"], "offline");
        assert_eq!(connections[0]["nodeId"], peer.to_string());
        assert_eq!(connections[0]["status"], "connecting");
        omochi_transport::live::forget(store, peer, Duration::from_secs(3))
            .await
            .unwrap();
        assert!(!status.lock().await.contains_key(&peer.to_string()));
        assert!(hub.connection_status().await.is_empty());
        hub.shutdown();
        task.await.unwrap();
    }
}
