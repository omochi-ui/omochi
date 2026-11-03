//! Hub-owned grants. Layout references are never credentials and gateway trust is separate.
use crate::dashboards::{ApiError, Dashboard, DashboardStore, Input};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

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
    pub public: bool,
    pub peer: Option<String>,
    pub invitation_expires_at: u64,
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
#[derive(Clone)]
pub struct ShareStore {
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
            Ok(bytes) if bytes.len() <= 1024 * 1024 => serde_json::from_slice(&bytes)
                .map_err(|_| ApiError::new(503, "Invalid sharing storage"))?,
            Ok(_) => return Err(ApiError::new(503, "Sharing storage exceeds capacity")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Storage {
                version: 1,
                generation: 0,
                records: vec![],
            },
            Err(error) => return Err(error.into()),
        };
        if storage.version != 1 || storage.records.len() > 1000 {
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
                || record
                    .grant
                    .peer
                    .as_ref()
                    .is_some_and(|peer| !omochi_protocol::session::valid_node_id(peer))
                || (record.grant.public && record.grant.invitation_expires_at != 0)
            {
                return Err(ApiError::new(503, "Invalid sharing storage"));
            }
            if record.grant.public
                && (record.grant.permissions != Permissions::default()
                    || record.grant.peer.is_some())
            {
                return Err(ApiError::new(503, "Invalid public grant"));
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
            // Adding a grant or binding a pending invitation never withdraws old
            // authority. Retire sessions only when existing authority is narrowed.
            let withdrawal = previous.iter().any(|old| {
                storage
                    .records
                    .iter()
                    .find(|r| r.grant.id == old.id)
                    .is_none_or(|record| {
                        let new = &record.grant;
                        old.board_id != new.board_id
                            || old.public != new.public
                            || (old.peer.is_some() && old.peer != new.peer)
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
        public: bool,
        permissions: Permissions,
    ) -> Result<(Grant, String)> {
        if !boards.list()?.iter().any(|b| b.id == board_id) {
            return Err(ApiError::new(404, "Dashboard not found"));
        }
        if public && permissions != Permissions::default() {
            return Err(ApiError::new(400, "Public links are readonly"));
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
            public,
            peer: None,
            invitation_expires_at: if public { 0 } else { now() + 180 },
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
    pub fn redeem(&self, id: &str, secret: &str, peer: &str) -> Result<Grant> {
        if !omochi_protocol::session::valid_node_id(peer) {
            return Err(denied());
        }
        self.transaction(|s| {
            let r = s
                .records
                .iter_mut()
                .find(|r| r.grant.id == id)
                .ok_or_else(denied)?;
            if r.grant.public
                || r.digest != hash(secret)
                || now() >= r.grant.invitation_expires_at
                || r.grant.peer.as_deref().is_some_and(|p| p != peer)
            {
                return Err(denied());
            }
            r.grant.peer = Some(peer.into());
            Ok(r.grant.clone())
        })
    }
    pub fn authorize(
        &self,
        boards: &DashboardStore,
        id: &str,
        peer: &str,
        secret: Option<&str>,
    ) -> Result<(Grant, Dashboard)> {
        self.with_authorized(id, peer, secret, |grant| {
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
        peer: &str,
        value: serde_json::Value,
    ) -> Result<Dashboard> {
        self.with_authorized(id, peer, None, |grant| {
            boards
                .mutate_shared_layout(grant, value)?
                .ok_or_else(denied)
        })
    }
    fn with_authorized<T>(
        &self,
        id: &str,
        peer: &str,
        secret: Option<&str>,
        operation: impl FnOnce(&Grant) -> Result<T>,
    ) -> Result<T> {
        self.transaction(|s| {
            let r = s
                .records
                .iter()
                .find(|r| r.grant.id == id)
                .ok_or_else(denied)?;
            let valid = if r.grant.public {
                secret.is_some_and(|s| hash(s) == r.digest)
            } else {
                r.grant.peer.as_deref() == Some(peer)
            };
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
    fn public_grants_are_readonly_persistent_and_never_export_the_secret() {
        let dir = tempfile::tempdir().unwrap();
        let boards = DashboardStore::open(dir.path().join("state")).unwrap();
        let b = board(&boards);
        let shares = boards.shares();
        assert!(
            shares
                .create(
                    &boards,
                    &b.id,
                    true,
                    Permissions {
                        interact: true,
                        edit_layout: false
                    }
                )
                .is_err()
        );
        let (g, secret) = shares
            .create(&boards, &b.id, true, Permissions::default())
            .unwrap();
        let disk = fs::read_to_string(dir.path().join("state/shares.json")).unwrap();
        assert!(!disk.contains(&secret));
        assert!(!serde_json::to_string(&g).unwrap().contains("digest"));
        assert!(shares.authorize(&boards, &g.id, "any", None).is_err());
        assert!(
            shares
                .authorize(&boards, &g.id, "any", Some("wrong"))
                .is_err()
        );
        for peer in ["a".repeat(64), "b".repeat(64)] {
            assert!(
                boards
                    .shares()
                    .authorize(&boards, &g.id, &peer, Some(&secret))
                    .is_ok()
            );
        }
        shares.revoke(&g.id).unwrap();
        assert!(
            shares
                .authorize(&boards, &g.id, "any", Some(&secret))
                .is_err()
        );
    }
    #[test]
    fn first_identity_wins_and_revocation_is_never_restored_by_another_store() {
        let dir = tempfile::tempdir().unwrap();
        let boards = DashboardStore::open(dir.path().join("state")).unwrap();
        let b = board(&boards);
        let a = boards.shares();
        let other = boards.shares();
        let (g, secret) = a
            .create(
                &boards,
                &b.id,
                false,
                Permissions {
                    interact: true,
                    edit_layout: true,
                },
            )
            .unwrap();
        let first = "a".repeat(64);
        let second = "b".repeat(64);
        assert!(a.authorize(&boards, &g.id, &first, Some(&secret)).is_err());
        a.redeem(&g.id, &secret, &first).unwrap();
        other.redeem(&g.id, &secret, &first).unwrap();
        assert!(other.redeem(&g.id, &secret, &second).is_err());
        assert!(a.authorize(&boards, &g.id, &second, None).is_err());
        assert!(other.authorize(&boards, &g.id, &first, None).is_ok());
        a.revoke(&g.id).unwrap();
        assert!(other.authorize(&boards, &g.id, &first, None).is_err());
        assert!(other.list().unwrap().is_empty());
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
                false,
                Permissions {
                    interact: false,
                    edit_layout: true,
                },
            )
            .unwrap();
        shares.redeem(&g.id, &s, &peer).unwrap();
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
        let (_, updated) = shares.authorize(&boards, &g.id, &peer, None).unwrap();
        assert!(!g.allows_surface(&updated, &route));
        boards
            .mutate(Some(&b.id), json!({"revision":updated.revision}), true)
            .unwrap();
        assert!(shares.authorize(&boards, &g.id, &peer, None).is_err());
    }
    #[test]
    fn shared_writes_enforce_current_grants_membership_and_revision_under_locks() {
        let dir = tempfile::tempdir().unwrap();
        let boards = DashboardStore::open(dir.path().join("state")).unwrap();
        let b = board(&boards);
        let shares = boards.shares();
        let peer = "c".repeat(64);
        let (g, secret) = shares
            .create(
                &boards,
                &b.id,
                false,
                Permissions {
                    interact: false,
                    edit_layout: true,
                },
            )
            .unwrap();
        shares.redeem(&g.id, &secret, &peer).unwrap();
        let mut value = serde_json::to_value(&b.input).unwrap();
        value["revision"] = json!(b.revision);
        value["name"] = json!("Edited");
        let saved = shares
            .edit_layout(&boards, &g.id, &peer, value.clone())
            .unwrap();
        assert_eq!(saved.revision, 2);
        assert_eq!(
            shares
                .edit_layout(&boards, &g.id, &peer, value.clone())
                .unwrap_err()
                .0,
            409
        );
        value["revision"] = json!(2);
        value["panels"][0]["reference"]["surfaceId"] = json!("private");
        assert_eq!(
            shares
                .edit_layout(&boards, &g.id, &peer, value.clone())
                .unwrap_err()
                .0,
            403
        );
        assert_eq!(boards.list().unwrap()[0].revision, 2);
        value["panels"][0]["reference"]["surfaceId"] = json!("main");
        shares.revoke(&g.id).unwrap();
        assert_eq!(
            shares
                .edit_layout(&boards, &g.id, &peer, value)
                .unwrap_err()
                .0,
            403
        );
    }
    #[test]
    fn expired_invitations_do_not_expire_bound_grants_and_corruption_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let boards = DashboardStore::open(dir.path().join("state")).unwrap();
        let b = board(&boards);
        let shares = boards.shares();
        let peer = "d".repeat(64);
        let (g, secret) = shares
            .create(&boards, &b.id, false, Permissions::default())
            .unwrap();
        shares.redeem(&g.id, &secret, &peer).unwrap();
        let path = dir.path().join("state/shares.json");
        let mut disk: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        disk["records"][0]["grant"]["invitationExpiresAt"] = json!(1);
        fs::write(&path, serde_json::to_vec(&disk).unwrap()).unwrap();
        assert!(shares.redeem(&g.id, &secret, &peer).is_err());
        assert!(shares.authorize(&boards, &g.id, &peer, None).is_ok());
        disk["records"][0]["grant"]["peer"] = json!("invalid");
        let corrupt = serde_json::to_vec(&disk).unwrap();
        fs::write(&path, &corrupt).unwrap();
        assert!(shares.authorize(&boards, &g.id, &peer, None).is_err());
        assert!(
            shares
                .create(&boards, &b.id, true, Permissions::default())
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), corrupt);
    }
    #[test]
    fn persisted_storage_admits_the_exact_byte_limit_and_rejects_one_extra_byte() {
        let root = tempfile::tempdir().unwrap();
        let boards = DashboardStore::open(root.path().join("state")).unwrap();
        let path = root.path().join("state/shares.json");
        let mut bytes = br#"{"version":1,"generation":0,"records":[]}"#.to_vec();
        bytes.resize(1024 * 1024, b' ');
        fs::write(&path, &bytes).unwrap();
        assert!(boards.shares().list().unwrap().is_empty());
        bytes.push(b' ');
        fs::write(&path, &bytes).unwrap();
        assert_eq!(boards.shares().list().unwrap_err().0, 503);
        assert_eq!(fs::metadata(&path).unwrap().len(), 1024 * 1024 + 1);
    }
    #[test]
    fn simultaneous_redemption_has_exactly_one_winner() {
        let dir = tempfile::tempdir().unwrap();
        let boards = DashboardStore::open(dir.path().join("state")).unwrap();
        let b = board(&boards);
        let shares = boards.shares();
        let (g, s) = shares
            .create(&boards, &b.id, false, Permissions::default())
            .unwrap();
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let shares = shares.clone();
                let id = g.id.clone();
                let secret = s.clone();
                std::thread::spawn(move || {
                    shares.redeem(&id, &secret, &format!("{i:064x}")).is_ok()
                })
            })
            .collect();
        assert_eq!(
            threads
                .into_iter()
                .filter_map(|t| t.join().ok())
                .filter(|ok| *ok)
                .count(),
            1
        );
    }
}
