//! File mechanics shared by independent authorization domains. Callers own schemas and policy.
use anyhow::{Context, Result, bail};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Hold this file for the entire read/validate/mutate/commit transaction.
pub fn lock(path: &Path) -> std::io::Result<File> {
    let file = omochi_local::files::private_file(path, false)?;
    file.lock_exclusive()?;
    Ok(file)
}
fn lock_new(path: &Path) -> std::io::Result<File> {
    let file = omochi_local::files::private_file(path, true)?;
    file.lock_exclusive()?;
    Ok(file)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    omochi_local::files::atomic_write(path, bytes)
}
pub struct Lease {
    file: File,
    path: PathBuf,
    ack: PathBuf,
}
impl Lease {
    /// Must happen before accepting/dialing remote connections. Registration is serialized
    /// with trust mutations, so forget cannot miss an older authorization snapshot.
    pub fn register(
        root: &std::path::Path,
        prefix: &str,
        generation: u64,
    ) -> std::io::Result<Self> {
        let name = format!("{prefix}{}", hex::encode(rand::random::<[u8; 16]>()));
        let path = root.join(format!("{name}.lease"));
        let ack = path.with_extension("ack");
        let file = lock_new(&path)?;
        let lease = Self { file, path, ack };
        lease.acknowledge(generation)?;
        Ok(lease)
    }
    /// Call only after all work authorized by earlier generations has been stopped/joined.
    pub fn acknowledge(&self, generation: u64) -> std::io::Result<()> {
        atomic_write(&self.ack, generation.to_string().as_bytes())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.ack);
        let _ = fs::remove_file(&self.path);
        let _ = FileExt::unlock(&self.file);
    }
}

pub fn leases(root: &Path, prefix: &str) -> std::io::Result<Vec<PathBuf>> {
    fs::read_dir(root)?
        .filter_map(|entry| match entry {
            Ok(entry)
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(prefix) && name.ends_with(".lease")) =>
            {
                Some(Ok(entry.path()))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}
pub async fn wait_for_ack(generation: u64, leases: Vec<PathBuf>, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let paths = leases.clone();
        let pending = crate::blocking::run(move || {
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
                    Err(e)
                        if e.raw_os_error() == fs2::lock_contended_error().raw_os_error()
                            || e.kind() == std::io::ErrorKind::WouldBlock => {}
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
    #[test]
    fn hold_transaction_in_child() {
        let Some(root) = std::env::var_os("OMOCHI_LOCK_TEST_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let _lock = lock(&root.join("state.lock")).unwrap();
        fs::write(root.join("ready"), b"ready").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join("release").exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[tokio::test]
    async fn another_process_holding_the_transaction_lock_does_not_block_async_timers() {
        let root = tempfile::tempdir().unwrap();
        omochi_local::files::private_directory(root.path()).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "persistence::tests::hold_transaction_in_child"])
            .env("OMOCHI_LOCK_TEST_ROOT", root.path())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !root.path().join("ready").exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let path = root.path().join("state.lock");
        let mut transaction = crate::blocking::run(move || lock(&path).unwrap());
        assert!(
            tokio::time::timeout(Duration::from_millis(30), &mut transaction)
                .await
                .is_err()
        );
        fs::write(root.path().join("release"), b"release").unwrap();
        drop(transaction.await.unwrap());
        assert!(child.wait().unwrap().success());
    }
    #[tokio::test]
    async fn namespaces_are_independent_and_invalid_ack_never_confirms_revocation() {
        let root = tempfile::tempdir().unwrap();
        omochi_local::files::private_directory(root.path()).unwrap();
        let owner = Lease::register(root.path(), "owner-live-", 3).unwrap();
        let share = Lease::register(root.path(), "share-live-", 3).unwrap();
        assert_eq!(leases(root.path(), "owner-live-").unwrap().len(), 1);
        fs::write(owner.path.with_extension("ack"), "invalid").unwrap();
        assert!(
            wait_for_ack(
                4,
                leases(root.path(), "owner-live-").unwrap(),
                Duration::from_millis(30)
            )
            .await
            .is_err()
        );
        owner.acknowledge(4).unwrap();
        wait_for_ack(
            4,
            leases(root.path(), "owner-live-").unwrap(),
            Duration::from_millis(30),
        )
        .await
        .unwrap();
        assert!(
            wait_for_ack(
                4,
                leases(root.path(), "share-live-").unwrap(),
                Duration::from_millis(30)
            )
            .await
            .is_err()
        );
        drop(share);
        drop(owner);
        assert!(leases(root.path(), "owner-live-").unwrap().is_empty());
    }
    #[test]
    fn commits_are_private_and_failed_replacements_clean_temporary_files() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        omochi_local::files::private_directory(root.path()).unwrap();
        let path = root.path().join("state.json");
        atomic_write(&path, b"first").unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        atomic_write(&path, b"second").unwrap();
        assert_eq!(fs::read(path).unwrap(), b"second");
        let directory = root.path().join("directory");
        fs::create_dir(&directory).unwrap();
        assert!(atomic_write(&directory, b"cannot replace directory").is_err());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }
}
