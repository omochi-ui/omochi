//! Embedded Hub lifecycle and trusted administration. No HTTP server or filesystem.
use crate::{
    Hub,
    host::{AuthorizationSnapshot, AuthorizationSource, HostFuture, RegisteredPeer},
    identity::{IdentityError, IdentityStore, NativeIdentity, SavedGateway},
};
use anyhow::{Context, Result, bail};
use omochi_transport::{
    Endpoint, EndpointId, pairing,
    trust::{Peer, Role},
};
use std::{
    collections::BTreeSet,
    future::Future,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Mutex, Semaphore, watch},
    task::JoinHandle,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

const STOP_TIMEOUT: Duration = Duration::from_secs(10);

struct NativeAuthorization {
    snapshot: RwLock<AuthorizationSnapshot>,
    acknowledged: watch::Sender<u64>,
}
impl NativeAuthorization {
    fn new() -> Self {
        let (acknowledged, _) = watch::channel(0);
        Self {
            snapshot: RwLock::new(AuthorizationSnapshot::default()),
            acknowledged,
        }
    }

    fn publish(&self, peers: &[SavedGateway], blocked: &BTreeSet<EndpointId>, online: bool) -> u64 {
        let mut snapshot = self.snapshot.write().unwrap();
        snapshot.generation += 1;
        snapshot.peers = peers
            .iter()
            .filter(|p| !blocked.contains(&p.address.id))
            .map(|p| {
                let mut address = p.address.clone();
                if !online {
                    address.addrs.retain(|a| a.is_ip());
                }
                (
                    p.address.id.to_string(),
                    RegisteredPeer {
                        peer: Peer {
                            name: p.name.clone(),
                            role: Role::Gateway,
                        },
                        address: Some(address),
                    },
                )
            })
            .collect();
        snapshot.generation
    }

    async fn wait(&self, generation: u64) -> Result<()> {
        let mut ack = self.acknowledged.subscribe();
        tokio::time::timeout(STOP_TIMEOUT, async {
            loop {
                if *ack.borrow_and_update() >= generation {
                    return Ok::<_, anyhow::Error>(());
                }
                ack.changed()
                    .await
                    .context("authorization supervisor stopped")?;
            }
        })
        .await
        .context("authorization teardown was not acknowledged")?
    }
}
impl AuthorizationSource for NativeAuthorization {
    fn load(&self) -> HostFuture<'_, Result<AuthorizationSnapshot>> {
        Box::pin(async { Ok(self.snapshot.read().unwrap().clone()) })
    }
    fn acknowledge(&self, generation: u64) -> HostFuture<'_, Result<()>> {
        Box::pin(async move {
            self.acknowledged.send_replace(generation);
            Ok(())
        })
    }
}

struct Running {
    hub: Hub,
    endpoint: Endpoint,
    source: Arc<NativeAuthorization>,
    task: JoinHandle<Result<()>>,
    termination: Option<std::result::Result<(), String>>,
}
impl Running {
    async fn start(
        identity: &NativeIdentity,
        blocked: &BTreeSet<EndpointId>,
        online: bool,
    ) -> Result<Self> {
        let endpoint = omochi_transport::bind(identity.key()?, !online, None).await?;
        let hub = Hub::embedded();
        for id in blocked {
            hub.revoke_gateway(&id.to_string()).await;
        }
        let source = Arc::new(NativeAuthorization::new());
        source.publish(identity.peers()?, blocked, online);
        let (task, _) = hub.attach_authorization(endpoint.clone(), source.clone());
        Ok(Self {
            hub,
            endpoint,
            source,
            task,
            termination: None,
        })
    }

    async fn stop(&mut self) -> Result<()> {
        self.hub.shutdown();
        if let Some(result) = &self.termination {
            return result.clone().map_err(anyhow::Error::msg);
        }
        // Keep the handle on timeout so a later stop can still verify termination.
        let result = tokio::time::timeout(STOP_TIMEOUT, &mut self.task)
            .await
            .context("Hub shutdown is not confirmed")?
            .context("Hub supervisor failed during shutdown")
            .and_then(|result| result);
        self.termination = Some(result.as_ref().map(|_| ()).map_err(ToString::to_string));
        result
    }
}

