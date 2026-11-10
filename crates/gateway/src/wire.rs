//! Gateway stream dispatch shared by Unix sockets and authenticated iroh connections.
use crate::Gateway;
use crate::dispatch::CommandOrigin;
use anyhow::{Context, Result, bail};
use omochi_protocol::{
    OpenStream, VERSION, read_frame,
    session::{Control, ServiceFrame},
    write_frame,
};
use serde::Serialize;
use serde_json::Value;

use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{OwnedSemaphorePermit, Semaphore, mpsc},
    task::{JoinHandle, JoinSet},
};

// Headers are bounded separately so long-lived subscriptions cannot starve
// control or resources, and clients cannot create unbounded pending reads.
const MAX_PENDING_STREAMS: usize = 64;
const MAX_RESOURCE_STREAMS: usize = 64;
const MAX_LOCAL_HUBS: usize = 8;
pub(crate) const MAX_REMOTE_STREAMS: u32 =
    (1 + crate::MAX_SERVICES + MAX_RESOURCE_STREAMS + MAX_PENDING_STREAMS) as u32;

#[derive(Clone)]
pub(crate) struct StreamLimits {
    pending: Arc<Semaphore>,
    control: Arc<Semaphore>,
    service: Arc<Semaphore>,
    resource: Arc<Semaphore>,
}
impl StreamLimits {
    pub(crate) fn new(hubs: usize) -> Self {
        Self {
            pending: Arc::new(Semaphore::new(MAX_PENDING_STREAMS)),
            control: Arc::new(Semaphore::new(hubs)),
            service: Arc::new(Semaphore::new(hubs * crate::MAX_SERVICES)),
            resource: Arc::new(Semaphore::new(MAX_RESOURCE_STREAMS)),
        }
    }
    pub(crate) fn admit(&self) -> Result<StreamAdmission> {
        Ok(StreamAdmission {
            limits: self.clone(),
            _pending: self.pending.clone().try_acquire_owned()?,
        })
    }
}
pub(crate) struct StreamAdmission {
    limits: StreamLimits,
    _pending: OwnedSemaphorePermit,
}
impl StreamAdmission {
    fn classify(self, open: &OpenStream) -> Result<OwnedSemaphorePermit> {
        let slots = match open {
            OpenStream::Control { .. } => &self.limits.control,
            OpenStream::Service { .. } => &self.limits.service,
            OpenStream::Resource { .. } => &self.limits.resource,
        };
        Ok(slots.clone().try_acquire_owned()?)
    }
}

