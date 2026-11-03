//! Domain adapter for cross-process revocation acknowledgements.
use crate::{dashboards::ApiError, sharing::ShareStore};
use omochi_transport::persistence;
use std::time::Duration;
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
    pub async fn revoke_live(&self, id: &str) -> Result<(), ApiError> {
        self.revoke_with_timeout(id, Duration::from_secs(10)).await
    }
    async fn revoke_with_timeout(&self, id: &str, timeout: Duration) -> Result<(), ApiError> {
        let store = self.clone();
        let id = id.to_owned();
        let (generation, paths) = omochi_transport::blocking::run(move || {
            store.revoke(&id)?;
            store.with_generation(|generation| {
                Ok((
                    generation,
                    persistence::leases(store.directory(), "share-live-")?,
                ))
            })
        })
        .await
        .map_err(|_| ApiError::new(503, "Revocation unavailable"))??;
        persistence::wait_for_ack(generation, paths, timeout)
            .await
            .map_err(|_| {
                ApiError::new(
                    503,
                    "Share removed, but live connection teardown was not confirmed",
                )
            })
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
