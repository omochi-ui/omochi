//! Remote admission and dispatch. Endpoint authentication alone is never authorization.
use crate::Gateway;
use crate::dispatch::{AuthorizationEpoch, CommandOrigin};
use crate::wire::{MAX_REMOTE_STREAMS, StreamLimits};
use anyhow::{Result, bail};
use omochi_protocol::{ALPN, PAIR_ALPN};
use omochi_transport::{
    Endpoint, EndpointConnection,
    store::{Role, Store},
};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::Semaphore,
    task::{JoinHandle, JoinSet},
};

async fn authorized(store: &Store, connection: &EndpointConnection) -> Result<bool> {
    let store = store.clone();
    let peer = connection.remote_id().to_string();
    omochi_transport::blocking::run(move || {
        Ok(store
            .peers()?
            .1
            .get(&peer)
            .is_some_and(|p| p.role == Role::Hub))
    })
    .await?
}
impl Gateway {
    /// Async hosts also offload identity reads and lease registration during startup.
    pub async fn listen_remote_async(
        &self,
        endpoint: Endpoint,
        store: Store,
        name: String,
    ) -> Result<JoinHandle<()>> {
        let gateway = self.clone();
        omochi_transport::blocking::run(move || gateway.listen_remote(endpoint, store, name))
            .await?
    }
    pub fn listen_remote(
        &self,
        endpoint: Endpoint,
        store: Store,
        name: String,
    ) -> Result<JoinHandle<()>> {
        if endpoint.id() != store.identity(Role::Gateway)?.public() {
            bail!("gateway endpoint identity mismatch")
        }
        let lease = Arc::new(omochi_transport::live::Lease::register(&store)?);
        let (_, mut revoked, _) = store.authorization()?;
        let gateway = self.clone();
        Ok(tokio::spawn(async move {
            let connections = Arc::new(Semaphore::new(64));
            let pairing = Arc::new(Semaphore::new(8));
            let mut tasks = JoinSet::new();
            let mut sessions = gateway.stop.child_token();
            let mut epoch = AuthorizationEpoch::new();
            let mut poll = tokio::time::interval(Duration::from_millis(100));
            loop {
                tokio::select! {
                    _ = gateway.stop.cancelled() => break,
                    _ = poll.tick() => {
                        let copy = store.clone();
                        let state = omochi_transport::blocking::run(move || copy.authorization()).await;
                        let (generation, next_revoked, _) = match state {
                            Ok(Ok(state)) => state,
                            _ => break, // Fail closed without acknowledging unreadable authorization.
                        };
                        if next_revoked != revoked {
                            epoch.cancel();
                            sessions.cancel();
                            while tasks.join_next().await.is_some() {}
                            epoch.drain().await;
                            omochi_transport::blocking::drain().await;
                        }
                        let changed = next_revoked != revoked;
                        revoked = next_revoked;
                        let copy = lease.clone();
                        if !matches!(omochi_transport::blocking::run(move || copy.acknowledge(generation)).await, Ok(Ok(()))) { break; }
                        if changed {
                            sessions = gateway.stop.child_token();
                            epoch = AuthorizationEpoch::new();
                        }
                    },
                    Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
                    incoming = endpoint.accept() => {
                        let Some(incoming) = incoming else { break };
                        let Ok(permit) = connections.clone().try_acquire_owned() else { incoming.refuse(); continue };
                        let gateway = gateway.clone(); let store = store.clone(); let pairing = pairing.clone(); let name = name.clone(); let node = endpoint.id().to_string();
                        let stop = sessions.child_token();
                        let origin = CommandOrigin::Remote { epoch: epoch.clone(), session: stop.clone() };
                        tasks.spawn(async move {
                            let _permit = permit;
                            let _cancel_on_exit = stop.clone().drop_guard();
                            let connection = tokio::select! { _ = stop.cancelled() => return Ok(()), result = tokio::time::timeout(Duration::from_secs(10), incoming) => result?? };
                            if connection.alpn() == PAIR_ALPN {
                                let Ok(_pairing) = pairing.try_acquire_owned() else { connection.close(1u32.into(), b"enrollment busy"); return Ok(()) };
                                return tokio::select! { _ = stop.cancelled() => { connection.close(0u32.into(), b"authorization changed"); Ok(()) }, result = omochi_transport::pairing::accept(connection.clone(), store) => result }
                            }
                            if connection.alpn() != ALPN || !authorized(&store, &connection).await? {
                                connection.close(1u32.into(), b"not authorized"); bail!("peer not authorized")
                            }
                            let result = gateway.remote_session(&connection, &store, &node, &name, &stop, origin).await;
                            connection.close(0u32.into(), b"session ended"); result
                        });
                    }
                }
            }
            epoch.cancel();
            sessions.cancel();
            while tasks.join_next().await.is_some() {}
            epoch.drain().await;
            omochi_transport::blocking::drain().await;
            endpoint.close().await;
        }))
    }
    async fn remote_session(
        &self,
        connection: &EndpointConnection,
        store: &Store,
        node: &str,
        name: &str,
        stop: &tokio_util::sync::CancellationToken,
        origin: CommandOrigin,
    ) -> Result<()> {
        let mut streams = JoinSet::new();
        // Raise QUIC credit only after ordinary gateway authorization. Pairing and
        // unauthenticated connections keep the transport's existing defaults.
        connection.set_max_concurrent_bi_streams(MAX_REMOTE_STREAMS.into());
        let limits = StreamLimits::new(1);
        let mut poll = tokio::time::interval(Duration::from_millis(100));
        let result = async { loop {
            tokio::select! {
                _ = stop.cancelled() => return Ok(()),
                Some(_) = streams.join_next(), if !streams.is_empty() => {},
                _ = poll.tick() => {
                    if !authorized(store, connection).await? { bail!("authorization revoked") }
                }
                stream = connection.accept_bi() => {
                    let (mut writer, mut reader) = stream?;
                    if !authorized(store, connection).await? { bail!("authorization revoked") }
                    let Ok(admission) = limits.admit() else { let _ = writer.reset(2u32.into()); let _ = reader.stop(2u32.into()); continue };
                    let gateway = self.clone(); let node = node.to_owned(); let name = name.to_owned();
                    let origin = origin.clone();
                    streams.spawn(async move { gateway.serve_stream_from(reader, writer, &node, &name, origin, Some(admission)).await });
                }
            }
        } }.await;
        stop.cancel();
        connection.close(0u32.into(), b"session ended");
        streams.abort_all();
        while streams.join_next().await.is_some() {}
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omochi_protocol::{OpenStream, VERSION, read_frame, session::Control, write_frame};
    #[tokio::test]
    async fn forget_acknowledgement_waits_for_an_already_dispatched_sdk_write() {
        use futures_util::SinkExt;
        use omochi_protocol::session::ServiceFrame;
        use serde_json::{Value, json};
        use std::os::unix::fs::PermissionsExt;
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
        use tokio::net::UnixListener;
        use tokio_util::codec::{Framed, LinesCodec};

        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join("runtime");
        std::fs::create_dir(&runtime).unwrap();
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
        let service_dir = runtime.join("app");
        std::fs::create_dir(&service_dir).unwrap();
        std::fs::write(
            service_dir.join("manifest.json"),
            r#"{"name":"App","socket":"a2ui.sock"}"#,
        )
        .unwrap();
        let listener = UnixListener::bind(service_dir.join("a2ui.sock")).unwrap();
        let gateway = Gateway::new();
        let discovery = gateway.discover(runtime);
        let (sdk, _) = listener.accept().await.unwrap();
        let mut sdk = Framed::new(sdk, LinesCodec::new());
        sdk.send(
            json!({"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"test"}})
                .to_string(),
        )
        .await
        .unwrap();
        let service = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(service) = gateway.list().await.first().cloned()
                    && !gateway
                        .subscribe("app", service.generation)
                        .await
                        .unwrap()
                        .snapshot
                        .is_empty()
                {
                    break service;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let mut sdk = sdk.into_inner();
        let store = Store::open(dir.path().join("gateway")).unwrap();
        let hub_store = Store::open(dir.path().join("hub")).unwrap();
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
            .listen_remote(endpoint, store.clone(), "gateway".into())
            .unwrap();
        let connection = hub.connect(address, ALPN).await.unwrap();
        let (mut tx, mut rx) = connection.open_bi().await.unwrap();
        write_frame(
            &mut tx,
            &OpenStream::Service {
                service: "app".into(),
                generation: service.generation,
            },
        )
        .await
        .unwrap();
        while !matches!(
            read_frame::<_, ServiceFrame>(&mut rx)
                .await
                .unwrap()
                .unwrap(),
            ServiceFrame::SnapshotEnd
        ) {}
        let first = json!({"version":"v0.9","action":{"surfaceId":"main","name":"dispatched","payload":"x".repeat(1024 * 1024)}});
        write_frame(&mut tx, &first).await.unwrap();
        let mut first_byte = [0u8; 1];
        tokio::time::timeout(Duration::from_secs(3), sdk.read_exact(&mut first_byte))
            .await
            .unwrap()
            .unwrap();
        write_frame(
            &mut tx,
            &json!({"version":"v0.9","action":{"surfaceId":"main","name":"revoked"}}),
        )
        .await
        .unwrap();
        let local = json!({"version":"v0.9","action":{"surfaceId":"main","name":"local"}});
        gateway
            .action("app", service.generation, local.clone())
            .await
            .unwrap();
        let forget =
            omochi_transport::live::forget(store.clone(), hub.id(), Duration::from_secs(3));
        tokio::pin!(forget);
        assert!(
            tokio::time::timeout(Duration::from_millis(150), &mut forget)
                .await
                .is_err(),
            "trust forget cannot acknowledge while the shared SDK write is blocked"
        );
        assert!(
            !store.peers().unwrap().1.contains_key(&hub.id().to_string()),
            "durable trust is removed even while acknowledgement waits"
        );
        let mut reader = BufReader::new(sdk);
        let mut line = vec![first_byte[0]];
        tokio::time::timeout(Duration::from_secs(3), reader.read_until(b'\n', &mut line))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&line).unwrap(), first);
        forget.await.unwrap();
        line.clear();
        tokio::time::timeout(Duration::from_secs(3), reader.read_until(b'\n', &mut line))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&line).unwrap(), local);
        gateway.shutdown();
        discovery.await.unwrap().unwrap();
        server.await.unwrap();
        hub.close().await;
    }
    #[tokio::test]
    async fn registered_hub_can_connect_and_revocation_closes_existing_connection() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("gateway")).unwrap();
        let hub_store = Store::open(dir.path().join("hub")).unwrap();
        let endpoint = omochi_transport::bind(store.identity(Role::Gateway).unwrap(), true, None)
            .await
            .unwrap();
        let hub = omochi_transport::bind(hub_store.identity(Role::Hub).unwrap(), true, None)
            .await
            .unwrap();
        let token = store.invite(0, u64::MAX).unwrap();
        store.redeem(&token, hub.id(), "hub", 1).unwrap();
        let address = endpoint.addr();
        let node = endpoint.id();
        let gateway = Gateway::new();
        let server = gateway
            .listen_remote(endpoint, store.clone(), "gateway".into())
            .unwrap();
        let connection = hub.connect(address, ALPN).await.unwrap();
        let (mut tx, mut rx) = connection.open_bi().await.unwrap();
        write_frame(&mut tx, &OpenStream::Control { protocol: VERSION })
            .await
            .unwrap();
        let hello = read_frame::<_, Control>(&mut rx).await.unwrap().unwrap();
        assert!(matches!(hello, Control::Hello { node_id, .. } if node_id == node.to_string()));
        omochi_transport::live::forget(store.clone(), hub.id(), Duration::from_secs(3))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), connection.closed())
            .await
            .unwrap();
        gateway.shutdown();
        server.await.unwrap();
        hub.close().await;
    }
    #[tokio::test]
    async fn unregistered_hub_gets_no_control_data() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("gateway")).unwrap();
        let other = Store::open(dir.path().join("other")).unwrap();
        let endpoint = omochi_transport::bind(store.identity(Role::Gateway).unwrap(), true, None)
            .await
            .unwrap();
        let hub = omochi_transport::bind(other.identity(Role::Hub).unwrap(), true, None)
            .await
            .unwrap();
        let address = endpoint.addr();
        let gateway = Gateway::new();
        let server = gateway
            .listen_remote(endpoint, store, "gateway".into())
            .unwrap();
        let connection = hub.connect(address, ALPN).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), connection.closed())
            .await
            .unwrap();
        gateway.shutdown();
        server.await.unwrap();
        hub.close().await;
    }
}
