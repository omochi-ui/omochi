//! Hub-owned grants. Layout references are never credentials and gateway trust is separate.
use crate::dashboards::{ApiError, Dashboard, DashboardStore, Input};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::PathBuf};

type Result<T> = std::result::Result<T, ApiError>;
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Permissions {
    pub interact: bool,
    pub edit_layout: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Grant {
    pub id: String,
    pub board_id: String,
    pub permissions: Permissions,
}
// Disk encoding is distinct: public responses never include credential digests.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    grant: Grant,
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
// Strictly decode the old envelope before retiring its credentials.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyStorage {
    version: u32,
    #[serde(default)]
    generation: u64,
    records: Vec<LegacyRecord>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRecord {
    grant: LegacyGrant,
    digest: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyGrant {
    id: String,
    board_id: String,
    permissions: Permissions,
    public: bool,
    peer: Option<String>,
    invitation_expires_at: u64,
}
#[derive(Clone)]
pub struct ShareStore {
    root: PathBuf,
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
impl ShareStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }
    fn transaction<T>(&self, operation: impl FnOnce(&mut Storage) -> Result<T>) -> Result<T> {
        let _lock = omochi_transport::persistence::lock(&self.root.join("shares.lock"))?;
        let bytes = (|| -> std::io::Result<Vec<u8>> {
            let file = File::open(self.root.join("shares.json"))?;
            let mut bytes = Vec::new();
            file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
            Ok(bytes)
        })();
        let mut storage: Storage = match bytes {
            Ok(bytes) if bytes.len() <= 1024 * 1024 => {
                let mut value: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|_| ApiError::new(503, "Invalid sharing storage"))?;
                if value["version"] == 1 {
                    // v1 credentials are withdrawn, never converted into bearer authority.
                    let old: LegacyStorage = serde_json::from_value(value)
                        .map_err(|_| ApiError::new(503, "Invalid sharing storage"))?;
                    let mut ids = std::collections::BTreeSet::new();
                    if old.version != 1
                        || old.records.len() > 1000
                        || old.records.iter().any(|r| {
                            uuid::Uuid::parse_str(&r.grant.id).is_err()
                                || !ids.insert(&r.grant.id)
                                || r.grant.board_id.trim().is_empty()
                                || r.grant.board_id.encode_utf16().count() > 100
                                || r.digest.len() != 64
                                || !r
                                    .digest
                                    .bytes()
                                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                                || r.grant
                                    .peer
                                    .as_ref()
                                    .is_some_and(|p| !omochi_protocol::session::valid_node_id(p))
                                || (r.grant.public
                                    && (r.grant.peer.is_some()
                                        || r.grant.invitation_expires_at != 0
                                        || r.grant.permissions != Permissions::default()))
                        })
                    {
                        return Err(ApiError::new(503, "Invalid sharing storage"));
                    }
                    let generation = old
                        .generation
                        .checked_add(1)
                        .ok_or_else(|| ApiError::new(503, "Sharing generation exhausted"))?;
                    value = serde_json::json!({"version":2,"generation":generation,"records":[]});
                    omochi_transport::persistence::atomic_write(
                        &self.root.join("shares.json"),
                        &serde_json::to_vec(&value).unwrap(),
                    )?;
                }
                serde_json::from_value(value)
                    .map_err(|_| ApiError::new(503, "Invalid sharing storage"))?
            }
            Ok(_) => return Err(ApiError::new(503, "Sharing storage exceeds capacity")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Storage {
                version: 2,
                generation: 0,
                records: vec![],
            },
            Err(error) => return Err(error.into()),
        };
        if storage.version != 2 || storage.records.len() > 1000 {
            return Err(ApiError::new(503, "Invalid sharing storage"));
        }
        let mut ids = std::collections::BTreeSet::new();
        for record in &storage.records {
            if uuid::Uuid::parse_str(&record.grant.id).is_err()
                || !ids.insert(&record.grant.id)
                || record.grant.board_id.trim().is_empty()
                || record.grant.board_id.encode_utf16().count() > 100
                || record.digest.len() != 64
                || !record
                    .digest
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                return Err(ApiError::new(503, "Invalid sharing storage"));
            }
        }
        let before = serde_json::to_vec(&storage)
            .map_err(|_| ApiError::new(503, "Invalid sharing storage"))?;
        let previous: Vec<_> = storage
            .records
            .iter()
            .map(|record| record.grant.clone())
            .collect();
        let result = operation(&mut storage)?;
        let mut bytes = serde_json::to_vec(&storage)
            .map_err(|_| ApiError::new(503, "Invalid sharing storage"))?;
        if bytes != before {
            // Adding a link never withdraws old
            // authority. Retire sessions only when existing authority is narrowed.
            let withdrawal = previous.iter().any(|old| {
                storage
                    .records
                    .iter()
                    .find(|r| r.grant.id == old.id)
                    .is_none_or(|record| {
                        let new = &record.grant;
                        old.board_id != new.board_id
                            || (old.permissions.interact && !new.permissions.interact)
                            || (old.permissions.edit_layout && !new.permissions.edit_layout)
                    })
            });
            if withdrawal {
                storage.generation = storage
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| ApiError::new(503, "Sharing generation exhausted"))?;
            }
            bytes = serde_json::to_vec(&storage)
                .map_err(|_| ApiError::new(503, "Invalid sharing storage"))?;
            if bytes.len() > 1024 * 1024 {
                return Err(ApiError::new(503, "Sharing storage exceeds capacity"));
            }
            omochi_transport::persistence::atomic_write(&self.root.join("shares.json"), &bytes)?;
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
    pub fn list(&self) -> Result<Vec<Grant>> {
        self.transaction(|s| Ok(s.records.iter().map(|r| r.grant.clone()).collect()))
    }
    pub fn create(
        &self,
        boards: &DashboardStore,
        board_id: &str,
        permissions: Permissions,
    ) -> Result<(Grant, String)> {
        if !boards.list()?.iter().any(|b| b.id == board_id) {
            return Err(ApiError::new(404, "Dashboard not found"));
        }
        let secret = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let grant = Grant {
            id: uuid::Uuid::new_v4().to_string(),
            board_id: board_id.into(),
            permissions,
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
    pub fn authorize(
        &self,
        boards: &DashboardStore,
        id: &str,
        _peer: &str,
        secret: Option<&str>,
    ) -> Result<(Grant, Dashboard)> {
        self.with_authorized(id, secret, |grant| {
            let board = boards
                .list()?
                .into_iter()
                .find(|b| b.id == grant.board_id)
                .ok_or_else(denied)?;
            Ok((grant.clone(), board))
        })
    }
    /// Authorization and the dispatched write share the grant lock. Revocation
    /// waits for this write, and stale in-memory grants cannot re-enable access.
    pub fn edit_layout(
        &self,
        boards: &DashboardStore,
        id: &str,
        secret: &str,
        value: serde_json::Value,
    ) -> Result<Dashboard> {
        self.with_authorized(id, Some(secret), |grant| {
            boards
                .mutate_shared_layout(grant, value)?
                .ok_or_else(denied)
        })
    }
    fn with_authorized<T>(
        &self,
        id: &str,
        secret: Option<&str>,
        operation: impl FnOnce(&Grant) -> Result<T>,
    ) -> Result<T> {
        self.transaction(|s| {
            let r = s
                .records
                .iter()
                .find(|r| r.grant.id == id)
                .ok_or_else(denied)?;
            let valid = secret.is_some_and(|s| hash(s) == r.digest);
            if !valid {
                return Err(denied());
            }
            operation(&r.grant)
        })
    }
}
impl Grant {
    pub fn allows_surface(&self, board: &Dashboard, id: &str) -> bool {
        board.input.panels.iter().any(|p| {
            format!(
                "{}~{}/{}",
                p.reference.node_id, p.reference.service_id, p.reference.surface_id
            ) == id
        })
    }
    pub fn validate_layout(&self, current: &Dashboard, input: &Input) -> Result<()> {
        if !self.permissions.edit_layout || input.panels.len() != current.input.panels.len() {
            return Err(denied());
        }
        for p in &input.panels {
            let Some(old) = current.input.panels.iter().find(|old| old.id == p.id) else {
                return Err(denied());
            };
            if old.reference.node_id != p.reference.node_id
                || old.reference.service_id != p.reference.service_id
                || old.reference.surface_id != p.reference.surface_id
            {
                return Err(denied());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    fn board(store: &DashboardStore) -> Dashboard {
        store.mutate(None,json!({"name":"Shared","panels":[{"id":"p","title":"View","reference":{"nodeId":"a".repeat(64),"serviceId":"app","surfaceId":"main"},"x":0,"y":0,"w":4,"h":3}]}),false).unwrap().unwrap()
    }

    #[test]
    fn bearer_permissions_persistence_and_revocation() {
        let root = tempfile::tempdir().unwrap();
        let boards = DashboardStore::open(root.path().join("state")).unwrap();
        for interact in [false, true] {
            for edit_layout in [false, true] {
                let board = boards
                    .mutate(None, json!({"name":"Shared","panels":[]}), false)
                    .unwrap()
                    .unwrap();
                let shares = boards.shares();
                let permissions = Permissions {
                    interact,
                    edit_layout,
                };
                let (grant, secret) = shares.create(&boards, &board.id, permissions).unwrap();
                assert!(shares.authorize(&boards, &grant.id, "a", None).is_err());
                assert!(
                    shares
                        .authorize(&boards, &grant.id, "a", Some("wrong"))
                        .is_err()
                );
                for peer in ["a", "b"] {
                    assert_eq!(
                        shares
                            .authorize(&boards, &grant.id, peer, Some(&secret))
                            .unwrap()
                            .0
                            .permissions,
                        permissions
                    );
                }
                let mut value = json!({"revision":1,"name":"Edited","panels":[]});
                let result = shares.edit_layout(&boards, &grant.id, &secret, value.clone());
                assert_eq!(result.is_ok(), edit_layout);
                if let Ok(saved) = result {
                    value["revision"] = json!(saved.revision);
                    assert_eq!(
                        shares
                            .edit_layout(
                                &boards,
                                &grant.id,
                                &secret,
                                json!({"revision":1,"name":"Stale","panels":[]})
                            )
                            .unwrap_err()
                            .0,
                        409
                    );
                }
                assert!(
                    !std::fs::read_to_string(root.path().join("state/shares.json"))
                        .unwrap()
                        .contains(&secret)
                );
                assert!(!serde_json::to_string(&grant).unwrap().contains("digest"));
                let other = boards.shares();
                shares.revoke(&grant.id).unwrap();
                assert!(
                    other
                        .authorize(&boards, &grant.id, "a", Some(&secret))
                        .is_err()
                );
                assert!(
                    other
                        .edit_layout(&boards, &grant.id, &secret, value)
                        .is_err()
                );
            }
        }
    }
    #[test]
    fn old_grants_are_retired_and_unknown_storage_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let boards = DashboardStore::open(root.path().join("state")).unwrap();
        let path = root.path().join("state/shares.json");
        let b = board(&boards);
        let (grant, secret) = boards
            .shares()
            .create(&boards, &b.id, Permissions::default())
            .unwrap();
        let mut legacy: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        legacy["version"] = json!(1);
        legacy["generation"] = json!(4);
        legacy["records"][0]["grant"]["public"] = json!(true);
        legacy["records"][0]["grant"]["peer"] = serde_json::Value::Null;
        legacy["records"][0]["grant"]["invitationExpiresAt"] = json!(0);
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert!(boards.shares().list().unwrap().is_empty());
        assert_eq!(boards.shares().generation().unwrap(), 5);
        assert!(
            boards
                .shares()
                .authorize(&boards, &grant.id, "any", Some(&secret))
                .is_err()
        );
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["version"], 2);
        std::fs::write(&path, br#"{"version":99,"generation":5,"records":[]}"#).unwrap();
        assert!(boards.shares().list().is_err());
    }
    #[test]
    fn grants_follow_current_membership_and_layout_editing_cannot_widen_authority() {
        let dir = tempfile::tempdir().unwrap();
        let boards = DashboardStore::open(dir.path().join("state")).unwrap();
        let b = board(&boards);
        let shares = boards.shares();
        let peer = "b".repeat(64);
        let (g, s) = shares
            .create(
                &boards,
                &b.id,
                Permissions {
                    interact: false,
                    edit_layout: true,
                },
            )
            .unwrap();
        let route = format!("{}~app/main", "a".repeat(64));
        assert!(g.allows_surface(&b, &route));
        let mut edited = b.input.clone();
        edited.name = "Edited".into();
        edited.panels[0].x = 4;
        g.validate_layout(&b, &edited).unwrap();
        edited.panels[0].reference.surface_id = "secret".into();
        assert!(g.validate_layout(&b, &edited).is_err());
        edited.panels.clear();
        assert!(g.validate_layout(&b, &edited).is_err());
        boards
            .mutate(
                Some(&b.id),
                json!({"revision":b.revision,"name":"Changed","panels":[]}),
                false,
            )
            .unwrap();
        let (_, updated) = shares.authorize(&boards, &g.id, &peer, Some(&s)).unwrap();
        assert!(!g.allows_surface(&updated, &route));
        boards
            .mutate(Some(&b.id), json!({"revision":updated.revision}), true)
            .unwrap();
        assert!(shares.authorize(&boards, &g.id, &peer, Some(&s)).is_err());
    }
    #[test]
    fn shared_writes_enforce_current_grants_membership_and_revision_under_locks() {
        let dir = tempfile::tempdir().unwrap();
        let boards = DashboardStore::open(dir.path().join("state")).unwrap();
        let b = board(&boards);
        let shares = boards.shares();
        let (g, secret) = shares
            .create(
                &boards,
                &b.id,
                Permissions {
                    interact: false,
                    edit_layout: true,
                },
            )
            .unwrap();
        let mut value = serde_json::to_value(&b.input).unwrap();
        value["revision"] = json!(b.revision);
        value["name"] = json!("Edited");
        let saved = shares
            .edit_layout(&boards, &g.id, &secret, value.clone())
            .unwrap();
        assert_eq!(saved.revision, 2);
        assert_eq!(
            shares
                .edit_layout(&boards, &g.id, &secret, value.clone())
                .unwrap_err()
                .0,
            409
        );
        value["revision"] = json!(2);
        value["panels"][0]["reference"]["surfaceId"] = json!("private");
        assert_eq!(
            shares
                .edit_layout(&boards, &g.id, &secret, value.clone())
                .unwrap_err()
                .0,
            403
        );
        assert_eq!(boards.list().unwrap()[0].revision, 2);
        value["panels"][0]["reference"]["surfaceId"] = json!("main");
        shares.revoke(&g.id).unwrap();
        assert_eq!(
            shares
                .edit_layout(&boards, &g.id, &secret, value)
                .unwrap_err()
                .0,
            403
        );
    }
    #[test]
    fn persisted_storage_admits_the_exact_byte_limit_and_rejects_one_extra_byte() {
        let root = tempfile::tempdir().unwrap();
        let boards = DashboardStore::open(root.path().join("state")).unwrap();
        let path = root.path().join("state/shares.json");
        let mut bytes = br#"{"version":2,"generation":0,"records":[]}"#.to_vec();
        bytes.resize(1024 * 1024, b' ');
        fs::write(&path, &bytes).unwrap();
        assert!(boards.shares().list().unwrap().is_empty());
        bytes.push(b' ');
        fs::write(&path, &bytes).unwrap();
        assert_eq!(boards.shares().list().unwrap_err().0, 503);
        assert_eq!(fs::metadata(&path).unwrap().len(), 1024 * 1024 + 1);
    }
}
