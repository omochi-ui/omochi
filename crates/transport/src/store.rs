//! Serialized, durable state for the new identity domain. Legacy trust is never read.
pub use crate::trust::{Peer, Role};
use anyhow::{Context, Result, bail};
use iroh::{EndpointId, SecretKey};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct Invitation {
    expires_at: u64,
    winner: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct State {
    version: u32,
    generation: u64,
    #[serde(default)]
    revocation_generation: u64,
    gateway_key: String,
    hub_key: String,
    peers: BTreeMap<String, Peer>,
    invitations: BTreeMap<String, Invitation>,
}
#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}

/// A definite registration rejection, before any state is written. Other errors
/// may be transient or leave the commit outcome unknown and can be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayRegistrationError {
    InvalidName,
    NameConflict,
    RoleConflict,
    AuthorizationChanged,
}
impl std::fmt::Display for GatewayRegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidName => "invalid gateway name",
            Self::NameConflict => "gateway name already registered",
            Self::RoleConflict => "peer role conflict",
            Self::AuthorizationChanged => "authorization changed",
        })
    }
}
impl std::error::Error for GatewayRegistrationError {}

impl State {
    fn check_gateway_registration(&self, id: &str, name: &str) -> Result<()> {
        if name.trim().is_empty() || name.len() > 256 {
            return Err(GatewayRegistrationError::InvalidName.into());
        }
        if self
            .peers
            .iter()
            .any(|(other, peer)| other != id && peer.name == name)
        {
            return Err(GatewayRegistrationError::NameConflict.into());
        }
        if self
            .peers
            .get(id)
            .is_some_and(|peer| peer.role != Role::Gateway)
        {
            return Err(GatewayRegistrationError::RoleConflict.into());
        }
        Ok(())
    }
}

