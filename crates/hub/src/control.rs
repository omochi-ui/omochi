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
                tokio::task::spawn_blocking(move || control.execute_sync(command))
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
            tokio::task::spawn_blocking(move || grants.authorize(&auth, &who, || Ok(())))
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
        if matches!(command, Command::ShareRevoke { .. } | Command::Connections) {
            let owners = self.boards.owners();
            tokio::task::spawn_blocking(move || owners.authorize(&id, &peer, || Ok(())))
                .await
                .map_err(|_| ApiError::new(503, "Owner authorization unavailable"))??;
            return self.execute(command).await;
        }
        let control = self.clone();
        let owners = self.boards.owners();
        tokio::task::spawn_blocking(move || {
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