async fn send<W: AsyncWrite + Unpin, T: Serialize>(writer: &mut W, value: &T) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), write_frame(writer, value)).await??;
    Ok(())
}
impl Gateway {
    /// Remote callers must authenticate the EndpointId and enforce role authorization
    /// before dispatching. Local callers use the private Unix socket trust boundary.
    pub async fn serve_stream<R, W>(
        &self,
        reader: R,
        writer: W,
        node_id: &str,
        name: &str,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin + Send,
        W: AsyncWrite + Unpin + Send,
    {
        self.serve_stream_from(reader, writer, node_id, name, CommandOrigin::Local, None)
            .await
    }
    pub(crate) async fn serve_stream_from<R, W>(
        &self,
        mut reader: R,
        mut writer: W,
        node_id: &str,
        name: &str,
        origin: CommandOrigin,
        admission: Option<StreamAdmission>,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin + Send,
        W: AsyncWrite + Unpin + Send,
    {
        let open: OpenStream =
            tokio::time::timeout(Duration::from_secs(5), read_frame(&mut reader))
                .await??
                .context("missing stream header")?;
        let _permit = admission
            .map(|admission| admission.classify(&open))
            .transpose()?;
        match open {
            OpenStream::Control { protocol } => {
                if protocol != VERSION {
                    bail!("incompatible gateway protocol; stop the legacy gateway")
                }
                let mut changes = self.changes();
                send(
                    &mut writer,
                    &Control::Hello {
                        protocol: VERSION,
                        node_id: node_id.into(),
                        name: name.into(),
                    },
                )
                .await?;
                send(
                    &mut writer,
                    &Control::Services {
                        services: self.list().await,
                    },
                )
                .await?;
                let (tx, mut rx) = mpsc::channel(8);
                let read = async {
                    while let Some(frame) = read_frame::<_, Control>(&mut reader).await? {
                        match frame {
                            Control::Ping { id } => tx
                                .send(id)
                                .await
                                .map_err(|_| anyhow::anyhow!("control stream closed"))?,
                            _ => bail!("unexpected client control frame"),
                        }
                    }
                    Ok::<_, anyhow::Error>(())
                };
                let write = async {
                    loop {
                        tokio::select! {
                            change = changes.recv() => {
                                match change { Ok(()) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}, Err(_) => return Ok(()) }
                                send(&mut writer, &Control::Services { services: self.list().await }).await?;
                            }
                            id = rx.recv() => { let Some(id) = id else { return Ok(()) }; send(&mut writer, &Control::Pong { id }).await?; }
                        }
                    }
                };
                tokio::select! { result = read => result, result = write => result, _ = self.stop.cancelled() => Ok(()) }
            }
            OpenStream::Service {
                service,
                generation,
            } => {
                let subscription = self.subscribe(&service, generation).await?;
                send(&mut writer, &ServiceFrame::SnapshotStart).await?;
                for message in subscription.snapshot {
                    send(&mut writer, &ServiceFrame::Message { message }).await?;
                }
                send(&mut writer, &ServiceFrame::SnapshotEnd).await?;
                let mut events = subscription.events;
                let read = async {
                    while let Some(action) = read_frame::<_, Value>(&mut reader).await? {
                        self.action_from(&service, generation, action, origin.clone())
                            .await?;
                    }
                    Ok::<_, anyhow::Error>(())
                };
                let write = async {
                    loop {
                        let message = events.recv().await.context(
                            "service closed or subscriber lagged; reconnect for a fresh snapshot",
                        )?;
                        send(
                            &mut writer,
                            &ServiceFrame::Message {
                                message: (*message).clone(),
                            },
                        )
                        .await?;
                    }
                    #[allow(unreachable_code)]
                    Ok::<_, anyhow::Error>(())
                };
                tokio::select! { result = read => result, result = write => result, _ = self.stop.cancelled() => Ok(()) }
            }
            OpenStream::Resource {
                service,
                generation,
            } => {
                let request = tokio::time::timeout(
                    Duration::from_secs(5),
                    read_frame::<_, Value>(&mut reader),
                )
                .await??
                .context("missing resource request")?;
                let response = self
                    .resource_from(&service, generation, request, origin)
                    .await?;
                send(&mut writer, &response).await
            }
        }
    }
    /// Never unlinks an existing socket. A running legacy gateway must be stopped explicitly.
    pub async fn listen_local(
        &self,
        path: PathBuf,
        node_id: String,
        name: String,
    ) -> Result<JoinHandle<Result<()>>> {
        let runtime = omochi_local::LocalRuntime::open(path)?;
        let listener = runtime.publish_gateway().await?;
        let gateway = self.clone();
        Ok(tokio::spawn(async move {
            let limits = StreamLimits::new(MAX_LOCAL_HUBS);
            let mut tasks = JoinSet::new();
            let result = loop {
                tokio::select! {
                    _ = gateway.stop.cancelled() => break Ok(()),
                    Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
                    accepted = listener.accept() => {
                        let socket = match accepted { Ok(value) => value, Err(error) => break Err(error.into()) };
                        let Ok(admission) = limits.admit() else { continue };
                        let child = gateway.clone(); let node = node_id.clone(); let name = name.clone();
                        tasks.spawn(async move { let (reader, writer) = socket.into_split(); child.serve_stream_from(reader, writer, &node, &name, CommandOrigin::Local, Some(admission)).await });
                    }
                }
            };
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
            listener.stop().await;
            drop(listener);
            result
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stream_budgets_are_independent_bounded_and_released() {
        let limits = StreamLimits::new(1);
        let service = OpenStream::Service {
            service: "app".into(),
            generation: 1,
        };
        let resource = OpenStream::Resource {
            service: "app".into(),
            generation: 1,
        };
        let control = OpenStream::Control { protocol: VERSION };
        let mut subscriptions = (0..crate::MAX_SERVICES)
            .map(|_| limits.admit().unwrap().classify(&service).unwrap())
            .collect::<Vec<_>>();
        assert!(limits.admit().unwrap().classify(&service).is_err());
        let requests = (0..MAX_RESOURCE_STREAMS)
            .map(|_| limits.admit().unwrap().classify(&resource).unwrap())
            .collect::<Vec<_>>();
        assert!(limits.admit().unwrap().classify(&resource).is_err());
        let controller = limits.admit().unwrap().classify(&control).unwrap();
        assert!(limits.admit().unwrap().classify(&control).is_err());
        // Successful/failed classification returns the header reservation.
        let mut pending = (0..MAX_PENDING_STREAMS)
            .map(|_| limits.admit().unwrap())
            .collect::<Vec<_>>();
        assert!(limits.admit().is_err());
        drop(pending.pop());
        drop(subscriptions.pop());
        let replacement = limits.admit().unwrap().classify(&service).unwrap();
        drop((pending, subscriptions, requests, controller, replacement));
        for open in [service, resource, control] {
            assert!(limits.admit().unwrap().classify(&open).is_ok());
        }
    }

    #[tokio::test]
    async fn unix_gateway_handshake_ping_and_owned_socket_cleanup() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        omochi_local::LocalRuntime::prepare(Some(root.path().to_path_buf())).unwrap();
        let path = root.path().join("gateway.sock");
        let gateway = Gateway::new();
        let id = "a".repeat(64);
        let task = gateway
            .listen_local(
                (path.clone()).parent().unwrap().to_path_buf(),
                id.clone(),
                "test".into(),
            )
            .await
            .unwrap();
        assert!(
            gateway
                .listen_local(
                    (path.clone()).parent().unwrap().to_path_buf(),
                    id.clone(),
                    "duplicate".into()
                )
                .await
                .is_err()
        );
        let mut socket = tokio::net::UnixStream::connect(&path).await.unwrap();
        write_frame(&mut socket, &OpenStream::Control { protocol: VERSION })
            .await
            .unwrap();
        match read_frame::<_, Control>(&mut socket)
            .await
            .unwrap()
            .unwrap()
        {
            Control::Hello {
                node_id, protocol, ..
            } => {
                assert_eq!(node_id, id);
                assert_eq!(protocol, VERSION);
            }
            _ => panic!("expected hello"),
        }
        assert!(
            matches!(read_frame::<_, Control>(&mut socket).await.unwrap().unwrap(), Control::Services { services } if services.is_empty())
        );
        write_frame(&mut socket, &Control::Ping { id: 42 })
            .await
            .unwrap();
        assert!(matches!(
            read_frame::<_, Control>(&mut socket)
                .await
                .unwrap()
                .unwrap(),
            Control::Pong { id: 42 }
        ));
        gateway.shutdown();
        task.await.unwrap().unwrap();
        assert!(!path.exists());
    }
    #[tokio::test]
    async fn rejects_legacy_protocol_before_listing_services() {
        let gateway = Gateway::new();
        let (mut client, server) = tokio::io::duplex(256);
        let (read, write) = tokio::io::split(server);
        let task = tokio::spawn(async move {
            gateway
                .serve_stream(read, write, &"a".repeat(64), "test")
                .await
        });
        write_frame(&mut client, &OpenStream::Control { protocol: 3 })
            .await
            .unwrap();
        assert!(task.await.unwrap().is_err());
        assert!(
            read_frame::<_, Control>(&mut client)
                .await
                .unwrap()
                .is_none()
        );
    }
}
