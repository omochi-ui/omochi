//! Trust adapter for cross-process authorization acknowledgements.
use crate::{EndpointId, persistence, store::Store};
use anyhow::Result;
use std::time::Duration;
pub struct Lease(persistence::Lease);
impl Lease {
    pub fn register(store: &Store) -> Result<Self> {
        store.with_generation(|generation| {
            Ok(Self(persistence::Lease::register(
                store.directory(),
                "live-",
                generation,
            )?))
        })
    }
    pub fn acknowledge(&self, generation: u64) -> Result<()> {
        Ok(self.0.acknowledge(generation)?)
    }
}
pub async fn forget(store: Store, peer: EndpointId, timeout: Duration) -> Result<()> {
    let (generation, leases) = crate::blocking::run(move || {
        let generation = store.revoke_record(peer)?;
        Ok::<_, anyhow::Error>((generation, persistence::leases(store.directory(), "live-")?))
    })
    .await??;
    persistence::wait_for_ack(generation, leases, timeout).await
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
