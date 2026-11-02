//! Host-specific authorization observation. Connection policy stays in the Hub.
use anyhow::Result;
use omochi_transport::{EndpointAddr, trust::Peer};
use std::{collections::BTreeMap, future::Future, pin::Pin};

pub type HostFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, PartialEq, Eq)]
pub struct RegisteredPeer {
    pub peer: Peer,
    /// Previously saved address hints never grant authority on their own.
    pub address: Option<EndpointAddr>,
}

#[derive(Clone, Default)]
pub struct AuthorizationSnapshot {
    pub generation: u64,
    pub revocation: u64,
    pub peers: BTreeMap<String, RegisteredPeer>,
}

pub trait AuthorizationSource: Send + Sync + 'static {
    fn load(&self) -> HostFuture<'_, Result<AuthorizationSnapshot>>;
    /// Called only after the supervisor has joined all obsolete peer tasks and
    /// closed their authenticated connections, including resource streams.
    fn acknowledge(&self, generation: u64) -> HostFuture<'_, Result<()>>;
}

#[cfg(feature = "desktop")]
pub struct DesktopAuthorization {
    store: omochi_transport::store::Store,
    lease: std::sync::Arc<omochi_transport::live::Lease>,
}

#[cfg(feature = "desktop")]
impl DesktopAuthorization {
    pub async fn open(store: omochi_transport::store::Store) -> Result<Self> {
        let copy = store.clone();
        let lease =
            tokio::task::spawn_blocking(move || omochi_transport::live::Lease::register(&copy))
                .await??;
        Ok(Self {
            store,
            lease: std::sync::Arc::new(lease),
        })
    }
}

#[cfg(feature = "desktop")]
impl AuthorizationSource for DesktopAuthorization {
    fn load(&self) -> HostFuture<'_, Result<AuthorizationSnapshot>> {
        let store = self.store.clone();
        Box::pin(async move {
            let (generation, revocation, peers) =
                tokio::task::spawn_blocking(move || store.authorization()).await??;
            Ok(AuthorizationSnapshot {
                generation,
                revocation,
                peers: peers
                    .into_iter()
                    .map(|(id, peer)| {
                        (
                            id,
                            RegisteredPeer {
                                peer,
                                address: None,
                            },
                        )
                    })
                    .collect(),
            })
        })
    }

    fn acknowledge(&self, generation: u64) -> HostFuture<'_, Result<()>> {
        let lease = self.lease.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || lease.acknowledge(generation)).await?
        })
    }
}
