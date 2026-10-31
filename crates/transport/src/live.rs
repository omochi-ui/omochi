//! Cross-process authorization acknowledgements. A held flock proves process liveness.
use crate::{EndpointId, store::Store};
use anyhow::{Context, Result, bail};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct Lease {
    file: File,
    path: PathBuf,
    ack: PathBuf,
}
impl Lease {
    /// Must happen before accepting/dialing remote connections. Registration is serialized
    /// with trust mutations, so forget cannot miss an older authorization snapshot.
    pub fn register(store: &Store) -> Result<Self> {
        store.with_generation(|generation| {
            let name = format!("live-{}", hex::encode(rand::random::<[u8; 16]>()));
            let path = store.directory().join(format!("{name}.lease"));
            let ack = store.directory().join(format!("{name}.ack"));
            let file = OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .mode(0o600)
                .open(&path)?;
            file.lock_exclusive()?;
            let lease = Self { file, path, ack };
            lease.acknowledge(generation)?;
            Ok(lease)
        })
    }
    /// Call only after all work authorized by earlier generations has been stopped/joined.
    pub fn acknowledge(&self, generation: u64) -> Result<()> {
        let tmp = self.ack.with_extension("tmp");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(generation.to_string().as_bytes())?;
        file.sync_all()?;
        fs::rename(tmp, &self.ack)?;
        Ok(())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.ack);
        let _ = fs::remove_file(&self.path);
        let _ = FileExt::unlock(&self.file);
    }
}
/// Record removal is durable even when an unresponsive process makes this return an error.
pub async fn forget(store: Store, peer: EndpointId, timeout: Duration) -> Result<()> {
    let copy = store.clone();
    let (generation, leases) = tokio::task::spawn_blocking(move || {
        let generation = copy.revoke_record(peer)?;
        let mut leases = Vec::new();
        for entry in fs::read_dir(copy.directory())? {
            let entry = entry?;
            if entry
                .file_name()
                .to_str()
                .is_some_and(|s| s.starts_with("live-") && s.ends_with(".lease"))
            {
                leases.push(entry.path());
            }
        }
        Ok::<_, anyhow::Error>((generation, leases))
    })
    .await??;
    let deadline = Instant::now() + timeout;
    loop {
        let paths = leases.clone();
        let pending = tokio::task::spawn_blocking(move || {
            let mut pending = 0;
            for path in paths {
                let file = match OpenOptions::new().read(true).write(true).open(&path) {
                    Ok(file) => file,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(e.into()),
                };
                match file.try_lock_exclusive() {
                    Ok(()) => {
                        FileExt::unlock(&file)?;
                        continue;
                    } // exited process
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.into()),
                }
                let acknowledged = fs::read_to_string(path.with_extension("ack"))
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok());
                if !acknowledged.is_some_and(|ack| ack >= generation) {
                    pending += 1;
                }
            }
            Ok::<_, anyhow::Error>(pending)
        })
        .await
        .context("authorization acknowledgement check failed")??;
        if pending == 0 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!(
                "peer removed, but {pending} live process(es) did not acknowledge connection teardown"
            )
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SecretKey, store::Role};
    #[tokio::test]
    async fn forget_requires_live_acknowledgement_and_keeps_revocation_on_failure() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().join("state")).unwrap();
        let peer = SecretKey::from_bytes(&rand::random()).public();
        store.register_gateway(peer, "gateway").unwrap();
        let lease = Lease::register(&store).unwrap();
        assert!(
            forget(store.clone(), peer, Duration::from_millis(50))
                .await
                .is_err()
        );
        assert!(!store.peers().unwrap().1.contains_key(&peer.to_string()));
        let before = store.peers().unwrap().0;
        let copy = store.clone();
        let task = tokio::spawn(async move { forget(copy, peer, Duration::from_secs(2)).await });
        let generation = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let generation = store.peers().unwrap().0;
                if generation > before {
                    break generation;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("revocation generation was not recorded");
        lease.acknowledge(generation).unwrap();
        task.await.unwrap().unwrap();
        drop(lease);
        forget(
            store.clone(),
            store.identity(Role::Gateway).unwrap().public(),
            Duration::from_millis(50),
        )
        .await
        .unwrap();
    }
}
