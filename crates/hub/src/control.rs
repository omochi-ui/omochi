//! Transport-independent owner operations. Authentication is supplied by the caller.
use crate::{
    Hub,
    dashboards::{ApiError, DashboardStore},
};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "camelCase", deny_unknown_fields)]
pub enum Command {
    GatewayPairStart {
        invitation: String,
        name: String,
    },
    GatewayPairStatus {
        id: String,
    },
    GatewayPairRetrySave {
        id: String,
    },
    BoardsList,
    BoardGet {
        id: String,
    },
    BoardCreate {
        value: Value,
    },
    BoardUpdate {
        id: String,
        value: Value,
    },
    BoardDelete {
        id: String,
        value: Value,
    },
    Presets,
    Connections,
    Sharing,
    SharesList,
    ShareCreate {
        board_id: String,
        public: bool,
        permissions: crate::sharing::Permissions,
    },
    ShareRevoke {
        id: String,
    },
}
#[derive(Clone)]
pub struct HubControl {
    pub hub: Hub,
    pub boards: DashboardStore,
}
impl HubControl {
    pub async fn board_async(
        &self,
        id: Option<String>,
        value: Option<Value>,
        remove: bool,
    ) -> Result<Value, ApiError> {
        let control = self.clone();
        storage(move || control.board(id.as_deref(), value, remove)).await
    }
    pub async fn owners_list(&self) -> Result<Value, ApiError> {
        let store = self.boards.owners();
        storage(move || Ok(json!(store.list()?))).await
    }
    pub async fn owner_create(&self) -> Result<(crate::owners::OwnerGrant, String), ApiError> {
        let store = self.boards.owners();
        storage(move || store.create()).await
    }
    pub async fn shared_layout(
        &self,
        id: String,
        peer: String,
        value: Value,
    ) -> Result<crate::dashboards::Dashboard, ApiError> {
        let boards = self.boards.clone();
        storage(move || boards.shares().edit_layout(&boards, &id, &peer, value)).await
    }
    pub fn board(
        &self,
        id: Option<&str>,
        value: Option<Value>,
        remove: bool,
    ) -> Result<Value, ApiError> {
        if let Some(value) = value {
            return Ok(json!(self.boards.mutate(id, value, remove)?));
        }
        let rows = self.boards.list()?;
        if let Some(id) = id {
            Ok(json!(rows.into_iter().find(|b| b.id == id).ok_or_else(
                || ApiError::new(404, "Dashboard not found")
            )?))
        } else {
            Ok(json!(rows))
        }
    }
    pub async fn execute(&self, command: Command) -> Result<Value, ApiError> {
        match command {
            Command::ShareRevoke { id } => {
                self.boards.shares().revoke_live(&id).await?;
                Ok(json!({"deleted":true}))
            }
            Command::Connections => Ok(json!(self.hub.connection_status().await)),
            command => {
                let control = self.clone();
                omochi_transport::blocking::run(move || control.execute_sync(command))
                    .await
                    .map_err(|_| ApiError::new(503, "Owner operation unavailable"))?
            }
        }
    }
    pub async fn execute_owner(
        &self,
        id: String,
        peer: String,
        command: Command,
    ) -> Result<Value, ApiError> {
        if matches!(
            command,
            Command::GatewayPairStart { .. }
                | Command::GatewayPairStatus { .. }
                | Command::GatewayPairRetrySave { .. }
        ) {
            let owners = self.boards.owners();
            let (auth, who, grants) = (id.clone(), peer.clone(), owners.clone());
            omochi_transport::blocking::run(move || grants.authorize(&auth, &who, || Ok(())))
                .await
                .map_err(|_| ApiError::new(503, "Owner authorization unavailable"))??;
            #[cfg(feature = "desktop")]
            {
                let service = self
                    .hub
                    .pairing
                    .read()
                    .unwrap()
                    .clone()
                    .ok_or_else(|| ApiError::new(503, "Gateway registration unavailable"))?;
                let retry = matches!(command, Command::GatewayPairRetrySave { .. });
                let view = service
                    .command(
                        self.hub.clone(),
                        owners.clone(),
                        id.clone(),
                        peer.clone(),
                        command,
                    )
                    .await?;
                if retry {
                    service.retry(owners.clone()).await;
                    return service
                        .command(
                            self.hub.clone(),
                            owners,
                            id,
                            peer,
                            Command::GatewayPairStatus {
                                id: view["id"].as_str().unwrap().to_owned(),
                            },
                        )
                        .await;
                }
                return Ok(view);
            }
            #[cfg(not(feature = "desktop"))]
            return Err(ApiError::new(503, "Gateway registration unavailable"));
        }
        if let Command::ShareRevoke { id: share_id } = command {
            let owners = self.boards.owners();
            let shares = self.boards.shares();
            let revocation = storage(move || {
                owners.authorize(&id, &peer, || shares.begin_revocation(&share_id))
            })
            .await?;
            revocation.wait(std::time::Duration::from_secs(10)).await?;
            return Ok(json!({"deleted":true}));
        }
        if matches!(command, Command::Connections) {
            let owners = self.boards.owners();
            omochi_transport::blocking::run(move || owners.authorize(&id, &peer, || Ok(())))
                .await
                .map_err(|_| ApiError::new(503, "Owner authorization unavailable"))??;
            return self.execute(command).await;
        }
        let control = self.clone();
        let owners = self.boards.owners();
        omochi_transport::blocking::run(move || {
            owners.authorize(&id, &peer, || control.execute_sync(command))
        })
        .await
        .map_err(|_| ApiError::new(503, "Owner operation unavailable"))?
    }
    fn execute_sync(&self, command: Command) -> Result<Value, ApiError> {
        match command {
            Command::GatewayPairStart { .. }
            | Command::GatewayPairStatus { .. }
            | Command::GatewayPairRetrySave { .. } => {
                Err(ApiError::new(403, "Owner access required"))
            }
            Command::BoardsList => self.board(None, None, false),
            Command::BoardGet { id } => self.board(Some(&id), None, false),
            Command::BoardCreate { value } => self.board(None, Some(value), false),
            Command::BoardUpdate { id, value } => self.board(Some(&id), Some(value), false),
            Command::BoardDelete { id, value } => self.board(Some(&id), Some(value), true),
            Command::Presets => Ok(json!(self.hub.owner_presets.read().unwrap().clone())),
            Command::Connections | Command::ShareRevoke { .. } => {
                unreachable!("async commands are dispatched before storage work")
            }
            Command::Sharing => Ok(self
                .hub
                .dashboard_host()
                .unwrap_or(json!({"enabled":false}))),
            Command::SharesList => Ok(json!(self.boards.shares().list()?)),
            Command::ShareCreate {
                board_id,
                public,
                permissions,
            } => {
                let (grant, secret) =
                    self.boards
                        .shares()
                        .create(&self.boards, &board_id, public, permissions)?;
                Ok(json!({"url":self.hub.share_link(&grant,&secret),"grant":grant,"secret":secret}))
            }
        }
    }
}

