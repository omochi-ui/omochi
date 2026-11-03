//! Queue provenance survives connection task disposal and authorization changes.
use std::sync::Arc;
use tokio::sync::{OwnedRwLockReadGuard, RwLock};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub(crate) struct AuthorizationEpoch {
    stop: CancellationToken,
    barrier: Arc<RwLock<()>>,
}
impl AuthorizationEpoch {
    pub(crate) fn new() -> Self {
        Self {
            stop: CancellationToken::new(),
            barrier: Arc::new(RwLock::new(())),
        }
    }
    pub(crate) fn cancel(&self) {
        self.stop.cancel();
    }
    /// Called after cancelling and joining remote sessions, before store ACK.
    pub(crate) async fn drain(&self) {
        let _exclusive = self.barrier.write().await;
    }
}

#[derive(Clone)]
pub(crate) enum CommandOrigin {
    Local,
    Remote {
        epoch: AuthorizationEpoch,
        session: CancellationToken,
    },
}

pub(crate) enum DispatchGuard {
    Local,
    Remote { _guard: OwnedRwLockReadGuard<()> },
}
impl CommandOrigin {
    /// No cancellation may interrupt a shared SDK write after this guard is acquired.
    pub(crate) async fn enter(&self) -> Option<DispatchGuard> {
        let Self::Remote { epoch, session } = self else {
            return Some(DispatchGuard::Local);
        };
        let guard = tokio::select! {
            biased;
            _ = epoch.stop.cancelled() => return None,
            _ = session.cancelled() => return None,
            guard = epoch.barrier.clone().read_owned() => guard,
        };
        if epoch.stop.is_cancelled() || session.is_cancelled() {
            return None;
        }
        Some(DispatchGuard::Remote { _guard: guard })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_origins_cannot_dispatch_and_local_work_remains_admitted() {
        let epoch = AuthorizationEpoch::new();
        let session = CancellationToken::new();
        let remote = CommandOrigin::Remote {
            epoch: epoch.clone(),
            session: session.clone(),
        };
        drop(remote.enter().await.unwrap());
        session.cancel();
        assert!(remote.enter().await.is_none());
        let other = CommandOrigin::Remote {
            epoch: epoch.clone(),
            session: CancellationToken::new(),
        };
        drop(other.enter().await.unwrap());
        epoch.cancel();
        assert!(other.enter().await.is_none());
        assert!(CommandOrigin::Local.enter().await.is_some());
    }

    #[tokio::test]
    async fn barrier_waits_for_dispatched_write_then_prevents_any_old_epoch_write() {
        let epoch = AuthorizationEpoch::new();
        let remote = CommandOrigin::Remote {
            epoch: epoch.clone(),
            session: CancellationToken::new(),
        };
        let dispatched = remote.enter().await.unwrap();
        epoch.cancel();
        let draining = epoch.drain();
        tokio::pin!(draining);
        assert!(futures_util::poll!(&mut draining).is_pending());
        assert!(remote.enter().await.is_none());
        drop(dispatched);
        draining.await;
        assert!(remote.enter().await.is_none());
    }
}