struct State {
    identity: NativeIdentity,
    running: Option<Running>,
    blocked: BTreeSet<EndpointId>,
    online: bool,
    pending_pair: Option<SavedGateway>,
}
impl State {
    async fn publish(&self) {
        if let Some(running) = &self.running {
            if self.identity.peers().is_err() {
                let ids: Vec<_> = running
                    .source
                    .snapshot
                    .read()
                    .unwrap()
                    .peers
                    .keys()
                    .cloned()
                    .collect();
                for id in ids {
                    running.hub.revoke_gateway(&id).await;
                }
            }
            running.source.publish(
                self.identity.peers().unwrap_or(&[]),
                &self.blocked,
                self.online,
            );
        }
    }

    async fn deny(&mut self, id: EndpointId) -> u64 {
        self.blocked.insert(id);
        if let Some(running) = &self.running {
            running.hub.revoke_gateway(&id.to_string()).await;
            running.source.publish(
                self.identity.peers().unwrap_or(&[]),
                &self.blocked,
                self.online,
            )
        } else {
            0
        }
    }

    async fn wait(&self, generation: u64) -> Result<()> {
        if let Some(running) = &self.running {
            running.source.wait(generation).await?;
        }
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(running) = &mut self.running {
            running.stop().await?;
        }
        self.running = None;
        Ok(())
    }
}

#[derive(Default)]
struct Admission {
    pairing: CancellationToken,
    suspension: u64,
}

struct Inner {
    state: Mutex<State>,
    admission: std::sync::Mutex<Admission>,
    closing: AtomicBool,
    cancel: CancellationToken,
    commands: Arc<Semaphore>,
    tasks: TaskTracker,
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(running) = &self.state.get_mut().running {
            running.hub.shutdown();
        }
    }
}

/// Trusted host API. Service-supplied A2UI must never receive this administrative
/// object. Once a command starts, dropping its caller does not abandon a save.
#[derive(Clone)]
pub struct NativeHub {
    inner: Arc<Inner>,
}

