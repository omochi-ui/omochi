//! Windows browse callbacks share records across services on an interface.
//! Keep early address records here, and publish only each service's dependencies.
use super::{Event, MAX_RECORDS, MAX_SERVICES, SERVICE, records};
use anyhow::{Context as _, Result, ensure};
use iroh::EndpointId;
use std::{
    collections::{BTreeSet, HashMap},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct Record {
    pub name: String,
    pub kind: u16,
    pub data: Vec<u8>,
}
impl Record {
    fn event(&self, token: u64, add: bool) -> Event {
        Event::Record {
            token,
            name: self.name.clone(),
            kind: self.kind,
            data: self.data.clone(),
            add,
        }
    }

    fn service(&self) -> Option<String> {
        let name = match self.kind {
            12 if self.name == format!("{SERVICE}.local.") => records::dns_name(&self.data)?,
            33 => self.name.clone(),
            _ => return None,
        };
        let id = records::endpoint(&name)?;
        (name == format!("{}.{SERVICE}.local.", records::instance(id))).then_some(name)
    }
}

struct Service {
    token: u64,
    published: BTreeSet<Record>,
}

#[derive(Default)]
pub(super) struct Cache {
    records: HashMap<(u32, Record), Instant>,
    services: HashMap<(u32, String), Service>,
}
impl Cache {
    pub fn update(
        &mut self,
        own: EndpointId,
        interface: u32,
        records: Vec<(Record, u32)>,
        now: Instant,
        serial: &mut u64,
        mut emit: impl FnMut(Event) -> Result<()>,
    ) -> Result<()> {
        self.records.retain(|_, until| *until > now);
        for (record, ttl) in records {
            let key = (interface, record.clone());
            if ttl == 0 {
                self.records.remove(&key);
            } else {
                ensure!(
                    self.records.len() < MAX_SERVICES * MAX_RECORDS
                        || self.records.contains_key(&key),
                    "Windows DNS-SD cache exceeded capacity"
                );
                self.records
                    .insert(key, now + Duration::from_secs(ttl.min(4500) as u64));
            }
            if let Some(name) = record
                .service()
                .filter(|name| records::endpoint(name) != Some(own))
            {
                let key = (interface, name.clone());
                if ttl == 0 && record.kind == 12 {
                    if let Some(service) = self.services.remove(&key) {
                        emit(Event::Removed(service.token))?;
                    }
                    self.records
                        .retain(|(i, rr), _| *i != interface || rr.name != name);
                }
                if ttl > 0
                    && !self.services.contains_key(&key)
                    && self.services.len() < MAX_SERVICES
                {
                    *serial = serial
                        .checked_add(1)
                        .context("Windows DNS-SD generation exhausted")?;
                    emit(Event::Found {
                        token: *serial,
                        instance: name,
                        interface,
                    })?;
                    self.services.insert(
                        key,
                        Service {
                            token: *serial,
                            published: BTreeSet::new(),
                        },
                    );
                }
            }
        }

        let gone: Vec<_> = self
            .services
            .keys()
            .filter(|(interface, name)| {
                !self.records.keys().any(|(i, rr)| {
                    i == interface
                        && ((rr.kind == 33 && &rr.name == name)
                            || (rr.kind == 12
                                && rr.name == format!("{SERVICE}.local.")
                                && records::dns_name(&rr.data).as_ref() == Some(name)))
                })
            })
            .cloned()
            .collect();
        for key in gone {
            emit(Event::Removed(self.services.remove(&key).unwrap().token))?;
        }

        // Recompute after the entire callback, so A/AAAA may precede SRV even
        // across callbacks. Shared hosts are dependencies of every referencing
        // service, but never cross interface boundaries.
        for ((interface, name), service) in &mut self.services {
            let hosts: BTreeSet<_> = self
                .records
                .keys()
                .filter(|(i, rr)| i == interface && rr.kind == 33 && &rr.name == name)
                .filter_map(|(_, rr)| records::srv(&rr.data).map(|(host, _)| host))
                .collect();
            let relevant: BTreeSet<_> = self
                .records
                .keys()
                .filter(|(i, rr)| {
                    i == interface
                        && match rr.kind {
                            16 | 33 => &rr.name == name,
                            1 | 28 => hosts.contains(&rr.name),
                            _ => false,
                        }
                })
                .map(|(_, rr)| rr.clone())
                .collect();
            ensure!(
                relevant.len() <= MAX_RECORDS,
                "Windows DNS-SD service records exceeded capacity"
            );
            // Free slots before replacements, including when the service was
            // exactly at capacity. Losing an SRV also removes its orphaned IPs.
            for rr in service.published.difference(&relevant) {
                emit(rr.event(service.token, false))?;
            }
            for rr in relevant.difference(&service.published) {
                emit(rr.event(service.token, true))?;
            }
            service.published = relevant;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::ServiceRecords;
    use std::net::SocketAddr;

    fn id(seed: u8) -> EndpointId {
        iroh::SecretKey::from_bytes(&[seed; 32]).public()
    }
    fn full(seed: u8) -> String {
        format!("{}.{SERVICE}.local.", records::instance(id(seed)))
    }
    fn wire(name: &str) -> Vec<u8> {
        name.trim_end_matches('.')
            .split('.')
            .flat_map(|label| std::iter::once(label.len() as u8).chain(label.bytes()))
            .chain([0])
            .collect()
    }
    fn srv(seed: u8, host: &str) -> Record {
        Record {
            name: full(seed),
            kind: 33,
            data: [vec![0, 0, 0, 0, 0x12, 0x34], wire(host)].concat(),
        }
    }
    fn txt(seed: u8, value: &str) -> Record {
        let value = format!("user-data={value}");
        Record {
            name: full(seed),
            kind: 16,
            data: [vec![value.len() as u8], value.into_bytes()].concat(),
        }
    }
    fn a(host: &str, last: u8) -> Record {
        Record {
            name: host.into(),
            kind: 1,
            data: vec![192, 0, 2, last],
        }
    }
    fn aaaa(host: &str) -> Record {
        Record {
            name: host.into(),
            kind: 28,
            data: "fe80::42"
                .parse::<std::net::Ipv6Addr>()
                .unwrap()
                .octets()
                .to_vec(),
        }
    }
    fn ptr(seed: u8) -> Record {
        Record {
            name: format!("{SERVICE}.local."),
            kind: 12,
            data: wire(&full(seed)),
        }
    }

    struct Fixture {
        cache: Cache,
        serial: u64,
        now: Instant,
        services: HashMap<u64, ServiceRecords>,
    }
    impl Fixture {
        fn new() -> Self {
            Self {
                cache: Cache::default(),
                serial: 0,
                now: Instant::now(),
                services: HashMap::new(),
            }
        }
        fn update(&mut self, interface: u32, records: Vec<(Record, u32)>) -> Vec<Event> {
            let mut events = Vec::new();
            self.cache
                .update(
                    id(255),
                    interface,
                    records,
                    self.now,
                    &mut self.serial,
                    |event| {
                        events.push(event);
                        Ok(())
                    },
                )
                .unwrap();
            // Exercise the actual bounded consumer, so a dropped addition fails
            // the test even if the cache thinks it has published the record.
            for event in &events {
                match event {
                    Event::Found {
                        token,
                        instance,
                        interface,
                    } => {
                        assert!(
                            self.services
                                .insert(*token, ServiceRecords::new(instance, *interface).unwrap())
                                .is_none()
                        );
                    }
                    Event::Removed(token) => {
                        assert!(self.services.remove(token).is_some());
                    }
                    Event::Record {
                        token,
                        name,
                        kind,
                        data,
                        add,
                    } => {
                        assert!(self.services.get_mut(token).unwrap().apply(
                            name.clone(),
                            *kind,
                            data.clone(),
                            *add
                        ));
                    }
                    _ => panic!("unexpected cache event"),
                }
            }
            events
        }
        fn add(&mut self, interface: u32, records: Vec<Record>) -> Vec<Event> {
            self.update(interface, records.into_iter().map(|rr| (rr, 120)).collect())
        }
        fn addresses(&self, seed: u8) -> BTreeSet<SocketAddr> {
            records::item("test", id(seed), self.services.values())
                .map(|item| item.to_endpoint_addr().ip_addrs().copied().collect())
                .unwrap_or_default()
        }
        fn published(&self, interface: u32, seed: u8) -> &BTreeSet<Record> {
            &self.cache.services[&(interface, full(seed))].published
        }
    }

    #[test]
    fn other_peers_do_not_consume_service_capacity_or_drop_updates() {
        let mut f = Fixture::new();
        for seed in 1..=41 {
            let host = format!("host{seed}.local.");
            f.add(
                7,
                vec![
                    a(&host, seed),
                    aaaa(&host),
                    txt(seed, "old"),
                    srv(seed, &host),
                ],
            );
        }
        assert!(f.cache.records.len() > MAX_RECORDS);
        assert!(f.services.len() < MAX_SERVICES);
        for seed in 1..=41 {
            assert_eq!(f.published(7, seed).len(), 4);
            assert_eq!(f.addresses(seed).len(), 2);
        }
        // Check both an early service and a service discovered after the shared
        // cache has already exceeded the per-service limit.
        for seed in [1, 41] {
            let host = format!("host{seed}.local.");
            let events = f.update(
                7,
                vec![
                    (a(&host, 100), 120),
                    (txt(seed, "new"), 120),
                    (a(&host, seed), 0),
                    (txt(seed, "old"), 0),
                ],
            );
            assert_eq!(events.len(), 4);
            assert!(
                f.addresses(seed)
                    .contains(&"192.0.2.100:4660".parse().unwrap())
            );
            assert!(
                !f.addresses(seed)
                    .contains(&format!("192.0.2.{seed}:4660").parse().unwrap())
            );
            assert!(f.published(7, seed).contains(&txt(seed, "new")));
            assert!(!f.published(7, seed).contains(&txt(seed, "old")));
        }
        assert!(f.add(7, vec![a("unrelated.local.", 200)]).is_empty());
        assert!(
            f.add(7, vec![srv(1, "host1.local.")]).is_empty(),
            "TTL refresh is not a record change"
        );
    }

    #[test]
    fn early_addresses_shared_hosts_and_interfaces_are_preserved() {
        let mut f = Fixture::new();
        assert!(
            f.add(7, vec![a("shared.local.", 1), aaaa("shared.local.")])
                .is_empty()
        );
        f.add(
            9,
            vec![
                a("shared.local.", 2),
                aaaa("shared.local."),
                srv(3, "shared.local."),
            ],
        );
        f.add(7, vec![srv(1, "shared.local."), srv(2, "shared.local.")]);
        for seed in [1, 2] {
            assert_eq!(
                f.addresses(seed),
                BTreeSet::from([
                    "192.0.2.1:4660".parse().unwrap(),
                    "[fe80::42%7]:4660".parse().unwrap(),
                ])
            );
        }
        assert_eq!(
            f.addresses(3),
            BTreeSet::from([
                "192.0.2.2:4660".parse().unwrap(),
                "[fe80::42%9]:4660".parse().unwrap(),
            ])
        );
        f.update(7, vec![(a("shared.local.", 1), 0)]);
        assert_eq!(f.addresses(1).len(), 1);
        assert_eq!(f.addresses(2).len(), 1);
        assert_eq!(f.addresses(3).len(), 2);
    }

    #[test]
    fn changing_srv_targets_removes_only_orphaned_addresses() {
        let mut f = Fixture::new();
        f.add(
            7,
            vec![
                a("first.local.", 1),
                a("second.local.", 2),
                srv(1, "first.local."),
                srv(1, "second.local."),
                srv(2, "first.local."),
            ],
        );
        f.update(7, vec![(srv(1, "first.local."), 0)]);
        assert_eq!(
            f.addresses(1),
            BTreeSet::from(["192.0.2.2:4660".parse().unwrap()])
        );
        assert!(!f.published(7, 1).contains(&a("first.local.", 1)));
        assert_eq!(
            f.addresses(2),
            BTreeSet::from(["192.0.2.1:4660".parse().unwrap()])
        );
        // The shared cache retains the address for a later SRV target change.
        f.add(7, vec![srv(1, "first.local.")]);
        assert_eq!(f.addresses(1).len(), 2);
    }

    #[test]
    fn replacements_remove_records_before_adding_at_capacity() {
        let mut f = Fixture::new();
        let mut initial = vec![srv(1, "old.local."), txt(1, "old")];
        initial.extend((1..=126).map(|n| a("old.local.", n)));
        f.add(7, initial);
        assert_eq!(f.published(7, 1).len(), MAX_RECORDS);
        f.add(7, (1..=126).map(|n| a("new.local.", n)).collect());
        let events = f.update(
            7,
            vec![
                (srv(1, "new.local."), 120),
                (txt(1, "new"), 120),
                (srv(1, "old.local."), 0),
                (txt(1, "old"), 0),
            ],
        );
        assert_eq!(events.len(), MAX_RECORDS * 2);
        assert!(
            events[..MAX_RECORDS]
                .iter()
                .all(|event| matches!(event, Event::Record { add: false, .. }))
        );
        assert!(
            events[MAX_RECORDS..]
                .iter()
                .all(|event| matches!(event, Event::Record { add: true, .. }))
        );
        assert_eq!(f.published(7, 1).len(), MAX_RECORDS);
        assert!(f.published(7, 1).iter().all(|rr| rr.name != "old.local."));
    }

    #[test]
    fn ttl_expiry_and_goodbye_remove_dependencies_and_invalidate_tokens() {
        let mut f = Fixture::new();
        f.update(
            7,
            vec![
                (ptr(1), 120),
                (txt(1, "old"), 120),
                (srv(1, "host.local."), 10),
                (a("host.local.", 1), 5),
                (aaaa("host.local."), 120),
            ],
        );
        f.now += Duration::from_secs(5);
        f.update(0, vec![]);
        assert_eq!(
            f.addresses(1),
            BTreeSet::from(["[fe80::42%7]:4660".parse().unwrap()])
        );
        f.now += Duration::from_secs(5);
        f.update(0, vec![]);
        assert!(f.addresses(1).is_empty());
        assert_eq!(f.published(7, 1), &BTreeSet::from([txt(1, "old")]));
        f.add(7, vec![srv(1, "host.local.")]);
        let token = f.cache.services[&(7, full(1))].token;
        let events = f.update(7, vec![(ptr(1), 0)]);
        assert!(matches!(events.as_slice(), [Event::Removed(t)] if *t == token));
        assert!(f.services.is_empty());
        assert!(f.add(7, vec![a("host.local.", 3)]).is_empty());
        f.add(7, vec![srv(1, "host.local.")]);
        assert!(f.cache.services[&(7, full(1))].token > token);
        assert!(!f.published(7, 1).contains(&txt(1, "old")));
        f.now += Duration::from_secs(121);
        f.update(0, vec![]);
        assert!(f.services.is_empty());
        assert!(f.cache.records.is_empty());
    }

    #[test]
    fn per_service_overflow_is_an_error_instead_of_silent_loss() {
        let mut f = Fixture::new();
        f.add(7, vec![srv(1, "host.local.")]);
        let error = f
            .cache
            .update(
                id(255),
                7,
                (1..=128).map(|n| (a("host.local.", n), 120)).collect(),
                f.now,
                &mut f.serial,
                |_| Ok(()),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("service records exceeded capacity")
        );
    }
}
