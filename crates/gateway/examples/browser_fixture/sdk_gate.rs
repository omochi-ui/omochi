//! A bounded transparent Unix proxy; the real native gateway retains admission,
//! remote command provenance, dispatch barriers and live revocation acknowledgements.
use anyhow::Result;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::watch,
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct Gate {
    armed: Arc<AtomicBool>,
    release: watch::Sender<u64>,
    stop: CancellationToken,
}
impl Gate {
    pub fn arm(&self) {
        self.armed.store(true, Ordering::SeqCst);
    }
    pub fn release(&self) {
        self.release.send_modify(|value| *value += 1);
    }
    pub fn stop(&self) {
        self.stop.cancel();
    }
}
pub fn install(socket: &Path) -> Result<(Gate, JoinHandle<()>)> {
    let backend = socket.with_file_name("b.sock");
    std::fs::rename(socket, &backend)?;
    let listener = UnixListener::bind(socket)?;
    let gate = Gate {
        armed: Arc::default(),
        release: watch::channel(0).0,
        stop: CancellationToken::new(),
    };
    let shared = gate.clone();
    let job = tokio::spawn(async move {
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
                _ = shared.stop.cancelled() => break,
                Some(_) = clients.join_next(), if !clients.is_empty() => {},
                connection = listener.accept() => {
                    let Ok((connection, _)) = connection else { break };
                    if clients.len() >= 8 { continue }
                    let backend = backend.clone(); let gate = shared.clone();
                    clients.spawn(async move {
                        let backend = UnixStream::connect(backend).await?;
                        let (mut native_read, mut native_write) = connection.into_split();
                        let (mut sdk_read, mut sdk_write) = backend.into_split();
                        let forward = async {
                            let mut buffer = [0u8; 8192];
                            loop {
                                let count = native_read.read(&mut buffer).await?;
                                if count == 0 { break }
                                if gate.armed.swap(false, Ordering::SeqCst) {
                                    let mut release = gate.release.subscribe();
                                    sdk_write.write_all(&buffer[..1]).await?;
                                    super::output(serde_json::json!({"kind":"sdk-write-held"})).map_err(std::io::Error::other)?;
                                    tokio::select! { _ = release.changed() => {}, _ = gate.stop.cancelled() => return Ok::<_, std::io::Error>(()) }
                                    sdk_write.write_all(&buffer[1..count]).await?;
                                } else { sdk_write.write_all(&buffer[..count]).await?; }
                            }
                            sdk_write.shutdown().await
                        };
                        let reverse = async { tokio::io::copy(&mut sdk_read, &mut native_write).await?; native_write.shutdown().await };
                        tokio::try_join!(forward, reverse)?;
                        Ok::<_, anyhow::Error>(())
                    });
                }
            }
        }
        clients.abort_all();
        while clients.join_next().await.is_some() {}
    });
    Ok((gate, job))
}
