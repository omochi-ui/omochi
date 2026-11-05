//! Native downstream adapter. Shared rules live in omochi-downstream-core.
mod connection;
mod invitation;
mod profile;

use super::{CallbackStore, CommitOutcome, IdentityStore};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use connection::Active;
use invitation::Link;
use omochi_downstream_core::{Command, Fault, Mode, Rpc, SessionState, Welcome};
use profile::{Profile, Record};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct Client {
    active: RwLock<Option<Arc<Active>>>,
    admin: AsyncMutex<()>,
}
impl Client {
    pub async fn shutdown(&self) {
        let _admin = self.admin.lock().await;
        self.close().await;
    }
    pub async fn lease_token(&self, lease: &str) -> Result<CancellationToken> {
        let active = self
            .active
            .read()
            .await
            .clone()
            .ok_or(Fault::ScopeExpired)?;
        let token = active
            .tokens
            .lock()
            .unwrap()
            .get(lease)
            .cloned()
            .ok_or(Fault::ScopeExpired)?;
        Ok(token)
    }
    pub async fn close(&self) {
        if let Some(active) = self.active.write().await.take() {
            active.stop.cancel();
            active.state.lock().await.close();
            for token in active.tokens.lock().unwrap().values() {
                token.cancel();
            }
            active.connection.close(0u32.into(), b"closed");
            active.endpoint.close().await;
        }
    }
    pub async fn execute(
        &self,
        request: Command,
        store: Arc<CallbackStore>,
        cancel: CancellationToken,
    ) -> Result<Value> {
        let result = tokio::select! {
            _ = cancel.cancelled() => { self.close().await; Err(Fault::Cancelled.into()) },
            result = self.execute_command(request, store) => result,
        };
        result.map_err(normalize_error)
    }
    async fn execute_command(&self, request: Command, store: Arc<CallbackStore>) -> Result<Value> {
        match request {
            Command::Parse { invitation } => Ok(Link::parse(&invitation)?.metadata()),
            Command::List {} => {
                let _admin = self.admin.lock().await;
                let (profile, _) = Profile::load(&store).await?;
                Ok(json!(
                    profile
                        .records
                        .iter()
                        .map(|r| {
                            let mut v = r.link.metadata();
                            v["accepted"] = r.accepted.into();
                            v
                        })
                        .collect::<Vec<_>>()
                ))
            }
            Command::Forget { id } => {
                let _admin = self.admin.lock().await;
                let (mut profile, expected) = Profile::load(&store).await?;
                self.close().await;
                profile.records.retain(|r| r.link.id() != id);
                profile.save(&store, expected).await?;
                Ok(Value::Null)
            }
            command @ (Command::Register { .. } | Command::Connect { .. }) => {
                let _admin = self.admin.lock().await;
                self.connect(command, store).await
            }
            Command::Disconnect {} => {
                self.close().await;
                Ok(Value::Null)
            }
            Command::Next { ack } => {
                let active = self
                    .active
                    .read()
                    .await
                    .clone()
                    .ok_or(Fault::SourceUnavailable)?;
                active.next(ack).await
            }
            Command::Rpc { value } => {
                let active = self
                    .active
                    .read()
                    .await
                    .clone()
                    .ok_or(Fault::SourceUnavailable)?;
                active.state.lock().await.authorize(&value)?;
                if matches!(value, Rpc::Image { .. }) {
                    active.image(value).await
                } else {
                    active.rpc(value).await
                }
            }
        }
    }
    #[cfg(test)]
    async fn execute_inner(&self, request: Value, store: Arc<CallbackStore>) -> Result<Value> {
        self.execute_command(serde_json::from_value(request)?, store)
            .await
    }
}

/// Errors crossing the C ABI always have an exact, non-sensitive category.
pub(super) fn normalize_error(error: anyhow::Error) -> anyhow::Error {
    if error.downcast_ref::<Fault>().is_some() {
        return error;
    }
    if error
        .downcast_ref::<tokio::time::error::Elapsed>()
        .is_some()
    {
        return Fault::Timeout.into();
    }
    if error.downcast_ref::<super::IdentityError>().is_some() {
        return Fault::StorageUnavailable.into();
    }
    Fault::NetworkUnavailable.into()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
