//! Acknowledgements for owner revocation, separate from sharing and gateway trust.
use crate::{dashboards::ApiError, owners::OwnerStore};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    time::{Duration, Instant},
};

pub(crate) struct Lease {
    file: File,
    path: PathBuf,
}
impl Lease {
    pub fn register(store: &OwnerStore) -> Result<Self, ApiError> {
        store.with_generation(|generation| {
            let path = store
                .directory()
                .join(format!("owner-live-{}.lease", uuid::Uuid::new_v4()));
            let file = OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .mode(0o600)
                .open(&path)?;
            file.lock_exclusive()?;
            let lease = Self { file, path };
            lease.acknowledge(generation)?;
            Ok(lease)
        })
    }
    pub fn acknowledge(&self, generation: u64) -> Result<(), ApiError> {
        let tmp = self.path.with_extension("tmp");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(generation.to_string().as_bytes())?;
        file.sync_all()?;
        fs::rename(tmp, self.path.with_extension("ack"))?;
        Ok(())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.path.with_extension("ack"));
        let _ = fs::remove_file(&self.path);
        let _ = FileExt::unlock(&self.file);
    }
}
impl OwnerStore {
    /// Removal is durable even if a live listener fails to acknowledge teardown.
    pub async fn revoke_live(&self, id: &str) -> Result<(), ApiError> {
        self.revoke_with_timeout(id, Duration::from_secs(10)).await
    }
    async fn revoke_with_timeout(&self, id: &str, timeout: Duration) -> Result<(), ApiError> {
        let store = self.clone();
        let id = id.to_owned();
        let (generation, paths) = tokio::task::spawn_blocking(move || {
            store.revoke(&id)?;
            store.with_generation(|generation| {
                let mut paths = Vec::new();
                for entry in fs::read_dir(store.directory())? {
                    let entry = entry?;
                    if entry
                        .file_name()
                        .to_str()
                        .is_some_and(|s| s.starts_with("owner-live-") && s.ends_with(".lease"))
                    {
                        paths.push(entry.path());
                    }
                }
                Ok((generation, paths))
            })
        })
        .await
        .map_err(|_| ApiError::new(503, "Sharing removal failed"))??;
        let deadline = Instant::now() + timeout;
        loop {
            let paths = paths.clone();
            let pending = tokio::task::spawn_blocking(move || -> Result<bool, ApiError> {
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
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => return Err(e.into()),
                    }
                    if !fs::read_to_string(path.with_extension("ack"))
                        .ok()
                        .and_then(|s| s.parse::<u64>().ok())
                        .is_some_and(|ack| ack >= generation)
                    {
                        return Ok(true);
                    }
                }
                Ok(false)
            })
            .await
            .map_err(|_| ApiError::new(503, "Sharing acknowledgement failed"))??;
            if !pending {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(ApiError::new(
                    503,
                    "Share removed, but a live hub did not acknowledge teardown",
                ));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unresponsive_owner_listener_is_an_error_without_restoring_revoked_records() {
        let root = tempfile::tempdir().unwrap();
        let store = OwnerStore::new(root.path().to_owned());
        let (grant, _) = store.create().unwrap();
        let lease = Lease::register(&store).unwrap();
        assert!(
            store
                .revoke_with_timeout(&grant.id, Duration::from_millis(40))
                .await
                .is_err()
        );
        assert!(store.list().unwrap().is_empty());
        drop(lease);
    }
}
