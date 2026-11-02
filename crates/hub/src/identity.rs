//! Native identity/trust state. Hosts implement storage, never authorization policy.
use crate::host::HostFuture;
use base64::{Engine, engine::general_purpose::STANDARD};
use omochi_transport::{EndpointAddr, EndpointId, SecretKey};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};

const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityError {
    Unavailable,
    Invalid,
    Changed,
    SaveFailed,
    Conflict,
    OutcomeUnknown,
    ReloadRequired,
}
impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "secure identity storage is unavailable",
            Self::Invalid => "invalid existing identity; refusing to generate a replacement",
            Self::Changed => "stored identity changed; stop and reopen the Hub",
            Self::SaveFailed => "identity change was not saved",
            Self::Conflict => "identity storage changed concurrently; reload required",
            Self::OutcomeUnknown => "identity save outcome is unknown; reload required",
            Self::ReloadRequired => "authorization is suspended until identity storage is reloaded",
        })
    }
}
impl std::error::Error for IdentityError {}

/// `Failed` guarantees that no write took effect. Use `Unknown` for exceptions
/// after write dispatch, interrupted callbacks, or an indeterminate OS result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitOutcome {
    Committed,
    Conflict,
    Failed,
    Unknown,
}

/// A small, trusted host interface for the *one* native identity document.
/// Missing (`None`) differs from unreadable/malformed state. Compare-and-exchange
/// must reread and compare under the host's serial lock before writing, and report
/// completion only after the durable secure-storage operation completes.
/// Neither method may reenter a Hub mutation or synchronously block the UI thread.
pub trait IdentityStore: Send + Sync + 'static {
    fn read(&self) -> HostFuture<'_, Result<Option<String>, IdentityError>>;
    fn compare_exchange(
        &self,
        expected: Option<String>,
        replacement: String,
    ) -> HostFuture<'_, CommitOutcome>;
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SavedGateway {
    pub address: EndpointAddr,
    pub name: String,
}

// Intentionally not Debug. This is exactly omochi.native.v1.identity, including
// the existing base64 key encoding and {address, name} peer records.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    secret: String,
    peers: Vec<SavedGateway>,
}

impl Document {
    fn parse(raw: &str) -> Result<Self, IdentityError> {
        if raw.len() > MAX_DOCUMENT_BYTES {
            return Err(IdentityError::Invalid);
        }
        // Discard serde errors: they can embed the original secret/string value.
        let document: Self = serde_json::from_str(raw).map_err(|_| IdentityError::Invalid)?;
        document.validate()?;
        Ok(document)
    }

    fn key(&self) -> Result<SecretKey, IdentityError> {
        let bytes = STANDARD
            .decode(&self.secret)
            .map_err(|_| IdentityError::Invalid)?;
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| IdentityError::Invalid)?;
        if STANDARD.encode(bytes) != self.secret {
            return Err(IdentityError::Invalid);
        }
        Ok(SecretKey::from_bytes(&bytes))
    }

    fn validate(&self) -> Result<(), IdentityError> {
        let identity = self.key()?.public();
        let mut ids = BTreeSet::new();
        for peer in &self.peers {
            if peer.address.id == identity
                || !ids.insert(peer.address.id)
                || peer.name.trim().is_empty()
                || peer.name.len() > 256
                || peer.address.addrs.len() > 64
                || peer
                    .address
                    .addrs
                    .iter()
                    .any(|addr| !addr.is_ip() && !addr.is_relay())
            {
                return Err(IdentityError::Invalid);
            }
            for relay in peer.address.relay_urls() {
                let url =
                    url::Url::parse(&relay.to_string()).map_err(|_| IdentityError::Invalid)?;
                if !matches!(url.scheme(), "http" | "https")
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                {
                    return Err(IdentityError::Invalid);
                }
            }
        }
        Ok(())
    }
}

/// Owned by the native Hub's serialized command loop. There is no arbitrary
/// document setter. A cancelled commit leaves this state unusable until reload.
pub(crate) struct NativeIdentity {
    store: Arc<dyn IdentityStore>,
    raw: String,
    document: Document,
    uncertain: bool,
}

