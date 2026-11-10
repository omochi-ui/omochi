//! Hub-owned grants. Layout references are never credentials and gateway trust is separate.
use crate::dashboards::ApiError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, ApiError>;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerGrant {
    pub id: String,
    pub peer: Option<String>,
    pub invitation_expires_at: u64,
}
// Disk encoding is distinct: public responses never include credential digests.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    grant: OwnerGrant,
    digest: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Storage {
    version: u32,
    #[serde(default)]
    generation: u64,
    records: Vec<Record>,
}
#[derive(Clone)]
pub struct OwnerStore {
    root: PathBuf,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn hash(secret: &str) -> String {
    Sha256::digest(secret.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn denied() -> ApiError {
    ApiError::new(403, "Shared access is unavailable")
}
impl OwnerStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }
    fn transaction<T>(&self, operation: impl FnOnce(&mut Storage) -> Result<T>) -> Result<T> {
        let _lock = omochi_transport::persistence::lock(&self.root.join("owners.lock"))?;
        let bytes = (|| -> std::io::Result<Vec<u8>> {
            let file = File::open(self.root.join("owners.json"))?;
            let mut bytes = Vec::new();
            file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
            Ok(bytes)
        })();
        let mut storage: Storage = match bytes {
            Ok(bytes) if bytes.len() <= 1024 * 1024 => serde_json::from_slice(&bytes)
                .map_err(|_| ApiError::new(503, "Invalid owner storage"))?,
            Ok(_) => return Err(ApiError::new(503, "Sharing storage exceeds capacity")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Storage {
                version: 1,
                generation: 0,
                records: vec![],
            },
            Err(error) => return Err(error.into()),
        };
        if storage.version != 1 || storage.records.len() > 1000 {
            return Err(ApiError::new(503, "Invalid owner storage"));
        }
        let mut ids = std::collections::BTreeSet::new();
        for record in &storage.records {
            if uuid::Uuid::parse_str(&record.grant.id).is_err()
                || !ids.insert(&record.grant.id)
                || record.digest.len() != 64
                || !record
                    .digest
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                || record
                    .grant
                    .peer
                    .as_ref()
                    .is_some_and(|peer| !omochi_protocol::session::valid_node_id(peer))
            {
                return Err(ApiError::new(503, "Invalid owner storage"));
            }
        }
        let before = serde_json::to_vec(&storage)
            .map_err(|_| ApiError::new(503, "Invalid owner storage"))?;
        let previous: Vec<_> = storage
            .records
            .iter()
            .map(|record| record.grant.clone())
            .collect();
        let result = operation(&mut storage)?;
        let mut bytes = serde_json::to_vec(&storage)
            .map_err(|_| ApiError::new(503, "Invalid owner storage"))?;
        if bytes != before {
            // Adding a grant or binding a pending invitation never withdraws old
            // authority. Retire sessions only when existing authority is narrowed.
            let withdrawal = previous.iter().any(|old| {
                storage
                    .records
                    .iter()
                    .find(|r| r.grant.id == old.id)
                    .is_none_or(|record| {
                        let new = &record.grant;
                        old.peer.is_some() && old.peer != new.peer
                    })
            });
            if withdrawal {
                storage.generation = storage
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| ApiError::new(503, "Sharing generation exhausted"))?;
            }
            bytes = serde_json::to_vec(&storage)
                .map_err(|_| ApiError::new(503, "Invalid owner storage"))?;
            if bytes.len() > 1024 * 1024 {
                return Err(ApiError::new(503, "Sharing storage exceeds capacity"));
            }
            omochi_transport::persistence::atomic_write(&self.root.join("owners.json"), &bytes)?;
        }
        Ok(result)
    }
    pub(crate) fn directory(&self) -> &std::path::Path {
        &self.root
    }
    pub(crate) fn with_generation<T>(&self, f: impl FnOnce(u64) -> Result<T>) -> Result<T> {
        self.transaction(|s| f(s.generation))
    }
    pub(crate) fn generation(&self) -> Result<u64> {
        self.with_generation(Ok)
    }
    pub fn list(&self) -> Result<Vec<OwnerGrant>> {
        self.transaction(|s| Ok(s.records.iter().map(|r| r.grant.clone()).collect()))
    }
    pub fn create(&self) -> Result<(OwnerGrant, String)> {
        let secret = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let grant = OwnerGrant {
            id: uuid::Uuid::new_v4().to_string(),
            peer: None,
            invitation_expires_at: now() + 180,
        };
        self.transaction(|s| {
            if s.records.len() >= 1000 {
                return Err(ApiError::new(503, "Sharing capacity reached"));
            }
            s.records.push(Record {
                grant: grant.clone(),
                digest: hash(&secret),
            });
            Ok(())
        })?;
        Ok((grant, secret))
    }
    pub fn revoke(&self, id: &str) -> Result<()> {
        self.transaction(|s| {
            let old = s.records.len();
            s.records.retain(|r| r.grant.id != id);
            if old == s.records.len() {
                return Err(ApiError::new(404, "Share not found"));
            }
            Ok(())
        })
    }
    pub fn redeem(&self, id: &str, secret: &str, peer: &str) -> Result<OwnerGrant> {
        if !omochi_protocol::session::valid_node_id(peer) {
            return Err(denied());
        }
        self.transaction(|s| {
            let r = s
                .records
                .iter_mut()
                .find(|r| r.grant.id == id)
                .ok_or_else(denied)?;
            if r.digest != hash(secret)
                || now() >= r.grant.invitation_expires_at
                || r.grant.peer.as_deref().is_some_and(|p| p != peer)
            {
                return Err(denied());
            }
            r.grant.peer = Some(peer.into());
            Ok(r.grant.clone())
        })
    }
    pub fn authorize<T>(
        &self,
        id: &str,
        peer: &str,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.transaction(|s| {
            let record = s
                .records
                .iter()
                .find(|r| r.grant.id == id)
                .ok_or_else(denied)?;
            if record.grant.peer.as_deref() != Some(peer) {
                return Err(denied());
            }
            operation()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owners_are_separate_single_identity_grants_and_revocation_never_restores_stale_records() {
        let root = tempfile::tempdir().unwrap();
        omochi_local::files::private_directory(root.path()).unwrap();
        let store = OwnerStore::new(root.path().to_owned());
        let other = store.clone();
        let (grant, secret) = store.create().unwrap();
        let peer = "a".repeat(64);
        assert!(store.authorize(&grant.id, &peer, || Ok(())).is_err());
        assert!(store.redeem(&grant.id, &"0".repeat(64), &peer).is_err());
        store.redeem(&grant.id, &secret, &peer).unwrap();
        assert!(other.redeem(&grant.id, &secret, &"b".repeat(64)).is_err());
        other.authorize(&grant.id, &peer, || Ok(())).unwrap();
        assert!(
            !std::fs::read_to_string(root.path().join("owners.json"))
                .unwrap()
                .contains(&secret)
        );
        other.revoke(&grant.id).unwrap();
        assert!(store.authorize(&grant.id, &peer, || Ok(())).is_err());
        assert!(store.redeem(&grant.id, &secret, &peer).is_err());
        assert!(store.list().unwrap().is_empty());
    }
    #[test]
    fn expired_tokens_do_not_enroll_but_bound_identities_survive_expiry() {
        let root = tempfile::tempdir().unwrap();
        omochi_local::files::private_directory(root.path()).unwrap();
        let store = OwnerStore::new(root.path().to_owned());
        let (pending, token) = store.create().unwrap();
        let (bound, proof) = store.create().unwrap();
        let peer = "a".repeat(64);
        store.redeem(&bound.id, &proof, &peer).unwrap();
        store
            .transaction(|s| {
                for r in &mut s.records {
                    r.grant.invitation_expires_at = 1;
                }
                Ok(())
            })
            .unwrap();
        assert!(store.redeem(&pending.id, &token, &peer).is_err());
        store.authorize(&bound.id, &peer, || Ok(())).unwrap();
    }
}