/// Keep blocking file transactions off the async executor, with one error contract.
pub(crate) async fn storage<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    omochi_transport::blocking::run(operation)
        .await
        .map_err(|_| ApiError::new(503, "Storage operation unavailable"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sharing::Permissions, sharing_live::Lease};
    use fs2::FileExt;
    use std::{fs::OpenOptions, time::Duration};

    fn setup() -> (tempfile::TempDir, HubControl, String, String, String) {
        let root = tempfile::tempdir().unwrap();
        let boards = DashboardStore::open(root.path().join("state")).unwrap();
        let board = boards
            .mutate(None, json!({"name":"Shared","panels":[]}), false)
            .unwrap()
            .unwrap();
        let (share, _) = boards
            .shares()
            .create(&boards, &board.id, true, Permissions::default())
            .unwrap();
        let (owner, secret) = boards.owners().create().unwrap();
        let peer = "a".repeat(64);
        boards.owners().redeem(&owner.id, &secret, &peer).unwrap();
        let control = HubControl {
            hub: Hub::new(),
            boards,
        };
        (root, control, owner.id, peer, share.id)
    }

    #[tokio::test]
    async fn pending_share_removal_holds_owner_authority_until_the_write_finishes() {
        let (root, control, owner, peer, share) = setup();
        let shares_lock =
            omochi_transport::persistence::lock(&root.path().join("state/shares.lock")).unwrap();
        let owners_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.path().join("state/owners.lock"))
            .unwrap();
        let copy = control.clone();
        let id = owner.clone();
        let (ready, release) =
            crate::sharing_live::test_hooks::pause_revocation(&control.boards.shares());
        let removal = tokio::spawn(async move {
            copy.execute_owner(id, peer, Command::ShareRevoke { id: share })
                .await
        });
        // Pause at the durable phase, after authorization has been dispatched.
        // This cannot accidentally observe the old, transient preflight lock.
        tokio::time::timeout(Duration::from_secs(5), ready)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            owners_lock.try_lock_exclusive(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ));
        let owners = control.boards.owners();
        let (started, ready) = tokio::sync::oneshot::channel();
        let revoke_owner = omochi_transport::blocking::run(move || {
            started.send(()).unwrap();
            owners.revoke(&owner)
        });
        ready.await.unwrap();
        assert!(matches!(
            owners_lock.try_lock_exclusive(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ));
        assert!(!revoke_owner.is_finished());
        release.send(()).unwrap();
        drop(shares_lock);
        tokio::time::timeout(Duration::from_secs(5), async {
            assert_eq!(removal.await.unwrap().unwrap(), json!({"deleted":true}));
            revoke_owner.await.unwrap().unwrap();
        })
        .await
        .unwrap();
        assert!(control.boards.shares().list().unwrap().is_empty());
        assert!(control.boards.owners().list().unwrap().is_empty());
    }

    #[tokio::test]
    async fn revoked_owners_cannot_remove_shares() {
        let (_root, control, owner, peer, share) = setup();
        control.boards.owners().revoke(&owner).unwrap();
        assert_eq!(
            control
                .execute_owner(owner, peer, Command::ShareRevoke { id: share.clone() })
                .await
                .unwrap_err()
                .0,
            403
        );
        assert_eq!(control.boards.shares().list().unwrap()[0].id, share);
    }

    #[tokio::test]
    async fn share_removal_waits_for_live_ack_without_holding_owner_authority() {
        for remote in [false, true] {
            let (_root, control, owner, peer, share) = setup();
            let shares = control.boards.shares();
            let lease = Lease::register(&shares).unwrap();
            let copy = control.clone();
            let id = owner.clone();
            let removal = tokio::spawn(async move {
                let command = Command::ShareRevoke { id: share };
                if remote {
                    copy.execute_owner(id, peer, command).await
                } else {
                    copy.execute(command).await
                }
            });
            let generation = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let copy = shares.clone();
                    let generation = storage(move || copy.generation()).await.unwrap();
                    if generation > 0 {
                        break generation;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let owners = control.boards.owners();
            tokio::time::timeout(
                Duration::from_secs(5),
                storage(move || owners.revoke(&owner)),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(
                !removal.is_finished(),
                "live share teardown is still unacknowledged"
            );
            lease.acknowledge(generation).unwrap();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(5), removal)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap(),
                json!({"deleted":true})
            );
        }
    }

    #[tokio::test]
    async fn share_removal_propagates_ack_errors_after_durable_removal() {
        for remote in [false, true] {
            let (root, control, owner, peer, share) = setup();
            let shares = control.boards.shares();
            let _lease = Lease::register(&shares).unwrap();
            let path =
                omochi_transport::persistence::leases(&root.path().join("state"), "share-live-")
                    .unwrap()
                    .pop()
                    .unwrap();
            // A failed lease read must not be mistaken for a teardown ACK.
            std::fs::remove_file(&path).unwrap();
            std::fs::create_dir(&path).unwrap();
            let command = Command::ShareRevoke { id: share };
            let error = if remote {
                control.execute_owner(owner, peer, command).await
            } else {
                control.execute(command).await
            }
            .unwrap_err();
            assert_eq!(error.0, 503);
            assert!(error.1.contains("teardown was not confirmed"));
            assert!(shares.list().unwrap().is_empty());
        }
    }
}
