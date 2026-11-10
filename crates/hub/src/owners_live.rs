//! Domain adapter for cross-process revocation acknowledgements.
use crate::{dashboards::ApiError, owners::OwnerStore};
use omochi_transport::persistence;
use std::time::Duration;
pub(crate) struct Lease(persistence::Lease);
impl Lease {
    pub fn register(store: &OwnerStore) -> Result<Self, ApiError> {
        store.with_generation(|generation| {
            Ok(Self(persistence::Lease::register(
                store.directory(),
                "owner-live-",
                generation,
            )?))
        })
    }
    pub fn acknowledge(&self, generation: u64) -> Result<(), ApiError> {
        self.0.acknowledge(generation).map_err(Into::into)
    }
}
impl OwnerStore {
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
                    persistence::leases(store.directory(), "owner-live-")?,
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
                    "Owner access removed, but live connection teardown was not confirmed",
                )
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unresponsive_owner_listener_is_an_error_without_restoring_revoked_records() {
        let root = tempfile::tempdir().unwrap();
        omochi_local::files::private_directory(root.path()).unwrap();
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
