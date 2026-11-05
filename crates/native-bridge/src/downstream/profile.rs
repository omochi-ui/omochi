//! Secure profile persistence. CAS outcomes are never guessed or recovered from stale state.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use omochi_transport::SecretKey;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub(super) link: Link,
    pub(super) accepted: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Profile {
    pub(super) version: u8,
    pub(super) key: String,
    pub(super) records: Vec<Record>,
}
impl Profile {
    pub(super) fn key(&self) -> Result<SecretKey> {
        let bytes: [u8; 32] = STANDARD
            .decode(&self.key)
            .map_err(|_| anyhow::anyhow!(Fault::StorageCorrupt))?
            .try_into()
            .map_err(|_| anyhow::anyhow!(Fault::StorageCorrupt))?;
        anyhow::ensure!(STANDARD.encode(bytes) == self.key, Fault::StorageCorrupt);
        Ok(SecretKey::from_bytes(&bytes))
    }
    pub(super) async fn load(store: &CallbackStore) -> Result<(Self, Option<String>)> {
        let raw = store.read().await?;
        let profile: Self = match &raw {
            Some(raw) => {
                serde_json::from_str(raw).map_err(|_| anyhow::anyhow!(Fault::StorageCorrupt))?
            }
            None => Self {
                version: 1,
                key: STANDARD.encode(SecretKey::generate().to_bytes()),
                records: vec![],
            },
        };
        anyhow::ensure!(
            profile.version == 1 && profile.records.len() <= 1000,
            Fault::StorageCorrupt
        );
        profile.key()?;
        let mut ids = std::collections::BTreeSet::new();
        for record in &profile.records {
            anyhow::ensure!(ids.insert(record.link.id()), Fault::StorageCorrupt);
            record
                .link
                .validate()
                .map_err(|_| anyhow::anyhow!(Fault::StorageCorrupt))?;
        }
        Ok((profile, raw))
    }
    pub(super) async fn save(&self, store: &CallbackStore, expected: Option<String>) -> Result<()> {
        let replacement = serde_json::to_string(self)?;
        anyhow::ensure!(replacement.len() <= 1024 * 1024, Fault::CapacityExceeded);
        match store.compare_exchange(expected, replacement).await {
            CommitOutcome::Committed => Ok(()),
            CommitOutcome::Conflict => bail!(Fault::StorageConflict),
            CommitOutcome::Failed => bail!(Fault::StorageUnavailable),
            CommitOutcome::Unknown => bail!(Fault::StorageOutcomeUnknown),
        }
    }
}
