//! Hub-owned enrollment jobs; bearer invitations never appear in status replies.
use crate::{Hub, dashboards::ApiError, owners::OwnerStore};
use omochi_transport::{
    Endpoint, EndpointId,
    pairing::{self, Invite},
    store::{GatewayRegistrationError, Store},
};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Service {
    endpoint: Endpoint,
    store: Store,
    job: Arc<Mutex<Option<Job>>>,
}
struct Job {
    id: String,
    owner: String,
    peer: String,
    status: &'static str,
    gateway: EndpointId,
    name: String,
    revocation: u64,
    proof: bool,
    updated: Instant,
}
impl Job {
    fn view(&self) -> Value {
        json!({"id":self.id,"status":self.status,"gatewayId":self.gateway.to_string()})
    }
}
impl Hub {
    pub fn configure_pairing(&self, endpoint: Endpoint, store: Store) {
        *self.pairing.write().unwrap() = Some(Service {
            endpoint,
            store,
            job: Arc::default(),
        });
    }
}
fn unavailable() -> ApiError {
    ApiError::new(503, "Gateway registration unavailable")
}
fn registration_error(error: anyhow::Error) -> ApiError {
    match error.downcast_ref::<GatewayRegistrationError>() {
        Some(GatewayRegistrationError::InvalidName) => ApiError::new(400, "Invalid gateway name"),
        Some(GatewayRegistrationError::NameConflict) => {
            ApiError::new(409, "Gateway name already registered")
        }
        Some(GatewayRegistrationError::RoleConflict) => ApiError::new(409, "Peer role conflict"),
        Some(GatewayRegistrationError::AuthorizationChanged) => {
            ApiError::new(403, "Gateway authorization changed")
        }
        None => unavailable(),
    }
}
impl Service {
    pub async fn command(
        &self,
        hub: Hub,
        owners: OwnerStore,
        owner: String,
        peer: String,
        command: crate::control::Command,
    ) -> Result<Value, ApiError> {
        use crate::control::Command;
        let mut slot = self.job.lock().await;
        match command {
            Command::GatewayPairStart { invitation, name } => {
                let invite = Invite::decode(&invitation)
                    .map_err(|_| ApiError::new(400, "Invalid invitation"))?;
                if let Some(job) = slot.as_ref().filter(|j| {
                    j.owner == owner
                        && j.peer == peer
                        && j.gateway == invite.gateway_id()
                        && j.name == name
                        && (j.status == "pairing" || j.status == "savePending")
                }) {
                    return Ok(job.view());
                }
                if slot.as_ref().is_some_and(|j| {
                    j.status == "pairing"
                        || j.status == "savePending"
                        || j.updated.elapsed() < Duration::from_secs(300)
                            && (j.owner != owner || j.peer != peer)
                }) {
                    return Err(ApiError::new(409, "Gateway registration busy"));
                }

                if name.trim().is_empty() || name.len() > 256 {
                    return Err(ApiError::new(400, "Invalid gateway name"));
                }
                let store = self.store.clone();
                let (gateway, gateway_name) = (invite.gateway_id(), name.clone());
                let revocation = omochi_transport::blocking::run(move || {
                    store.prepare_gateway_registration(gateway, &gateway_name)
                })
                .await
                .map_err(|_| unavailable())?
                .map_err(registration_error)?;
                let job = Job {
                    id: uuid::Uuid::new_v4().to_string(),
                    owner,
                    peer,
                    status: "pairing",
                    gateway: invite.gateway_id(),
                    name,
                    revocation,
                    proof: false,
                    updated: Instant::now(),
                };
                let view = job.view();
                let id = job.id.clone();
                *slot = Some(job);
                drop(slot);
                let service = self.clone();
                hub.browser_tasks.clone().spawn(async move {
                    service.run(hub, owners, invite, id).await;
                });
                Ok(view)
            }
            Command::GatewayPairStatus { id } | Command::GatewayPairRetrySave { id } => {
                let job = slot
                    .as_mut()
                    .filter(|j| j.id == id && j.owner == owner && j.peer == peer)
                    .ok_or_else(|| ApiError::new(404, "Registration not found"))?;
                if job.updated.elapsed() > Duration::from_secs(300)
                    && job.status != "pairing"
                    && job.status != "savePending"
                {
                    return Err(ApiError::new(404, "Registration expired"));
                }
                // A status read is side-effect free; retry is dispatched separately below.
                Ok(job.view())
            }
            _ => Err(unavailable()),
        }
    }
    async fn save(&self, owners: OwnerStore) {
        let mut slot = self.job.lock().await;
        let Some(job) = slot.as_mut() else { return };
        if !job.proof {
            return;
        }
        let (owner, peer, store, id, name, revocation) = (
            job.owner.clone(),
            job.peer.clone(),
            self.store.clone(),
            job.gateway,
            job.name.clone(),
            job.revocation,
        );
        let result = omochi_transport::blocking::run(move || {
            owners.authorize(&owner, &peer, || {
                store
                    .register_gateway_guarded(id, &name, Some(revocation))
                    .map_err(registration_error)
            })
        })
        .await;
        job.status = match result {
            Ok(Ok(())) => {
                job.proof = false;
                "registered"
            }
            Ok(Err(error)) if matches!(error.0, 400 | 403 | 409) => {
                job.proof = false;
                "failed"
            }
            _ => "savePending",
        };
        job.updated = Instant::now();
    }
    pub async fn retry(&self, owners: OwnerStore) {
        self.save(owners).await;
    }
    async fn retain_proof(&self, owners: OwnerStore, id: &str) -> bool {
        let mut slot = self.job.lock().await;
        let Some(job) = slot.as_mut().filter(|job| job.id == id && job.proof) else {
            return false;
        };
        let (owner, peer, store, revocation) = (
            job.owner.clone(),
            job.peer.clone(),
            self.store.clone(),
            job.revocation,
        );
        let result = omochi_transport::blocking::run(move || {
            owners.authorize(&owner, &peer, || {
                if store.authorization().map_err(|_| unavailable())?.1 != revocation {
                    return Err(ApiError::new(403, "Gateway authorization changed"));
                }
                Ok(())
            })
        })
        .await;
        if matches!(result, Ok(Err(error)) if error.0 == 403) {
            job.proof = false;
            job.status = "failed";
            job.updated = Instant::now();
            return false;
        }
        // A failed read cannot establish revocation. Retain the proof; saving
        // still rechecks authority and the watermark under their respective locks.
        true
    }
    async fn run(&self, hub: Hub, owners: OwnerStore, invite: Invite, id: String) {
        let enrollment = pairing::enroll(&self.endpoint, invite, "Omochi Hub");
        tokio::pin!(enrollment);
        let mut check = tokio::time::interval(Duration::from_millis(250));
        let status = loop {
            tokio::select! {
                _ = hub.stop.cancelled() => break "outcomeUnknown",
                result = &mut enrollment => break match result {
                    Ok(_) => "savePending",
                    Err(pairing::EnrollmentError::OutcomeUnknown) => "outcomeUnknown",
                    Err(_) => "failed",
                },
                _ = check.tick() => {
                    let slot = self.job.lock().await;
                    let Some(job) = slot.as_ref() else {return};
                    let (owner,peer,owners)=(job.owner.clone(),job.peer.clone(),owners.clone());
                    drop(slot);
                    if !matches!(omochi_transport::blocking::run(move || owners.authorize(&owner,&peer,||Ok(()))).await,Ok(Ok(()))) { break "failed"; }
                }
            }
        };
        if let Some(job) = self.job.lock().await.as_mut() {
            job.status = status;
            job.proof = status == "savePending";
            job.updated = Instant::now();
        }
        if status == "savePending" {
            self.save(owners.clone()).await;
            // Drop retained proofs promptly when owner authority or trust changes.
            loop {
                tokio::select! { _ = hub.stop.cancelled() => break, _ = tokio::time::sleep(Duration::from_millis(250)) => {} }
                if !self.retain_proof(owners.clone(), &id).await {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{Command, HubControl};
    use omochi_transport::store::Role;
    #[tokio::test]
    async fn owner_job_registers_hub_identity_and_hides_invitation() {
        let temp = tempfile::tempdir().unwrap();
        let gateway_store = Store::open(temp.path().join("gateway")).unwrap();
        let hub_store = Store::open(temp.path().join("hub")).unwrap();
        let gateway =
            omochi_transport::bind(gateway_store.identity(Role::Gateway).unwrap(), true, None)
                .await
                .unwrap();
        let endpoint = omochi_transport::bind(hub_store.identity(Role::Hub).unwrap(), true, None)
            .await
            .unwrap();
        let gateway_id = gateway.id();
        let hub_id = endpoint.id();
        let invitation = Invite::issue(&gateway_store, &gateway)
            .unwrap()
            .encode()
            .unwrap();
        let boards = crate::dashboards::DashboardStore::open(temp.path().join("boards")).unwrap();
        let owners = boards.owners();
        let (grant, secret) = owners.create().unwrap();
        let peer = "a".repeat(64);
        owners.redeem(&grant.id, &secret, &peer).unwrap();
        let hub = Hub::new();
        hub.configure_pairing(endpoint.clone(), hub_store.clone());
        let control = HubControl {
            hub: hub.clone(),
            boards,
        };
        assert!(
            control
                .execute(Command::GatewayPairStart {
                    invitation: invitation.clone(),
                    name: "test".into()
                })
                .await
                .is_err()
        );
        assert!(
            control
                .execute_owner(
                    grant.id.clone(),
                    "b".repeat(64),
                    Command::GatewayPairStart {
                        invitation: invitation.clone(),
                        name: "test".into()
                    }
                )
                .await
                .is_err()
        );
        let existing = hub_store.identity(Role::Gateway).unwrap().public();
        hub_store.register_gateway(existing, "test").unwrap();
        let error = control
            .execute_owner(
                grant.id.clone(),
                peer.clone(),
                Command::GatewayPairStart {
                    invitation: invitation.clone(),
                    name: "test".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.0, 409);
        // Rejection happens before an enrollment task can contact the gateway.
        assert!(hub.browser_tasks.is_empty());
        let service = hub.pairing.read().unwrap().clone().unwrap();
        assert!(service.job.lock().await.is_none());
        hub_store.register_gateway(existing, "existing").unwrap();
        let gs = gateway_store.clone();
        let server = tokio::spawn(async move {
            let connection = gateway.accept().await.unwrap().await.unwrap();
            pairing::accept(connection, gs).await.unwrap();
            gateway
        });
        let job = control
            .execute_owner(
                grant.id.clone(),
                peer.clone(),
                Command::GatewayPairStart {
                    invitation: invitation.clone(),
                    name: "test".into(),
                },
            )
            .await
            .unwrap();
        assert!(!job.to_string().contains(&invitation));
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let status = control
                    .execute_owner(
                        grant.id.clone(),
                        peer.clone(),
                        Command::GatewayPairStatus {
                            id: job["id"].as_str().unwrap().into(),
                        },
                    )
                    .await
                    .unwrap();
                if status["status"] == "registered" {
                    break;
                }
                assert_eq!(status["status"], "pairing");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            gateway_store.peers().unwrap().1[&hub_id.to_string()].role,
            Role::Hub
        );
        assert_eq!(
            hub_store.peers().unwrap().1[&gateway_id.to_string()].role,
            Role::Gateway
        );
        owners.revoke(&grant.id).unwrap();
        assert!(
            control
                .execute_owner(
                    grant.id,
                    peer,
                    Command::GatewayPairStatus {
                        id: job["id"].as_str().unwrap().into()
                    }
                )
                .await
                .is_err()
        );
        hub.shutdown();
        endpoint.close().await;
        server.await.unwrap().close().await;
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use crate::control::{Command, HubControl};
    use omochi_transport::store::Role;

    async fn wait_for_status(
        control: &HubControl,
        owner: &str,
        peer: &str,
        id: &str,
        status: &str,
    ) {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let view = control
                    .execute_owner(
                        owner.into(),
                        peer.into(),
                        Command::GatewayPairStatus { id: id.into() },
                    )
                    .await
                    .unwrap();
                if view["status"] == status {
                    break;
                }
                assert!(view["status"] == "pairing" || view["status"] == "savePending");
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn name_conflict_during_enrollment_releases_job_for_its_owner() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().join("hub")).unwrap();
        let gateway_store = Store::open(temp.path().join("gateway")).unwrap();
        let endpoint = omochi_transport::bind(store.identity(Role::Hub).unwrap(), true, None)
            .await
            .unwrap();
        let gateway =
            omochi_transport::bind(gateway_store.identity(Role::Gateway).unwrap(), true, None)
                .await
                .unwrap();
        let gateway_id = gateway.id();
        let invitation = Invite::issue(&gateway_store, &gateway)
            .unwrap()
            .encode()
            .unwrap();
        let boards = crate::dashboards::DashboardStore::open(temp.path().join("boards")).unwrap();
        let owners = boards.owners();
        let (owner, secret) = owners.create().unwrap();
        let peer = "a".repeat(64);
        owners.redeem(&owner.id, &secret, &peer).unwrap();
        let (other_owner, other_secret) = owners.create().unwrap();
        let other_peer = "b".repeat(64);
        owners
            .redeem(&other_owner.id, &other_secret, &other_peer)
            .unwrap();
        let hub = Hub::new();
        hub.configure_pairing(endpoint.clone(), store.clone());
        let control = HubControl {
            hub: hub.clone(),
            boards,
        };
        let conflict_store = store.clone();
        let server = tokio::spawn(async move {
            let connection = gateway.accept().await.unwrap().await.unwrap();
            // The connection proves preflight has passed. Introduce the conflict
            // before allowing enrollment to complete and its proof to be saved.
            let other = conflict_store.identity(Role::Gateway).unwrap().public();
            conflict_store.register_gateway(other, "work").unwrap();
            pairing::accept(connection, gateway_store.clone())
                .await
                .unwrap();
            let connection = gateway.accept().await.unwrap().await.unwrap();
            pairing::accept(connection, gateway_store).await.unwrap();
            gateway
        });
        let first = control
            .execute_owner(
                owner.id.clone(),
                peer.clone(),
                Command::GatewayPairStart {
                    invitation: invitation.clone(),
                    name: "work".into(),
                },
            )
            .await
            .unwrap();
        let first_id = first["id"].as_str().unwrap();
        wait_for_status(&control, &owner.id, &peer, first_id, "failed").await;
        let service = hub.pairing.read().unwrap().clone().unwrap();
        assert!(!service.job.lock().await.as_ref().unwrap().proof);
        assert!(
            !store
                .peers()
                .unwrap()
                .1
                .contains_key(&gateway_id.to_string())
        );
        let error = control
            .execute_owner(
                other_owner.id.clone(),
                other_peer.clone(),
                Command::GatewayPairStatus {
                    id: first_id.into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.0, 404);
        let error = control
            .execute_owner(
                other_owner.id,
                other_peer,
                Command::GatewayPairStart {
                    invitation: invitation.clone(),
                    name: "personal".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.0, 409);
        let second = control
            .execute_owner(
                owner.id.clone(),
                peer.clone(),
                Command::GatewayPairStart {
                    invitation,
                    name: "personal".into(),
                },
            )
            .await
            .unwrap();
        assert_ne!(first["id"], second["id"]);
        wait_for_status(
            &control,
            &owner.id,
            &peer,
            second["id"].as_str().unwrap(),
            "registered",
        )
        .await;
        assert_eq!(
            store.peers().unwrap().1[&gateway_id.to_string()].name,
            "personal"
        );
        hub.shutdown();
        endpoint.close().await;
        server.await.unwrap().close().await;
    }

    #[tokio::test]
    async fn confirmed_proof_retries_locally_but_revocation_blocks_stale_save() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().join("hub")).unwrap();
        let target = Store::open(temp.path().join("target"))
            .unwrap()
            .identity(Role::Gateway)
            .unwrap()
            .public();
        let boards = crate::dashboards::DashboardStore::open(temp.path().join("boards")).unwrap();
        let owners = boards.owners();
        let (grant, secret) = owners.create().unwrap();
        let peer = "a".repeat(64);
        owners.redeem(&grant.id, &secret, &peer).unwrap();
        let endpoint = omochi_transport::bind(store.identity(Role::Hub).unwrap(), true, None)
            .await
            .unwrap();
        let service = Service {
            endpoint: endpoint.clone(),
            store: store.clone(),
            job: Arc::new(Mutex::new(Some(Job {
                id: "job".into(),
                owner: grant.id.clone(),
                peer,
                status: "savePending",
                gateway: target,
                name: "target".into(),
                revocation: store.authorization().unwrap().1,
                proof: true,
                updated: Instant::now(),
            }))),
        };
        // Make the durable state temporarily unreadable without discarding it.
        let path = temp.path().join("hub/state.json");
        let backup = temp.path().join("hub/state.backup");
        std::fs::rename(&path, &backup).unwrap();
        std::fs::create_dir(&path).unwrap();
        service.save(owners.clone()).await;
        assert_eq!(
            service.job.lock().await.as_ref().unwrap().status,
            "savePending"
        );
        assert!(service.job.lock().await.as_ref().unwrap().proof);
        assert!(service.retain_proof(owners.clone(), "job").await);
        assert!(!service.retain_proof(owners.clone(), "old-job").await);
        assert!(service.job.lock().await.as_ref().unwrap().proof);
        std::fs::remove_dir(&path).unwrap();
        std::fs::rename(&backup, &path).unwrap();
        service.save(owners.clone()).await;
        assert_eq!(
            service.job.lock().await.as_ref().unwrap().status,
            "registered"
        );
        {
            let mut slot = service.job.lock().await;
            let job = slot.as_mut().unwrap();
            job.proof = true;
            job.status = "savePending";
        }
        store.revoke_record(target).unwrap();
        service.save(owners.clone()).await;
        assert!(!store.peers().unwrap().1.contains_key(&target.to_string()));
        assert_eq!(service.job.lock().await.as_ref().unwrap().status, "failed");
        assert!(!service.job.lock().await.as_ref().unwrap().proof);
        {
            let mut slot = service.job.lock().await;
            let job = slot.as_mut().unwrap();
            job.proof = true;
            job.status = "savePending";
            job.revocation = store.authorization().unwrap().1;
        }
        owners.revoke(&grant.id).unwrap();
        assert!(!service.retain_proof(owners, "job").await);
        let slot = service.job.lock().await;
        assert_eq!(slot.as_ref().unwrap().status, "failed");
        assert!(!slot.as_ref().unwrap().proof);
        endpoint.close().await;
    }
}
