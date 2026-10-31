//! Session generations keep stale updates and actions out of replacement connections.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Service {
    pub id: String,
    pub name: String,
    pub generation: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Control {
    Hello {
        protocol: u32,
        #[serde(rename = "nodeId")]
        node_id: String,
        name: String,
    },
    Services {
        services: Vec<Service>,
    },
    Ping {
        id: u64,
    },
    Pong {
        id: u64,
    },
    Bye {
        reason: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ServiceFrame {
    SnapshotStart,
    Message { message: Value },
    SnapshotEnd,
}
pub fn valid_node_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn valid_service_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id != "."
        && id != ".."
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// This guard is owned by a single session actor. Actions are never queued for replay.
#[derive(Default)]
pub struct Session {
    epoch: u64,
    services: BTreeMap<String, (u64, bool)>,
}
impl Session {
    pub fn reset(&mut self) -> u64 {
        self.epoch = self.epoch.checked_add(1).expect("session epoch exhausted");
        self.services.clear();
        self.epoch
    }
    pub fn replace_services(
        &mut self,
        epoch: u64,
        services: &[Service],
    ) -> Result<(), &'static str> {
        if epoch != self.epoch {
            return Err("stale session");
        }
        let mut next = BTreeMap::new();
        for service in services {
            if !valid_service_id(&service.id)
                || service.name.trim().is_empty()
                || service.generation == 0
            {
                return Err("invalid service");
            }
            let ready = self
                .services
                .get(&service.id)
                .is_some_and(|(generation, ready)| *generation == service.generation && *ready);
            if next
                .insert(service.id.clone(), (service.generation, ready))
                .is_some()
            {
                return Err("duplicate service");
            }
        }
        self.services = next;
        Ok(())
    }
    pub fn snapshot_complete(
        &mut self,
        epoch: u64,
        id: &str,
        generation: u64,
    ) -> Result<(), &'static str> {
        if epoch != self.epoch {
            return Err("stale session");
        }
        let entry = self.services.get_mut(id).ok_or("unknown service")?;
        if entry.0 != generation {
            return Err("stale service");
        }
        entry.1 = true;
        Ok(())
    }
    pub fn can_send_action(&self, epoch: u64, id: &str, generation: u64) -> bool {
        epoch == self.epoch && self.services.get(id) == Some(&(generation, true))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reconnect_and_service_restart_require_a_fresh_snapshot() {
        let mut session = Session::default();
        let first = session.reset();
        let mut service = Service {
            id: "app".into(),
            name: "App".into(),
            generation: 1,
            icon: None,
        };
        session
            .replace_services(first, std::slice::from_ref(&service))
            .unwrap();
        assert!(!session.can_send_action(first, "app", 1));
        session.snapshot_complete(first, "app", 1).unwrap();
        assert!(session.can_send_action(first, "app", 1));
        service.generation = 2;
        session
            .replace_services(first, std::slice::from_ref(&service))
            .unwrap();
        assert!(!session.can_send_action(first, "app", 1));
        assert!(session.snapshot_complete(first, "app", 1).is_err());
        let second = session.reset();
        session.replace_services(second, &[service]).unwrap();
        assert!(session.snapshot_complete(first, "app", 2).is_err());
        assert!(!session.can_send_action(second, "app", 2));
    }
    #[test]
    fn invalid_service_list_does_not_partially_replace_current_state() {
        let mut session = Session::default();
        let epoch = session.reset();
        let service = Service {
            id: "app".into(),
            name: "App".into(),
            generation: 1,
            icon: None,
        };
        session
            .replace_services(epoch, std::slice::from_ref(&service))
            .unwrap();
        session.snapshot_complete(epoch, "app", 1).unwrap();
        assert!(
            session
                .replace_services(epoch, &[service.clone(), service])
                .is_err()
        );
        assert!(session.can_send_action(epoch, "app", 1));
    }
}