impl NativeIdentity {
    pub(crate) async fn open(store: Arc<dyn IdentityStore>) -> Result<Self, IdentityError> {
        let existing = store.read().await?;
        let raw = match existing {
            Some(raw) => raw,
            None => {
                let document = Document {
                    secret: STANDARD.encode(SecretKey::generate().to_bytes()),
                    peers: Vec::new(),
                };
                let raw = serde_json::to_string(&document).map_err(|_| IdentityError::Invalid)?;
                match store.compare_exchange(None, raw.clone()).await {
                    CommitOutcome::Committed => raw,
                    CommitOutcome::Conflict => return Err(IdentityError::Conflict),
                    CommitOutcome::Failed => return Err(IdentityError::SaveFailed),
                    CommitOutcome::Unknown => return Err(IdentityError::OutcomeUnknown),
                }
            }
        };
        let document = Document::parse(&raw)?;
        Ok(Self {
            store,
            raw,
            document,
            uncertain: false,
        })
    }

    pub(crate) fn key(&self) -> Result<SecretKey, IdentityError> {
        self.ready()?;
        self.document.key()
    }

    pub(crate) fn peers(&self) -> Result<&[SavedGateway], IdentityError> {
        self.ready()?;
        Ok(&self.document.peers)
    }

    fn ready(&self) -> Result<(), IdentityError> {
        if self.uncertain {
            Err(IdentityError::ReloadRequired)
        } else {
            Ok(())
        }
    }

    pub(crate) async fn reload(&mut self) -> Result<(), IdentityError> {
        self.uncertain = true;
        let raw = self.store.read().await?.ok_or(IdentityError::Changed)?;
        let document = Document::parse(&raw)?;
        if document.key()?.public() != self.document.key()?.public() {
            return Err(IdentityError::Changed);
        }
        self.document = document;
        self.raw = raw;
        self.uncertain = false;
        Ok(())
    }

    async fn commit(&mut self, next: Document) -> Result<(), IdentityError> {
        self.ready()?;
        next.validate()?;
        let raw = serde_json::to_string(&next).map_err(|_| IdentityError::Invalid)?;
        if raw.len() > MAX_DOCUMENT_BYTES {
            return Err(IdentityError::Invalid);
        }
        self.uncertain = true;
        match self
            .store
            .compare_exchange(Some(self.raw.clone()), raw.clone())
            .await
        {
            CommitOutcome::Committed => {
                self.document = next;
                self.raw = raw;
                self.uncertain = false;
                Ok(())
            }
            CommitOutcome::Failed => {
                self.uncertain = false;
                Err(IdentityError::SaveFailed)
            }
            CommitOutcome::Conflict => Err(IdentityError::Conflict),
            CommitOutcome::Unknown => Err(IdentityError::OutcomeUnknown),
        }
    }

    pub(crate) async fn remember(&mut self, peer: SavedGateway) -> Result<(), IdentityError> {
        let mut next = self.document.clone();
        next.peers.retain(|p| p.address.id != peer.address.id);
        next.peers.push(peer);
        next.peers.sort_by_key(|p| p.address.id);
        self.commit(next).await
    }

