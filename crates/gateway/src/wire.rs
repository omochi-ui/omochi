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
use std::os::unix::fs::MetadataExt;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::UnixListener,
    sync::{Semaphore, mpsc},
    task::{JoinHandle, JoinSet},
};

struct OwnedSocket {
    path: PathBuf,
    device: u64,
    inode: u64,
}
impl Drop for OwnedSocket {
    fn drop(&mut self) {
        if std::fs::symlink_metadata(&self.path)
            .is_ok_and(|m| m.dev() == self.device && m.ino() == self.inode)
        {
            let _ = std::fs::remove_file(&self.path);
        }
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
        self.serve_stream_from(reader, writer, node_id, name, CommandOrigin::Local)
            .await
    }
    pub(crate) async fn serve_stream_from<R, W>(
        &self,
        mut reader: R,
        mut writer: W,
        node_id: &str,
        name: &str,
        origin: CommandOrigin,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin + Send,
        W: AsyncWrite + Unpin + Send,
    {
        let open: OpenStream =
            tokio::time::timeout(Duration::from_secs(5), read_frame(&mut reader))
                .await??
                .context("missing stream header")?;
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
    pub fn listen_unix(
        &self,
        path: PathBuf,
        node_id: String,
        name: String,
    ) -> Result<JoinHandle<Result<()>>> {
        let parent = path
            .parent()
            .context("gateway socket needs a parent directory")?;
        omochi_local_discovery::scan(parent)?; // also checks the local trust boundary
        let listener = UnixListener::bind(&path).context(
            "gateway socket is already present; stop any existing gateway before retrying",
        )?;
        let metadata = std::fs::symlink_metadata(&path)?;
        let owned_socket = OwnedSocket {
            path,
            device: metadata.dev(),
            inode: metadata.ino(),
        };
        let gateway = self.clone();
        Ok(tokio::spawn(async move {
            let _owned_socket = owned_socket;
            let limit = Arc::new(Semaphore::new(64));
            let mut tasks = JoinSet::new();
            let result = loop {
                tokio::select! {
                    _ = gateway.stop.cancelled() => break Ok(()),
                    Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
                    accepted = listener.accept() => {
                        let (socket, _) = match accepted { Ok(value) => value, Err(error) => break Err(error.into()) };
                        let Ok(permit) = limit.clone().try_acquire_owned() else { continue };
                        let child = gateway.clone(); let node = node_id.clone(); let name = name.clone();
                        tasks.spawn(async move { let _permit = permit; let (reader, writer) = socket.into_split(); child.serve_stream(reader, writer, &node, &name).await });
                    }
                }
            };
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
            drop(listener);
            result
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unix_gateway_handshake_ping_and_owned_socket_cleanup() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.path().join("gateway.sock");
        let gateway = Gateway::new();
        let id = "a".repeat(64);
        let task = gateway
            .listen_unix(path.clone(), id.clone(), "test".into())
            .unwrap();
        assert!(
            gateway
                .listen_unix(path.clone(), id.clone(), "duplicate".into())
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
