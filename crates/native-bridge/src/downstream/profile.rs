//! Secure profile persistence. CAS outcomes are never guessed or recovered from stale state.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use omochi_transport::SecretKey;
use serde::{Deserialize, Serialize};

pub(super) const MAX_RECORDS: usize = 1000;

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
            profile.version == 1 && profile.records.len() <= MAX_RECORDS,
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
        anyhow::ensure!(self.records.len() <= MAX_RECORDS, Fault::CapacityExceeded);
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

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use std::{future::Future, task::Poll, time::Duration};

    fn full_profile() -> Profile {
        let hub_id = SecretKey::generate().public().to_string();
        Profile {
            version: 1,
            key: STANDARD.encode(SecretKey::generate().to_bytes()),
            records: (0..MAX_RECORDS)
                .map(|index| Record {
                    link: Link {
                        version: 1,
                        hub_id: hub_id.clone(),
                        relay_urls: vec!["https://relay.example/".into()],
                        grant_id: uuid::Uuid::from_u128(index as u128).to_string(),
                        mode: Mode::Owner,
                        secret: "0".repeat(64),
                    },
                    accepted: index != 0,
                })
                .collect(),
        }
    }

    fn registration(link: &Link) -> Command {
        let kind = if link.mode == Mode::Owner {
            "owner"
        } else {
            "share"
        };
        Command::Register {
            invitation: format!(
                "omochi://{kind}/{}",
                URL_SAFE_NO_PAD.encode(serde_json::to_vec(link).unwrap())
            ),
            save: true,
        }
    }

    // Drive real storage callbacks against an in-memory secure-store document.
    async fn drive<F: Future>(
        store: &CallbackStore,
        raw: &mut Option<String>,
        allow_commit: bool,
        operation: F,
    ) -> (F::Output, Vec<Value>) {
        let mut operation = std::pin::pin!(operation);
        let mut requests = Vec::new();
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            std::future::poll_fn(|cx| {
                if let Poll::Ready(result) = operation.as_mut().poll(cx) {
                    return Poll::Ready(result);
                }
                let request = store.requests.lock().unwrap().pop_front();
                if let Some(request) = request {
                    let request: Value = serde_json::from_str(&request).unwrap();
                    let reply = match request["op"].as_str().unwrap() {
                        "read" => json!({"ok":true,"value":raw}),
                        "commit" => {
                            assert_eq!(request["expected"], json!(raw));
                            if allow_commit {
                                *raw = Some(request["replacement"].as_str().unwrap().into());
                                json!({"outcome":"committed"})
                            } else {
                                json!({"outcome":"failed"})
                            }
                        }
                        op => panic!("unexpected storage operation: {op}"),
                    };
                    assert!(store.reply(request["id"].as_u64().unwrap(), reply));
                    requests.push(request);
                }
                Poll::Pending
            }),
        )
        .await
        .expect("operation should complete using only storage callbacks");
        assert!(store.requests.lock().unwrap().is_empty());
        assert!(store.pending.lock().unwrap().is_empty());
        (result, requests)
    }

    #[tokio::test]
    async fn saving_too_many_records_never_dispatches_a_commit() {
        let mut profile = full_profile();
        let mut extra = profile.records[0].clone();
        extra.link.grant_id = uuid::Uuid::from_u128(MAX_RECORDS as u128).to_string();
        profile.records.push(extra);
        let store = CallbackStore::new();
        let error = profile.save(&store, None).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<Fault>(),
            Some(&Fault::CapacityExceeded)
        );
        assert!(store.requests.lock().unwrap().is_empty());
        assert!(store.pending.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn full_profile_rejects_new_grants_and_preserves_identity_and_records() {
        let profile = full_profile();
        let original = serde_json::to_string(&profile).unwrap();
        let mut raw = Some(original.clone());
        let store = Arc::new(CallbackStore::new());
        let client = Client::default();
        for mode in [Mode::Owner, Mode::Share] {
            let link = Link {
                version: if mode == Mode::Owner { 1 } else { 2 },
                grant_id: uuid::Uuid::from_u128(MAX_RECORDS as u128).to_string(),
                mode,
                ..profile.records[0].link.clone()
            };
            // Closing the active session precedes binding the transport. Keeping
            // its lock held proves overflow exits before either network effect.
            let active = client.active.write().await;
            let (result, requests) = drive(
                &store,
                &mut raw,
                true,
                client.connect(registration(&link), store.clone()),
            )
            .await;
            assert_eq!(
                result.unwrap_err().downcast_ref::<Fault>(),
                Some(&Fault::CapacityExceeded)
            );
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0]["op"], "read");
            assert_eq!(raw.as_deref(), Some(original.as_str()));
            drop(active);
        }

        let (listed, _) = drive(
            &store,
            &mut raw,
            true,
            client.execute_command(Command::List {}, store.clone()),
        )
        .await;
        assert_eq!(listed.unwrap().as_array().unwrap().len(), MAX_RECORDS);
        let (forgotten, requests) = drive(
            &store,
            &mut raw,
            true,
            client.execute_command(
                Command::Forget {
                    id: profile.records[0].link.id(),
                },
                store.clone(),
            ),
        )
        .await;
        forgotten.unwrap();
        assert_eq!(requests.len(), 2);
        let (loaded, _) = drive(&store, &mut raw, true, Profile::load(&store)).await;
        let (remaining, _) = loaded.unwrap();
        assert_eq!(
            remaining.key().unwrap().public(),
            profile.key().unwrap().public()
        );
        assert_eq!(remaining.key, profile.key);
        assert_eq!(
            serde_json::to_value(&remaining.records).unwrap(),
            serde_json::to_value(&profile.records[1..]).unwrap()
        );
    }

    #[tokio::test]
    async fn full_profile_can_save_and_register_an_existing_grant() {
        let mut profile = full_profile();
        let original_key = profile.key.clone();
        let mut raw = Some(serde_json::to_string(&profile).unwrap());
        let expected = raw.clone();
        profile.records[0].accepted = true;
        let store = Arc::new(CallbackStore::new());
        let (saved, requests) = drive(&store, &mut raw, true, profile.save(&store, expected)).await;
        saved.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["op"], "commit");
        let (loaded, _) = drive(&store, &mut raw, true, Profile::load(&store)).await;
        let (loaded, _) = loaded.unwrap();
        assert_eq!(loaded.records.len(), MAX_RECORDS);
        assert!(loaded.records[0].accepted);
        assert_eq!(loaded.key, original_key);

        // Fail the persistence callback to stop before network access, while
        // proving an existing ID passes the new-record capacity check.
        let client = Client::default();
        let before = raw.clone();
        let (registered, requests) = drive(
            &store,
            &mut raw,
            false,
            client.connect(registration(&loaded.records[0].link), store.clone()),
        )
        .await;
        assert_eq!(
            registered.unwrap_err().downcast_ref::<Fault>(),
            Some(&Fault::StorageUnavailable)
        );
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1]["op"], "commit");
        assert_eq!(raw, before);
    }
}