fn key() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}
fn digest(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}
impl Store {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        omochi_local::files::private_directory(&root)?;
        let store = Self { root };
        store.mutate(|_| Ok(()))?;
        Ok(store)
    }
    pub(crate) fn directory(&self) -> &Path {
        &self.root
    }
    pub(crate) fn with_generation<T>(&self, f: impl FnOnce(u64) -> Result<T>) -> Result<T> {
        self.mutate(|s| f(s.generation))
    }
    fn mutate<T>(&self, f: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let _lock = crate::persistence::lock(&self.root.join("state.lock"))?;
        let path = self.root.join("state.json");
        let existed = path.exists();
        let mut state: State = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .context("invalid Rust state; refusing to overwrite")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State {
                version: 1,
                generation: 0,
                revocation_generation: 0,
                gateway_key: key(),
                hub_key: key(),
                peers: BTreeMap::new(),
                invitations: BTreeMap::new(),
            },
            Err(e) => return Err(e.into()),
        };
        if state.version != 1 {
            bail!("unsupported Rust state version")
        }
        let before = serde_json::to_vec(&state)?;
        let result = f(&mut state)?;
        if existed && before == serde_json::to_vec(&state)? {
            return Ok(result);
        }
        crate::persistence::atomic_write(&path, &serde_json::to_vec(&state)?)?;
        Ok(result)
    }
    pub fn identity(&self, role: Role) -> Result<SecretKey> {
        self.mutate(|state| {
            let encoded = match role {
                Role::Gateway => &state.gateway_key,
                Role::Hub => &state.hub_key,
            };
            let bytes: [u8; 32] = hex::decode(encoded)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid key length"))?;
            Ok(SecretKey::from_bytes(&bytes))
        })
    }
    pub fn peers(&self) -> Result<(u64, BTreeMap<String, Peer>)> {
        self.mutate(|s| Ok((s.generation, s.peers.clone())))
    }
    pub fn invite(&self, now: u64, ttl: u64) -> Result<String> {
        let secret = key();
        self.mutate(|s| {
            s.invitations.retain(|_, i| i.expires_at > now);
            if s.invitations.len() >= 128 {
                bail!("too many active invitations")
            }
            s.invitations.insert(
                digest(&secret),
                Invitation {
                    expires_at: now.checked_add(ttl).context("invalid expiry")?,
                    winner: None,
                },
            );
            Ok(())
        })?;
        Ok(secret)
    }
    /// `endpoint` must come from the authenticated connection, never a request field.
    pub fn redeem(&self, secret: &str, endpoint: EndpointId, name: &str, now: u64) -> Result<()> {
        if secret.len() != 64 || !secret.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("invalid invitation")
        }
        if name.trim().is_empty() || name.len() > 256 {
            bail!("invalid peer name")
        }
        let id = endpoint.to_string();
        self.mutate(|s| {
            let invite = s
                .invitations
                .get_mut(&digest(secret))
                .context("invalid invitation")?;
            if invite.expires_at <= now {
                bail!("invitation expired")
            }
            if let Some(winner) = &invite.winner {
                if winner == &id && s.peers.get(&id).is_some_and(|p| p.role == Role::Hub) {
                    return Ok(());
                }
                bail!("invitation already used")
            }
            if s.peers.get(&id).is_some_and(|peer| peer.role != Role::Hub) {
                bail!("peer role conflict")
            }
            invite.winner = Some(id.clone());
            s.peers.insert(
                id,
                Peer {
                    name: name.into(),
                    role: Role::Hub,
                },
            );
            s.generation += 1;
            Ok(())
        })
    }
    pub fn register_gateway(&self, endpoint: EndpointId, name: &str) -> Result<()> {
        self.register_gateway_guarded(endpoint, name, None)
    }
    /// Check before contacting the gateway and capture the revocation watermark.
    /// This does not reserve the name: registration rechecks under the write lock.
    pub fn prepare_gateway_registration(&self, endpoint: EndpointId, name: &str) -> Result<u64> {
        self.mutate(|s| {
            s.check_gateway_registration(&endpoint.to_string(), name)?;
            Ok(s.revocation_generation)
        })
    }
    pub fn register_gateway_guarded(
        &self,
        endpoint: EndpointId,
        name: &str,
        revocation: Option<u64>,
    ) -> Result<()> {
        self.mutate(|s| {
            if revocation.is_some_and(|generation| generation != s.revocation_generation) {
                return Err(GatewayRegistrationError::AuthorizationChanged.into());
            }
            let id = endpoint.to_string();
            s.check_gateway_registration(&id, name)?;
            s.peers.insert(
                id,
                Peer {
                    name: name.into(),
                    role: Role::Gateway,
                },
            );
            s.generation += 1;
            Ok(())
        })
    }

    /// Includes a durable revocation watermark, even if a removed peer was re-enrolled.
    pub fn authorization(&self) -> Result<(u64, u64, BTreeMap<String, Peer>)> {
        self.mutate(|s| Ok((s.generation, s.revocation_generation, s.peers.clone())))
    }

    /// Persist revocation only. Callers must separately await live-process acknowledgements.
    /// This deliberately is not a successful `trust forget` implementation.
    pub fn revoke_record(&self, endpoint: EndpointId) -> Result<u64> {
        self.mutate(|s| {
            let id = endpoint.to_string();
            s.peers.remove(&id);
            s.invitations.retain(|_, i| i.winner.as_ref() != Some(&id));
            s.generation += 1;
            s.revocation_generation = s.generation;
            Ok(s.generation)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn peer() -> EndpointId {
        SecretKey::from_bytes(&rand::random::<[u8; 32]>()).public()
    }
    #[test]
    fn gateway_registration_rechecks_preflight_and_types_definite_rejections() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("state")).unwrap();
        let gateway = peer();
        let other = peer();
        let revocation = store.prepare_gateway_registration(gateway, "work").unwrap();
        store.register_gateway(other, "work").unwrap();
        let error = store
            .prepare_gateway_registration(gateway, "work")
            .unwrap_err();
        assert_eq!(
            error.downcast_ref(),
            Some(&GatewayRegistrationError::NameConflict)
        );
        let error = store
            .register_gateway_guarded(gateway, "work", Some(revocation))
            .unwrap_err();
        assert_eq!(
            error.downcast_ref(),
            Some(&GatewayRegistrationError::NameConflict)
        );
        assert!(!store.peers().unwrap().1.contains_key(&gateway.to_string()));
        store.prepare_gateway_registration(other, "work").unwrap();
        let error = store
            .prepare_gateway_registration(gateway, " ")
            .unwrap_err();
        assert_eq!(
            error.downcast_ref(),
            Some(&GatewayRegistrationError::InvalidName)
        );
        let invitation = store.invite(0, 180).unwrap();
        store.redeem(&invitation, gateway, "hub", 1).unwrap();
        let error = store
            .prepare_gateway_registration(gateway, "personal")
            .unwrap_err();
        assert_eq!(
            error.downcast_ref(),
            Some(&GatewayRegistrationError::RoleConflict)
        );
        let error = store.register_gateway(gateway, "personal").unwrap_err();
        assert_eq!(
            error.downcast_ref(),
            Some(&GatewayRegistrationError::RoleConflict)
        );
        store.revoke_record(gateway).unwrap();
        let error = store
            .register_gateway_guarded(gateway, "personal", Some(revocation))
            .unwrap_err();
        assert_eq!(
            error.downcast_ref(),
            Some(&GatewayRegistrationError::AuthorizationChanged)
        );
        assert!(!store.peers().unwrap().1.contains_key(&gateway.to_string()));
    }
    #[test]
    fn redemption_is_durable_retryable_and_revocation_wins() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("state");
        let store = Store::open(&root).unwrap();
        let p = peer();
        let other = peer();
        let token = store.invite(100, 180).unwrap();
        store.redeem(&token, p, "hub", 101).unwrap();
        let reopened = Store::open(&root).unwrap();
        reopened.redeem(&token, p, "hub", 102).unwrap();
        assert!(reopened.redeem(&token, other, "other", 102).is_err());
        reopened.revoke_record(p).unwrap();
        assert!(store.redeem(&token, p, "hub", 103).is_err());
        assert!(store.peers().unwrap().1.is_empty());
    }
    #[test]
    fn simultaneous_redemption_has_one_winner() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("state")).unwrap();
        let token = store.invite(0, 180).unwrap();
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let store = store.clone();
                let token = token.clone();
                std::thread::spawn(move || store.redeem(&token, peer(), "hub", 1).is_ok())
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
        assert_eq!(store.peers().unwrap().1.len(), 1);
    }
    #[test]
    fn expiry_and_key_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("state");
        let store = Store::open(&root).unwrap();
        let gateway = store.identity(Role::Gateway).unwrap().public();
        assert_ne!(gateway, store.identity(Role::Hub).unwrap().public());
        assert_eq!(
            gateway,
            Store::open(&root)
                .unwrap()
                .identity(Role::Gateway)
                .unwrap()
                .public()
        );
        let token = store.invite(100, 3).unwrap();
        assert!(store.redeem(&token, peer(), "hub", 103).is_err());
    }
}