impl NativeHub {
    pub async fn open(store: Arc<dyn IdentityStore>, online: bool) -> Result<Self> {
        let identity = NativeIdentity::open(store).await?;
        let blocked = BTreeSet::new();
        let running = Some(Running::start(&identity, &blocked, online).await?);
        Ok(Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State {
                    identity,
                    running,
                    blocked,
                    online,
                    pending_pair: None,
                }),
                admission: std::sync::Mutex::new(Admission::default()),
                closing: AtomicBool::new(false),
                cancel: CancellationToken::new(),
                commands: Arc::new(Semaphore::new(8)),
                tasks: TaskTracker::new(),
            }),
        })
    }

    async fn command<T: Send + 'static>(
        &self,
        task: impl Future<Output = Result<T>> + Send + 'static,
    ) -> Result<T> {
        self.command_admitted(|_| task).await
    }

    async fn command_admitted<T: Send + 'static, F: Future<Output = Result<T>> + Send + 'static>(
        &self,
        task: impl FnOnce(&mut Admission) -> F,
    ) -> Result<T> {
        let task = {
            let mut admission = self.inner.admission.lock().unwrap();
            if self.inner.closing.load(Ordering::Acquire) {
                bail!("Hub is stopping")
            }
            let permit = self
                .inner
                .commands
                .clone()
                .try_acquire_owned()
                .context("Hub administration is busy")?;
            let task = task(&mut admission);
            self.inner.tasks.spawn(async move {
                let _permit = permit;
                task.await
            })
        };
        task.await?
    }

    fn resume_pairing(&self, suspension: u64) {
        let mut admission = self.inner.admission.lock().unwrap();
        // A suspension admitted during restart must still cancel new enrollment.
        if admission.suspension == suspension {
            admission.pairing.cancel();
            admission.pairing = CancellationToken::new();
        }
    }

    pub async fn identity(&self) -> Result<String> {
        Ok(self
            .inner
            .state
            .lock()
            .await
            .identity
            .key()?
            .public()
            .to_string())
    }

    pub async fn peers(&self) -> Result<Vec<SavedGateway>> {
        Ok(self.inner.state.lock().await.identity.peers()?.to_vec())
    }

    pub async fn hub(&self) -> Result<Hub> {
        self.inner
            .state
            .lock()
            .await
            .running
            .as_ref()
            .map(|r| r.hub.clone())
            .context("Hub is suspended")
    }

    pub async fn pair(&self, invitation: String, cancel: CancellationToken) -> Result<()> {
        // Validate/bound input before admitting an owned background operation.
        let invite = pairing::Invite::decode(&invitation)?;
        let this = self.clone();
        self.command_admitted(|admission| {
            let pairing = admission.pairing.clone();
            async move {
                let mut state = this.inner.state.lock().await;
                if this.inner.cancel.is_cancelled() || cancel.is_cancelled() || pairing.is_cancelled() {
                    bail!("pairing cancelled")
                }
                state.identity.key()?;
                let running = state.running.as_ref().context("Hub is suspended")?;
                let address = tokio::select! {
                    _ = this.inner.cancel.cancelled() => bail!("pairing cancelled; remote enrollment may have completed"),
                    _ = cancel.cancelled() => bail!("pairing cancelled; remote enrollment may have completed"),
                    _ = pairing.cancelled() => bail!("pairing cancelled; remote enrollment may have completed"),
                    result = pairing::enroll(&running.endpoint, invite, "Omochi Flutter Hub") => result?,
                };
                let id = address.id;
                // Re-enrollment cannot revive callbacks from an older registration.
                let generation = state.deny(id).await;
                state.wait(generation).await?;
                if this.inner.cancel.is_cancelled() || cancel.is_cancelled() || pairing.is_cancelled() {
                    bail!("pairing cancelled before local save; gateway accepted enrollment")
                }
                let peer = SavedGateway {
                    name: format!("Gateway {}", &id.to_string()[..8]),
                    address,
                };
                // No cancellation/timeout around storage: always settle the write.
                state.pending_pair = Some(peer.clone());
                let result = state.identity.remember(peer).await;
                if let Err(error) = result {
                    if !matches!(error, IdentityError::OutcomeUnknown | IdentityError::Conflict) {
                        state.pending_pair = None;
                    }
                    state.publish().await;
                    return Err(error).context("gateway accepted enrollment, but local registration was not confirmed");
                }
                state.pending_pair = None;
                state.blocked.remove(&id);
                if !this.inner.closing.load(Ordering::Acquire) {
                    if let Some(running) = &state.running {
                        running.hub.allow_gateway(&id.to_string()).await;
                    }
                    state.publish().await;
                }
                Ok(())
            }
        }).await
    }

    pub async fn forget(&self, id: EndpointId) -> Result<()> {
        let this = self.clone();
        self.command(async move {
            let mut state = this.inner.state.lock().await;
            if state
                .pending_pair
                .as_ref()
                .is_some_and(|peer| peer.address.id == id)
            {
                state.pending_pair = None;
            }
            let generation = state.deny(id).await;
            let saved = state.identity.forget(id).await;
            state.publish().await;
            // Even a failed deletion must stop current work; the blocked set is
            // retained in memory and is not claimed to survive process restart.
            let stopped = state.wait(generation).await;
            saved.context("gateway remains disabled, but persistent deletion was not confirmed")?;
            stopped.context("gateway deleted, but connection teardown was not confirmed")?;
            Ok(())
        })
        .await
    }

    pub async fn reload_authorization(&self) -> Result<()> {
        let this = self.clone();
        self.command_admitted(|admission| {
            let suspension = admission.suspension;
            async move {
                let mut state = this.inner.state.lock().await;
                // End old lifetimes before resolving an uncertain persistent result.
                state.stop().await?;
                state.identity.reload().await?;
                if let Some(peer) = state.pending_pair.take()
                    && state.identity.peers()?.contains(&peer)
                {
                    state.blocked.remove(&peer.address.id);
                }
                if !this.inner.closing.load(Ordering::Acquire) {
                    state.running =
                        Some(Running::start(&state.identity, &state.blocked, state.online).await?);
                    this.resume_pairing(suspension);
                }
                Ok(())
            }
        })
        .await
    }

    pub async fn suspend(&self) -> Result<()> {
        let this = self.clone();
        self.command_admitted(|admission| {
            // Cancel network waits before waiting for the state lock. Keep this
            // generation cancelled until restart; accepted storage writes settle.
            admission.pairing.cancel();
            admission.suspension += 1;
            async move { this.inner.state.lock().await.stop().await }
        })
        .await
    }

    pub async fn retry(&self, id: EndpointId) -> Result<()> {
        let this = self.clone();
        self.command(async move {
            let mut state = this.inner.state.lock().await;
            if state.blocked.contains(&id)
                || !state
                    .identity
                    .peers()?
                    .iter()
                    .any(|peer| peer.address.id == id)
            {
                bail!("gateway is not authorized for retry")
            }
            let generation = state.deny(id).await;
            state.wait(generation).await?;
            state.blocked.remove(&id);
            if let Some(running) = &state.running {
                running.hub.allow_gateway(&id.to_string()).await;
            }
            state.publish().await;
            Ok(())
        })
        .await
    }

    pub async fn resume(&self, online: bool) -> Result<()> {
        let this = self.clone();
        self.command_admitted(|admission| {
            let suspension = admission.suspension;
            async move {
                let mut state = this.inner.state.lock().await;
                if this.inner.closing.load(Ordering::Acquire) {
                    bail!("Hub is stopping")
                }
                state.stop().await?;
                state.identity.reload().await?;
                if let Some(peer) = state.pending_pair.take()
                    && state.identity.peers()?.contains(&peer)
                {
                    state.blocked.remove(&peer.address.id);
                }
                state.online = online;
                state.running =
                    Some(Running::start(&state.identity, &state.blocked, online).await?);
                this.resume_pairing(suspension);
                Ok(())
            }
        })
        .await
    }

    pub async fn close(&self) -> Result<()> {
        {
            let _admission = self.inner.admission.lock().unwrap();
            self.inner.closing.store(true, Ordering::Release);
            self.inner.cancel.cancel();
            self.inner.tasks.close();
        }
        // Accepted writes settle even if the requesting UI has disappeared.
        tokio::time::timeout(STOP_TIMEOUT, self.inner.tasks.wait())
            .await
            .context("Hub shutdown is waiting for a storage operation")?;
        self.inner.state.lock().await.stop().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{CommitOutcome, IdentityError};
    use omochi_transport::SecretKey;
    use std::sync::Mutex as SyncMutex;

    #[derive(Default)]
    struct Storage {
        value: SyncMutex<Option<String>>,
        fail: AtomicBool,
        unknown: AtomicBool,
        gate: SyncMutex<Option<Arc<Semaphore>>>,
        save_started: tokio::sync::Notify,
        read_gate: SyncMutex<Option<Arc<Semaphore>>>,
        read_started: tokio::sync::Notify,
    }
    impl IdentityStore for Storage {
        fn read(&self) -> HostFuture<'_, std::result::Result<Option<String>, IdentityError>> {
            Box::pin(async {
                let gate = self.read_gate.lock().unwrap().clone();
                if let Some(gate) = gate {
                    self.read_started.notify_one();
                    let _ = gate.acquire().await;
                }
                Ok(self.value.lock().unwrap().clone())
            })
        }
        fn compare_exchange(
            &self,
            expected: Option<String>,
            replacement: String,
        ) -> HostFuture<'_, CommitOutcome> {
            Box::pin(async move {
                let gate = self.gate.lock().unwrap().clone();
                if let Some(gate) = gate {
                    self.save_started.notify_one();
                    let _ = gate.acquire().await;
                }
                if self.fail.load(Ordering::Acquire) {
                    return CommitOutcome::Failed;
                }
                let mut value = self.value.lock().unwrap();
                if *value != expected {
                    return CommitOutcome::Conflict;
                }
                *value = Some(replacement);
                if self.unknown.load(Ordering::Acquire) {
                    CommitOutcome::Unknown
                } else {
                    CommitOutcome::Committed
                }
            })
        }
    }

    #[tokio::test]
    async fn suspend_resume_keeps_identity_and_stops_old_hub() {
        let store = Arc::new(Storage::default());
        let native = NativeHub::open(store.clone(), false).await.unwrap();
        let identity = native.identity().await.unwrap();
        let previous = native.hub().await.unwrap();
        native.suspend().await.unwrap();
        assert!(previous.cancellation_token().is_cancelled());
        assert!(native.hub().await.is_err());
        native.resume(false).await.unwrap();
        assert_eq!(native.identity().await.unwrap(), identity);
        native.close().await.unwrap();
        assert!(native.resume(false).await.is_err());
        let reopened = NativeHub::open(store, false).await.unwrap();
        assert_eq!(reopened.identity().await.unwrap(), identity);
        reopened.close().await.unwrap();
    }

    #[tokio::test]
    async fn failed_forget_stays_disabled_until_explicit_retry() {
        let store = Arc::new(Storage::default());
        let mut identity = NativeIdentity::open(store.clone()).await.unwrap();
        let id = SecretKey::generate().public();
        identity
            .remember(SavedGateway {
                address: id.into(),
                name: "offline gateway".into(),
            })
            .await
            .unwrap();
        let native = NativeHub::open(store.clone(), false).await.unwrap();
        store.fail.store(true, Ordering::Release);
        assert!(native.forget(id).await.is_err());
        assert_eq!(native.peers().await.unwrap().len(), 1);
        {
            let state = native.inner.state.lock().await;
            assert!(state.blocked.contains(&id));
            assert!(
                !state
                    .running
                    .as_ref()
                    .unwrap()
                    .source
                    .load()
                    .await
                    .unwrap()
                    .peers
                    .contains_key(&id.to_string())
            );
        }
        native.resume(false).await.unwrap();
        assert!(native.inner.state.lock().await.blocked.contains(&id));
        store.fail.store(false, Ordering::Release);
        native.forget(id).await.unwrap();
        assert!(native.peers().await.unwrap().is_empty());
        native.close().await.unwrap();
        let reopened = NativeHub::open(store, false).await.unwrap();
        assert!(reopened.peers().await.unwrap().is_empty());
        reopened.close().await.unwrap();
    }

    #[tokio::test]
    async fn caller_cancellation_cannot_abandon_dispatched_save() {
        let store = Arc::new(Storage::default());
        let native = NativeHub::open(store.clone(), false).await.unwrap();
        let gate = Arc::new(Semaphore::new(0));
        *store.gate.lock().unwrap() = Some(gate.clone());
        let entered = Arc::new(Semaphore::new(0));
        let observed = entered.clone();
        let copy = native.clone();
        let id = SecretKey::generate().public();
        let caller = tokio::spawn(async move {
            let owned = copy.clone();
            copy.command(async move {
                let mut state = owned.inner.state.lock().await;
                entered.add_permits(1);
                state
                    .identity
                    .remember(SavedGateway {
                        address: id.into(),
                        name: "gateway".into(),
                    })
                    .await?;
                Ok(())
            })
            .await
        });
        let _ = observed.acquire().await.unwrap();
        caller.abort();
        let _ = caller.await;
        let stopping = tokio::spawn({
            let copy = native.clone();
            async move { copy.close().await }
        });
        tokio::task::yield_now().await;
        assert!(!stopping.is_finished());
        gate.add_permits(1);
        stopping.await.unwrap().unwrap();
        let reopened = NativeHub::open(store, false).await.unwrap();
        assert_eq!(reopened.peers().await.unwrap()[0].address.id, id);
        reopened.close().await.unwrap();
    }

    #[tokio::test]
    async fn restart_cancels_old_pairing_generation_and_preserves_newer_suspension() {
        for reload in [false, true] {
            for suspend_during_restart in [false, true] {
                let store = Arc::new(Storage::default());
                let native = NativeHub::open(store.clone(), false).await.unwrap();
                let gate = Arc::new(Semaphore::new(0));
                *store.read_gate.lock().unwrap() = Some(gate.clone());
                let restart = tokio::spawn({
                    let native = native.clone();
                    async move {
                        if reload {
                            native.reload_authorization().await
                        } else {
                            native.resume(false).await
                        }
                    }
                });
                store.read_started.notified().await;
                // Enrollment must never begin for an operation admitted against
                // the old lifetime while restart owns the state lock.
                let invite: pairing::Invite = serde_json::from_value(serde_json::json!({
                    "version": omochi_protocol::VERSION,
                    "address": omochi_transport::EndpointAddr::from(SecretKey::generate().public()),
                    "secret": "0".repeat(64),
                    "expires": u64::MAX,
                }))
                .unwrap();
                let mut queued =
                    Box::pin(native.pair(invite.encode().unwrap(), CancellationToken::new()));
                assert!(futures_util::poll!(&mut queued).is_pending());
                let mut suspension = Box::pin(native.suspend());
                if suspend_during_restart {
                    assert!(futures_util::poll!(&mut suspension).is_pending());
                }
                gate.add_permits(1);
                restart.await.unwrap().unwrap();
                let result = tokio::time::timeout(Duration::from_secs(5), queued)
                    .await
                    .expect("old pairing generation must not start enrollment");
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("pairing cancelled")
                );
                assert_eq!(
                    native
                        .inner
                        .admission
                        .lock()
                        .unwrap()
                        .pairing
                        .is_cancelled(),
                    suspend_during_restart
                );
                suspension.await.unwrap();
                assert!(native.hub().await.is_err());
                native.close().await.unwrap();
            }
        }
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn suspend_cancels_enrollment_and_queued_pairs_until_resume() {
        use omochi_transport::store::Store;
        let directory = tempfile::tempdir().unwrap();
        let gateway_store = Store::open(directory.path().join("gateway")).unwrap();
        let gateway =
            omochi_transport::bind(gateway_store.identity(Role::Gateway).unwrap(), true, None)
                .await
                .unwrap();
        let invitation = pairing::Invite::issue(&gateway_store, &gateway)
            .unwrap()
            .encode()
            .unwrap();
        let native = NativeHub::open(Arc::new(Storage::default()), false)
            .await
            .unwrap();
        let identity = native.identity().await.unwrap();
        let previous = native.hub().await.unwrap();
        let pairing = tokio::spawn({
            let native = native.clone();
            let invitation = invitation.clone();
            async move { native.pair(invitation, CancellationToken::new()).await }
        });
        // Wait for the actual authenticated request, then withhold its response.
        let connection = tokio::time::timeout(Duration::from_secs(5), async {
            gateway.accept().await.unwrap().await.unwrap()
        })
        .await
        .unwrap();
        assert_eq!(connection.remote_id().to_string(), identity);
        let (_tx, mut rx) = connection.accept_bi().await.unwrap();
        let _: serde_json::Value = omochi_protocol::read_frame(&mut rx).await.unwrap().unwrap();
        let mut queued = Box::pin(native.pair(invitation.clone(), CancellationToken::new()));
        assert!(futures_util::poll!(&mut queued).is_pending());
        let mut suspension = Box::pin(native.suspend());
        assert!(futures_util::poll!(&mut suspension).is_pending());
        assert!(
            native
                .inner
                .admission
                .lock()
                .unwrap()
                .pairing
                .is_cancelled()
        );
        let mut late = Box::pin(native.pair(invitation.clone(), CancellationToken::new()));
        assert!(futures_util::poll!(&mut late).is_pending());
        tokio::time::timeout(Duration::from_secs(5), suspension)
            .await
            .expect("suspend must not wait for the enrollment timeout")
            .unwrap();
        for result in [pairing.await.unwrap(), queued.await, late.await] {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("pairing cancelled")
            );
        }
        assert!(previous.cancellation_token().is_cancelled());
        assert!(native.hub().await.is_err());
        assert!(native.peers().await.unwrap().is_empty());
        tokio::time::timeout(Duration::from_secs(5), connection.closed())
            .await
            .expect("cancelled enrollment connection must close");

        native.resume(false).await.unwrap();
        assert_eq!(native.identity().await.unwrap(), identity);
        let server = tokio::spawn({
            let gateway = gateway.clone();
            async move {
                let connection = gateway.accept().await.unwrap().await.unwrap();
                pairing::accept(connection, gateway_store).await.unwrap();
            }
        });
        native
            .pair(invitation, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(native.peers().await.unwrap()[0].address.id, gateway.id());
        server.await.unwrap();
        native.close().await.unwrap();
        gateway.close().await;
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn suspend_waits_for_dispatched_pairing_save() {
        use omochi_transport::store::Store;
        let directory = tempfile::tempdir().unwrap();
        let gateway_store = Store::open(directory.path().join("gateway")).unwrap();
        let gateway =
            omochi_transport::bind(gateway_store.identity(Role::Gateway).unwrap(), true, None)
                .await
                .unwrap();
        let invitation = pairing::Invite::issue(&gateway_store, &gateway)
            .unwrap()
            .encode()
            .unwrap();
        let store = Arc::new(Storage::default());
        let native = NativeHub::open(store.clone(), false).await.unwrap();
        let previous = native.hub().await.unwrap();
        let gate = Arc::new(Semaphore::new(0));
        *store.gate.lock().unwrap() = Some(gate.clone());
        let server = tokio::spawn({
            let gateway = gateway.clone();
            async move {
                let connection = gateway.accept().await.unwrap().await.unwrap();
                pairing::accept(connection, gateway_store).await.unwrap();
            }
        });
        let pairing = tokio::spawn({
            let native = native.clone();
            async move { native.pair(invitation, CancellationToken::new()).await }
        });
        store.save_started.notified().await;
        let mut suspension = Box::pin(native.suspend());
        assert!(futures_util::poll!(&mut suspension).is_pending());
        assert!(
            native
                .inner
                .admission
                .lock()
                .unwrap()
                .pairing
                .is_cancelled()
        );
        assert!(!pairing.is_finished());
        gate.add_permits(1);
        pairing.await.unwrap().unwrap();
        suspension.await.unwrap();
        assert!(previous.cancellation_token().is_cancelled());
        assert!(native.hub().await.is_err());
        assert_eq!(native.peers().await.unwrap()[0].address.id, gateway.id());
        native.resume(false).await.unwrap();
        assert_eq!(native.peers().await.unwrap()[0].address.id, gateway.id());
        server.await.unwrap();
        native.close().await.unwrap();
        gateway.close().await;
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn real_enrollment_requires_confirmed_storage_and_recovers_unknown_results() {
        use omochi_transport::store::Store;
        for (outcome, forget_before_reload) in [
            (CommitOutcome::Committed, false),
            (CommitOutcome::Failed, false),
            (CommitOutcome::Unknown, false),
            (CommitOutcome::Unknown, true),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let gateway_store = Store::open(directory.path().join("gateway")).unwrap();
            let gateway =
                omochi_transport::bind(gateway_store.identity(Role::Gateway).unwrap(), true, None)
                    .await
                    .unwrap();
            let invitation = pairing::Invite::issue(&gateway_store, &gateway)
                .unwrap()
                .encode()
                .unwrap();
            let gateway_id = gateway.id();
            let store = Arc::new(Storage::default());
            let native = NativeHub::open(store.clone(), false).await.unwrap();
            let hub_id = native.identity().await.unwrap();
            store
                .fail
                .store(outcome == CommitOutcome::Failed, Ordering::Release);
            store
                .unknown
                .store(outcome == CommitOutcome::Unknown, Ordering::Release);
            let server = tokio::spawn({
                let gateway_store = gateway_store.clone();
                async move {
                    let connection = gateway.accept().await.unwrap().await.unwrap();
                    pairing::accept(connection, gateway_store).await.unwrap();
                    gateway.close().await;
                }
            });
            let result = native.pair(invitation, CancellationToken::new()).await;
            if outcome == CommitOutcome::Committed {
                result.unwrap();
                assert_eq!(native.peers().await.unwrap().len(), 1);
                assert_eq!(
                    native
                        .inner
                        .state
                        .lock()
                        .await
                        .running
                        .as_ref()
                        .unwrap()
                        .source
                        .load()
                        .await
                        .unwrap()
                        .peers
                        .len(),
                    1
                );
            } else {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("local registration was not confirmed")
                );
                if outcome == CommitOutcome::Failed {
                    assert!(native.peers().await.unwrap().is_empty());
                } else {
                    assert!(native.peers().await.is_err());
                }
                assert!(
                    native
                        .inner
                        .state
                        .lock()
                        .await
                        .running
                        .as_ref()
                        .unwrap()
                        .source
                        .load()
                        .await
                        .unwrap()
                        .peers
                        .is_empty()
                );
            }
            // The remote half succeeded. Retrying must use this same persisted identity.
            assert_eq!(gateway_store.peers().unwrap().1[&hub_id].role, Role::Hub);
            if outcome == CommitOutcome::Unknown {
                if forget_before_reload {
                    // A subsequent forget intent must win over pending pairing
                    // recovery, even when deletion itself needs a reload first.
                    assert!(native.forget(gateway_id).await.is_err());
                }
                native.reload_authorization().await.unwrap();
                assert_eq!(native.peers().await.unwrap().len(), 1);
                let state = native.inner.state.lock().await;
                assert_eq!(state.blocked.is_empty(), !forget_before_reload);
                assert_eq!(
                    state
                        .running
                        .as_ref()
                        .unwrap()
                        .source
                        .load()
                        .await
                        .unwrap()
                        .peers
                        .len(),
                    usize::from(!forget_before_reload)
                );
            }
            native.close().await.unwrap();
            server.await.unwrap();
            store.fail.store(false, Ordering::Release);
            let reopened = NativeHub::open(store, false).await.unwrap();
            assert_eq!(reopened.identity().await.unwrap(), hub_id);
            assert_eq!(
                reopened.peers().await.unwrap().len(),
                usize::from(outcome != CommitOutcome::Failed)
            );
            reopened.close().await.unwrap();
        }
    }
}
