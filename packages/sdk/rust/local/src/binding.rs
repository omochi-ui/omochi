//! Shared lifetime and cancellation machinery for language adapters.
use crate::{LocalRuntime, LocalStream, ServicePublication};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;

const CHUNK: usize = 64 * 1024;
struct Peer {
    parent: u32,
    read: Mutex<Option<ReadHalf<LocalStream>>>,
    write: Mutex<Option<WriteHalf<LocalStream>>>,
    stop: CancellationToken,
}
#[derive(Clone)]
enum Handle {
    Publication(Arc<ServicePublication>),
    Peer(Arc<Peer>),
}
#[derive(Default)]
pub struct Context {
    handles: Mutex<BTreeMap<u32, Handle>>,
    next: AtomicU32,
    closing: AtomicBool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    op: String,
    #[serde(default)]
    id: u32,
    runtime_dir: Option<PathBuf>,
    service_id: Option<String>,
    name: Option<String>,
    icon: Option<String>,
    endpoint: Option<crate::LocalEndpoint>,
}
impl Context {
    fn next_id(&self) -> io::Result<u32> {
        let mut previous = self.next.load(Ordering::Relaxed);
        loop {
            let next = previous
                .checked_add(1)
                .ok_or_else(|| io::Error::other("handle limit reached"))?;
            match self.next.compare_exchange_weak(
                previous,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(next),
                Err(current) => previous = current,
            }
        }
    }

    async fn insert(&self, handle: Handle) -> io::Result<u32> {
        let id = self.next_id()?;
        let mut handles = self.handles.lock().await;
        if self.closing.load(Ordering::Acquire) {
            return Err(crate::invalid("native context is closing"));
        }
        handles.insert(id, handle);
        Ok(id)
    }
    async fn get(&self, id: u32) -> io::Result<Handle> {
        self.handles
            .lock()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| crate::invalid("unknown native handle"))
    }
    pub async fn request(&self, request: &[u8], data: &[u8]) -> io::Result<Vec<u8>> {
        if request.len() > 128 * 1024 || data.len() > CHUNK {
            return Err(crate::invalid("native request exceeds limit"));
        }
        let request: Request = serde_json::from_slice(request)?;
        if self.closing.load(Ordering::Acquire) && request.op != "close" {
            return Err(crate::invalid("native context is closing"));
        }
        match request.op.as_str() {
            "publish" => {
                let id = request
                    .service_id
                    .ok_or_else(|| crate::invalid("serviceId required"))?;
                crate::validate_service_id(&id)?;
                let runtime = LocalRuntime::prepare(request.runtime_dir)?;
                let publication = Arc::new(
                    runtime
                        .publish(&id, request.name.as_deref().unwrap_or(&id), request.icon)
                        .await?,
                );
                let endpoint = publication.endpoint().clone();
                let directory = publication.directory().to_owned();
                let id = self.insert(Handle::Publication(publication)).await?;
                Ok(serde_json::to_vec(
                    &serde_json::json!({"id":id,"endpoint":endpoint,"directory":directory}),
                )?)
            }
            "connect" => {
                let endpoint = request
                    .endpoint
                    .ok_or_else(|| crate::invalid("endpoint required"))?;
                let (read, write) = endpoint.connect().await?.into_split();
                let id = self
                    .insert(Handle::Peer(Arc::new(Peer {
                        parent: 0,
                        read: Mutex::new(Some(read)),
                        write: Mutex::new(Some(write)),
                        stop: CancellationToken::new(),
                    })))
                    .await?;
                Ok(serde_json::to_vec(&id)?)
            }
            "accept" => {
                let Handle::Publication(publication) = self.get(request.id).await? else {
                    return Err(crate::invalid("expected publication"));
                };
                let (read, write) = publication.accept().await?.into_split();
                let peer = Arc::new(Peer {
                    parent: request.id,
                    read: Mutex::new(Some(read)),
                    write: Mutex::new(Some(write)),
                    stop: CancellationToken::new(),
                });
                // A close racing accept must not leave an untracked successor stream.
                let mut handles = self.handles.lock().await;
                if !handles.contains_key(&request.id) {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "publication closed",
                    ));
                }
                let id = self.next_id()?;
                handles.insert(id, Handle::Peer(peer));
                Ok(serde_json::to_vec(&id)?)
            }
            "read" | "write" => {
                let Handle::Peer(peer) = self.get(request.id).await? else {
                    return Err(crate::invalid("expected stream"));
                };
                tokio::select! {
                    biased;
                    _ = peer.stop.cancelled() => Err(io::Error::new(io::ErrorKind::Interrupted, "stream closed")),
                    result = async {
                        if request.op == "read" {
                            let mut guard = peer.read.try_lock().map_err(|_| crate::invalid("read already pending"))?;
                            let stream = guard.as_mut().ok_or_else(|| crate::invalid("stream closed"))?;
                            let mut bytes = vec![0; CHUNK]; let length = stream.read(&mut bytes).await?; bytes.truncate(length); Ok(bytes)
                        } else {
                            let mut guard = peer.write.try_lock().map_err(|_| crate::invalid("write already pending"))?;
                            guard.as_mut().ok_or_else(|| crate::invalid("stream closed"))?.write_all(data).await?; Ok(Vec::new())
                        }
                    } => result,
                }
            }
            "close" => {
                self.close(request.id).await?;
                Ok(Vec::new())
            }
            _ => Err(crate::invalid("unknown native operation")),
        }
    }
    pub async fn close(&self, id: u32) -> io::Result<()> {
        let Some(handle) = self.handles.lock().await.remove(&id) else {
            return Ok(());
        };
        match handle {
            Handle::Peer(peer) => Self::close_peer(peer).await,
            Handle::Publication(publication) => {
                publication.stop().await;
                let peers = {
                    let mut handles = self.handles.lock().await;
                    let ids: Vec<_> = handles
                        .iter()
                        .filter_map(|(peer_id, h)| match h {
                            Handle::Peer(p) if p.parent == id => Some(*peer_id),
                            _ => None,
                        })
                        .collect();
                    ids.into_iter()
                        .filter_map(|id| handles.remove(&id))
                        .collect::<Vec<_>>()
                };
                for peer in peers {
                    if let Handle::Peer(peer) = peer {
                        Self::close_peer(peer).await;
                    }
                }
                let mut finished = publication.finished();
                drop(publication);
                finished
                    .wait_for(|done| *done)
                    .await
                    .map_err(io::Error::other)?;
            }
        }
        Ok(())
    }
    async fn close_peer(peer: Arc<Peer>) {
        peer.stop.cancel();
        peer.read.lock().await.take();
        peer.write.lock().await.take();
    }
    pub async fn shutdown(&self) {
        self.closing.store(true, Ordering::Release);
        self.close_all().await;
    }
    pub async fn close_all(&self) {
        loop {
            let id = self.handles.lock().await.keys().next().copied();
            let Some(id) = id else {
                break;
            };
            let _ = self.close(id).await;
        }
    }
}