    pub(crate) async fn forget(&mut self, id: EndpointId) -> Result<(), IdentityError> {
        let mut next = self.document.clone();
        next.peers.retain(|p| p.address.id != id);
        self.commit(next).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemoryStore {
        value: Mutex<Option<String>>,
        outcome: Mutex<Option<CommitOutcome>>,
    }
    impl IdentityStore for MemoryStore {
        fn read(&self) -> HostFuture<'_, Result<Option<String>, IdentityError>> {
            Box::pin(async { Ok(self.value.lock().unwrap().clone()) })
        }
        fn compare_exchange(
            &self,
            expected: Option<String>,
            replacement: String,
        ) -> HostFuture<'_, CommitOutcome> {
            Box::pin(async move {
                let outcome = self
                    .outcome
                    .lock()
                    .unwrap()
                    .take()
                    .unwrap_or(CommitOutcome::Committed);
                if outcome != CommitOutcome::Committed {
                    return outcome;
                }
                let mut value = self.value.lock().unwrap();
                if *value != expected {
                    return CommitOutcome::Conflict;
                }
                *value = Some(replacement);
                CommitOutcome::Committed
            })
        }
    }
    fn peer() -> SavedGateway {
        SavedGateway {
            address: SecretKey::generate().public().into(),
            name: "gateway".into(),
        }
    }

    #[tokio::test]
    async fn existing_native_document_roundtrips_identity_addresses_and_peers() {
        let store = Arc::new(MemoryStore::default());
        let secret = STANDARD.encode([42u8; 32]);
        let saved_peer = peer();
        // Exact Dart shape, without adding version/revision/role fields.
        *store.value.lock().unwrap() = Some(
            serde_json::json!({
                "secret": secret,
                "peers": [{"address": saved_peer.address, "name": saved_peer.name}]
            })
            .to_string(),
        );
        let mut identity = NativeIdentity::open(store.clone()).await.unwrap();
        assert!(identity.peers().unwrap() == [saved_peer]);
        let key = identity.key().unwrap().public();
        identity.remember(peer()).await.unwrap();
        let reopened = NativeIdentity::open(store.clone()).await.unwrap();
        assert_eq!(key, reopened.key().unwrap().public());
        assert_eq!(reopened.peers().unwrap().len(), 2);
        let saved: serde_json::Value =
            serde_json::from_str(store.value.lock().unwrap().as_ref().unwrap()).unwrap();
        assert_eq!(saved["secret"], secret);
        assert_eq!(saved.as_object().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn malformed_or_missing_existing_key_is_never_regenerated() {
        for raw in [
            "{}",
            "null",
            "not json",
            r#"{"secret":null,"peers":[]}"#,
            r#"{"secret":"bad","peers":[]}"#,
        ] {
            let store = Arc::new(MemoryStore::default());
            *store.value.lock().unwrap() = Some(raw.into());
            assert!(matches!(
                NativeIdentity::open(store.clone()).await,
                Err(IdentityError::Invalid)
            ));
            assert_eq!(store.value.lock().unwrap().as_deref(), Some(raw));
        }
    }

    #[tokio::test]
    async fn failed_or_unknown_save_never_activates_new_trust() {
        let store = Arc::new(MemoryStore::default());
        let mut identity = NativeIdentity::open(store.clone()).await.unwrap();
        *store.outcome.lock().unwrap() = Some(CommitOutcome::Failed);
        assert_eq!(
            identity.remember(peer()).await,
            Err(IdentityError::SaveFailed)
        );
        assert!(identity.peers().unwrap().is_empty());
        *store.outcome.lock().unwrap() = Some(CommitOutcome::Unknown);
        assert_eq!(
            identity.remember(peer()).await,
            Err(IdentityError::OutcomeUnknown)
        );
        assert!(matches!(
            identity.peers(),
            Err(IdentityError::ReloadRequired)
        ));
        assert_eq!(
            identity.remember(peer()).await,
            Err(IdentityError::ReloadRequired)
        );
        identity.reload().await.unwrap();
        assert!(identity.peers().unwrap().is_empty());
    }

    #[tokio::test]
    async fn stale_writer_cannot_restore_deleted_registration() {
        let store = Arc::new(MemoryStore::default());
        let mut first = NativeIdentity::open(store.clone()).await.unwrap();
        let registered = peer();
        first.remember(registered.clone()).await.unwrap();
        let mut stale = NativeIdentity::open(store.clone()).await.unwrap();
        first.forget(registered.address.id).await.unwrap();
        assert_eq!(stale.remember(peer()).await, Err(IdentityError::Conflict));
        stale.reload().await.unwrap();
        assert!(stale.peers().unwrap().is_empty());
    }

    #[tokio::test]
    async fn first_run_requires_a_confirmed_save_and_reload_rejects_rotated_key() {
        let store = Arc::new(MemoryStore::default());
        *store.outcome.lock().unwrap() = Some(CommitOutcome::Unknown);
        assert!(matches!(
            NativeIdentity::open(store.clone()).await,
            Err(IdentityError::OutcomeUnknown)
        ));
        let mut identity = NativeIdentity::open(store.clone()).await.unwrap();
        *store.value.lock().unwrap() = Some(
            serde_json::json!({"secret": STANDARD.encode([1u8; 32]), "peers": []}).to_string(),
        );
        assert_eq!(identity.reload().await, Err(IdentityError::Changed));
        assert!(matches!(identity.key(), Err(IdentityError::ReloadRequired)));
    }
}
