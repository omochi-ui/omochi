//! Domain adapter for cross-process revocation acknowledgements.
use crate::{dashboards::ApiError, sharing::ShareStore};
use omochi_transport::persistence;
use std::time::Duration;
pub(crate) struct Revocation {
    generation: u64,
    paths: Vec<std::path::PathBuf>,
}
impl Revocation {
    pub(crate) async fn wait(self, timeout: Duration) -> Result<(), ApiError> {
        persistence::wait_for_ack(self.generation, self.paths, timeout)
            .await
            .map_err(|_| {
                ApiError::new(
                    503,
                    "Share removed, but live connection teardown was not confirmed",
                )
            })
    }
}
pub(crate) struct Lease(persistence::Lease);
impl Lease {
    pub fn register(store: &ShareStore) -> Result<Self, ApiError> {
        store.with_generation(|generation| {
            Ok(Self(persistence::Lease::register(
                store.directory(),
                "share-live-",
                generation,
            )?))
        })
    }
    pub fn acknowledge(&self, generation: u64) -> Result<(), ApiError> {
        self.0.acknowledge(generation).map_err(Into::into)
    }
}
impl ShareStore {
    /// Commit while the caller still holds any owner authorization lock. Live
    /// teardown must be awaited only after releasing that lock.
    pub(crate) fn begin_revocation(&self, id: &str) -> Result<Revocation, ApiError> {
        #[cfg(test)]
        test_hooks::before_revocation(self);
        self.revoke(id)?;
        self.with_generation(|generation| {
            Ok(Revocation {
                generation,
                paths: persistence::leases(self.directory(), "share-live-")?,
            })
        })
    }
    pub async fn revoke_live(&self, id: &str) -> Result<(), ApiError> {
        self.revoke_with_timeout(id, Duration::from_secs(10)).await
    }
    async fn revoke_with_timeout(&self, id: &str, timeout: Duration) -> Result<(), ApiError> {
        let store = self.clone();
        let id = id.to_owned();
        let revocation = omochi_transport::blocking::run(move || store.begin_revocation(&id))
            .await
            .map_err(|_| ApiError::new(503, "Revocation unavailable"))??;
        revocation.wait(timeout).await
    }
}

#[cfg(test)]
pub(crate) mod test_hooks {
    use super::*;
    use std::sync::{LazyLock, Mutex, mpsc};
    use tokio::sync::oneshot;

    type Pause = (std::path::PathBuf, oneshot::Sender<()>, mpsc::Receiver<()>);
    static PAUSE: LazyLock<Mutex<Option<Pause>>> = LazyLock::new(|| Mutex::new(None));

    pub fn pause_revocation(store: &ShareStore) -> (oneshot::Receiver<()>, mpsc::Sender<()>) {
        let (started, ready) = oneshot::channel();
        let (release, wait) = mpsc::channel();
        let mut pause = PAUSE.lock().unwrap();
        assert!(pause.is_none());
        *pause = Some((store.directory().to_owned(), started, wait));
        (ready, release)
    }

    pub(super) fn before_revocation(store: &ShareStore) {
        let pause = {
            let mut pause = PAUSE.lock().unwrap();
            if pause
                .as_ref()
                .is_some_and(|(root, _, _)| root == store.directory())
            {
                pause.take()
            } else {
                None
            }
        };
        if let Some((_, started, wait)) = pause {
            let _ = started.send(());
            let _ = wait.recv();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dashboards::DashboardStore, sharing::Permissions};
    #[tokio::test]
    async fn unresponsive_live_owner_makes_revocation_an_error_without_restoring_the_grant() {
        let root = tempfile::tempdir().unwrap();
        let boards = DashboardStore::open(root.path().join("state")).unwrap();
        let board = boards
            .mutate(
                None,
                serde_json::json!({"name":"Shared","panels":[]}),
                false,
            )
            .unwrap()
            .unwrap();
        let shares = boards.shares();
        let (grant, secret) = shares
            .create(&boards, &board.id, true, Permissions::default())
            .unwrap();
        let lease = Lease::register(&shares).unwrap();
        assert!(
            shares
                .revoke_with_timeout(&grant.id, Duration::from_millis(50))
                .await
                .is_err()
        );
        assert!(shares.list().unwrap().is_empty());
        assert!(
            shares
                .authorize(&boards, &grant.id, "peer", Some(&secret))
                .is_err()
        );
        drop(lease);
    }
}
