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

// Legacy modes are recognized only in protected storage, never in invitations
// or the downstream wire protocol.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredProfile {
    version: u8,
    key: String,
    records: Vec<StoredRecord>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRecord {
    link: StoredLink,
    accepted: bool,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum StoredLink {
    Current(Link),
    Legacy(LegacyLink),
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyLink {
    version: u8,
    hub_id: String,
    relay_urls: Vec<String>,
    grant_id: String,
    #[serde(rename = "mode")]
    _mode: LegacyMode,
    secret: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum LegacyMode {
    Invite,
    Public,
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
        let mut raw = store.read().await?;
        let Some(document) = &raw else {
            return Ok((
                Self {
                    version: 1,
                    key: STANDARD.encode(SecretKey::generate().to_bytes()),
                    records: vec![],
                },
                None,
            ));
        };
        let stored: StoredProfile =
            serde_json::from_str(document).map_err(|_| anyhow::anyhow!(Fault::StorageCorrupt))?;
        anyhow::ensure!(
            stored.version == 1 && stored.records.len() <= MAX_RECORDS,
            Fault::StorageCorrupt
        );
        let mut profile = Self {
            version: stored.version,
            key: stored.key,
            records: vec![],
        };
        profile.key()?;
        let mut ids = std::collections::BTreeSet::new();
        let mut retired = false;
        for record in stored.records {
            let (link, legacy) = match record.link {
                StoredLink::Current(link) => (link, false),
                StoredLink::Legacy(link) => {
                    // Owner v1 has the same field validation as legacy shares.
                    // This temporary link is validated and discarded, never saved.
                    (
                        Link {
                            version: link.version,
                            hub_id: link.hub_id,
                            relay_urls: link.relay_urls,
                            grant_id: link.grant_id,
                            mode: Mode::Owner,
                            secret: link.secret,
                        },
                        true,
                    )
                }
            };
            anyhow::ensure!(ids.insert(link.id()), Fault::StorageCorrupt);
            link.validate()
                .map_err(|_| anyhow::anyhow!(Fault::StorageCorrupt))?;
            if legacy {
                retired = true;
            } else {
                profile.records.push(Record {
                    link,
                    accepted: record.accepted,
                });
            }
        }
        if retired {
            profile.save(store, raw).await?;
            // Further mutations must compare against the committed migration.
            raw = Some(serde_json::to_string(&profile)?);
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
        drive_with_outcome(
            store,
            raw,
            || if allow_commit { "committed" } else { "failed" },
            operation,
        )
        .await
    }

    async fn drive_with_outcome<F: Future>(
        store: &CallbackStore,
        raw: &mut Option<String>,
        mut outcome: impl FnMut() -> &'static str,
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
                            let outcome = outcome();
                            if outcome == "committed" {
                                *raw = Some(request["replacement"].as_str().unwrap().into());
                            }
                            json!({"outcome":outcome})
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

    fn legacy_profile() -> (Profile, String) {
        let mut profile = full_profile();
        profile.records.truncate(3);
        profile.records[2].link.version = 2;
        profile.records[2].link.mode = Mode::Share;
        let mut stored = serde_json::to_value(&profile).unwrap();
        for (index, mode) in ["invite", "public"].into_iter().enumerate() {
            for accepted in [false, true] {
                let mut record = stored["records"][0].clone();
                record["link"]["grantId"] =
                    uuid::Uuid::from_u128((3 + index * 2 + usize::from(accepted)) as u128)
                        .to_string()
                        .into();
                record["link"]["mode"] = mode.into();
                record["accepted"] = accepted.into();
                stored["records"].as_array_mut().unwrap().push(record);
            }
        }
        (profile, stored.to_string())
    }

    #[tokio::test]
    async fn legacy_shares_are_retired_without_losing_identity_or_current_records() {
        let (profile, original) = legacy_profile();
        let mut raw = Some(original.clone());
        let store = Arc::new(CallbackStore::new());
        let client = Client::default();
        let (listed, requests) = drive(
            &store,
            &mut raw,
            true,
            client.execute_command(Command::List {}, store.clone()),
        )
        .await;
        let listed = listed.unwrap();
        assert_eq!(listed.as_array().unwrap().len(), profile.records.len());
        for (listed, record) in listed.as_array().unwrap().iter().zip(&profile.records) {
            assert_eq!(listed["id"], record.link.id());
            assert_eq!(listed["accepted"], record.accepted);
            assert!(listed.get("secret").is_none());
        }
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1]["op"], "commit");
        assert_eq!(
            serde_json::from_str::<Value>(raw.as_deref().unwrap()).unwrap(),
            serde_json::to_value(&profile).unwrap()
        );

        // A second load is read-only and returns the migrated CAS token.
        let (loaded, requests) = drive(&store, &mut raw, true, Profile::load(&store)).await;
        let (loaded, expected) = loaded.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(expected, raw);
        assert_eq!(
            loaded.key().unwrap().public(),
            profile.key().unwrap().public()
        );

        // Forget must also work as the first operation after upgrading, using
        // the migrated document as its expected value for the second commit.
        raw = Some(original);
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
        assert_eq!(requests.len(), 3);
        let remaining: Profile = serde_json::from_str(raw.as_deref().unwrap()).unwrap();
        assert_eq!(remaining.key, profile.key);
        assert_eq!(
            serde_json::to_value(&remaining.records).unwrap(),
            serde_json::to_value(&profile.records[1..]).unwrap()
        );
    }

    #[tokio::test]
    async fn legacy_retirement_preserves_storage_failure_categories() {
        for (outcome, fault) in [
            ("conflict", Fault::StorageConflict),
            ("failed", Fault::StorageUnavailable),
            ("unknown", Fault::StorageOutcomeUnknown),
        ] {
            let (_, original) = legacy_profile();
            let mut raw = Some(original.clone());
            let store = CallbackStore::new();
            let (result, requests) =
                drive_with_outcome(&store, &mut raw, || outcome, Profile::load(&store)).await;
            let error = result
                .err()
                .expect("migration must report the failed commit");
            assert_eq!(error.downcast_ref::<Fault>(), Some(&fault));
            assert_eq!(requests.len(), 2);
            assert_eq!(raw.as_deref(), Some(original.as_str()));
        }
    }

    #[tokio::test]
    async fn legacy_retirement_does_not_hide_corrupt_storage() {
        let (_, original) = legacy_profile();
        for (path, value) in [
            ("/version", json!(2)),
            ("/key", json!("invalid")),
            ("/records/3/link/mode", json!("unknown")),
            ("/records/3/link/version", json!(2)),
            ("/records/3/link/secret", json!("invalid")),
            ("/records/3/link/hubId", json!("invalid")),
            ("/records/3/link/relayUrls", json!([])),
            (
                "/records/3/link/grantId",
                json!(uuid::Uuid::nil().to_string()),
            ),
            ("/records/3/accepted", Value::Null),
            ("/records/0/link/secret", json!("invalid")),
        ] {
            let mut stored: Value = serde_json::from_str(&original).unwrap();
            *stored.pointer_mut(path).unwrap() = value;
            let before = Some(stored.to_string());
            let mut raw = before.clone();
            let store = CallbackStore::new();
            let (result, requests) = drive(&store, &mut raw, true, Profile::load(&store)).await;
            let error = result.err().expect("corrupt storage must remain an error");
            assert_eq!(
                error.downcast_ref::<Fault>(),
                Some(&Fault::StorageCorrupt),
                "{path}"
            );
            assert_eq!(requests.len(), 1);
            assert_eq!(raw, before);
        }
    }

    #[tokio::test]
    async fn migrated_profiles_can_connect_owners_and_register_replacement_shares() {
        let (profile, original) = legacy_profile();
        let replacement = Link {
            grant_id: uuid::Uuid::from_u128(3).to_string(),
            ..profile.records[2].link.clone()
        };
        for command in [
            Command::Connect {
                id: profile.records[1].link.id(),
            },
            registration(&replacement),
        ] {
            let mut expected_records = profile.records.clone();
            if matches!(command, Command::Register { .. }) {
                expected_records.push(Record {
                    link: replacement.clone(),
                    accepted: false,
                });
            }
            let mut raw = Some(original.clone());
            let store = Arc::new(CallbackStore::new());
            let client = Client::default();
            // Commit retirement, then stop at the operation's own save so no
            // network is needed. Both paths must use the migrated CAS token.
            let mut outcomes = ["committed", "failed"].into_iter();
            let (result, requests) = drive_with_outcome(
                &store,
                &mut raw,
                || outcomes.next().expect("unexpected extra commit"),
                client.execute_command(command, store.clone()),
            )
            .await;
            assert_eq!(
                result.unwrap_err().downcast_ref::<Fault>(),
                Some(&Fault::StorageUnavailable)
            );
            assert_eq!(requests.len(), 3);
            let pending: Profile =
                serde_json::from_str(requests[2]["replacement"].as_str().unwrap()).unwrap();
            assert_eq!(pending.key, profile.key);
            assert_eq!(
                serde_json::to_value(&pending.records).unwrap(),
                serde_json::to_value(&expected_records).unwrap()
            );
            assert_eq!(
                serde_json::from_str::<Value>(raw.as_deref().unwrap()).unwrap(),
                serde_json::to_value(&profile).unwrap()
            );
        }
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
