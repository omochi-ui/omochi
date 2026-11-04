//! LAN discovery is an address source, never authorization. Native backends own
//! their registrations and callbacks for exactly one endpoint lifetime.
use anyhow::Result;
use iroh::{
    EndpointId,
    address_lookup::{AddressLookup, EndpointData, Error, Item},
};
#[cfg(not(any(target_vendor = "apple", windows)))]
use iroh_mdns_address_lookup::MdnsAddressLookup;
use n0_future::boxed::BoxStream;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{mpsc, watch};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

#[cfg(target_vendor = "apple")]
mod apple;
#[cfg(target_os = "linux")]
mod avahi;
mod records;
#[cfg(windows)]
mod windows;

use records::{Advertisement, ServiceRecords};
pub(crate) const MAX_SERVICES: usize = 64;
pub(crate) const MAX_RECORDS: usize = 128;
#[cfg(any(target_vendor = "apple", target_os = "linux"))]
pub(crate) const MAX_TARGETS: usize = 8;
pub(crate) const SERVICE: &str = "_irohv1._udp";

#[derive(Debug)]
pub struct LanDiscovery {
    backend: Backend,
}

#[derive(Debug)]
enum Backend {
    System(SystemLookup),
    #[cfg(not(any(target_vendor = "apple", windows)))]
    Rust(MdnsAddressLookup),
}

impl LanDiscovery {
    pub async fn new(id: EndpointId) -> Result<Self> {
        let (events, receiver) = mpsc::channel(256);
        let (publication, updates) = watch::channel(None);
        let cancel = CancellationToken::new();
        #[cfg(target_vendor = "apple")]
        {
            apple::start(id, events, updates, cancel.clone())?;
            Ok(Self {
                backend: Backend::System(SystemLookup::new(
                    "bonjour",
                    id,
                    publication,
                    receiver,
                    cancel,
                )),
            })
        }
        #[cfg(windows)]
        {
            windows::start(id, events, updates, cancel.clone())?;
            Ok(Self {
                backend: Backend::System(SystemLookup::new(
                    "windows-dns-sd",
                    id,
                    publication,
                    receiver,
                    cancel,
                )),
            })
        }
        #[cfg(target_os = "linux")]
        if let Some(connection) = avahi::connect().await {
            avahi::start(id, connection, events, updates, cancel.clone());
            return Ok(Self {
                backend: Backend::System(SystemLookup::new(
                    "avahi",
                    id,
                    publication,
                    receiver,
                    cancel,
                )),
            });
        }
        #[cfg(not(any(target_vendor = "apple", windows)))]
        Ok(Self {
            backend: Backend::Rust(MdnsAddressLookup::builder().build(id)?),
        })
    }

    pub fn backend(&self) -> &'static str {
        match &self.backend {
            Backend::System(system) => system.0.name,
            #[cfg(not(any(target_vendor = "apple", windows)))]
            Backend::Rust(_) => "rust-mdns",
        }
    }
}

impl AddressLookup for LanDiscovery {
    fn publish(&self, data: &EndpointData) {
        match &self.backend {
            Backend::System(system) => {
                system
                    .0
                    .publication
                    .send_replace(Advertisement::from_data(data));
            }
            #[cfg(not(any(target_vendor = "apple", windows)))]
            Backend::Rust(rust) => rust.publish(data),
        }
    }
    fn resolve(&self, id: EndpointId) -> Option<BoxStream<Result<Item, Error>>> {
        match &self.backend {
            #[cfg(not(any(target_vendor = "apple", windows)))]
            Backend::Rust(rust) => rust.resolve(id),
            Backend::System(system) => {
                let (sender, receiver) = mpsc::channel(4);
                if system.0.queries.try_send((id, sender.clone())).is_err() {
                    let _ = sender.try_send(Err(error(
                        system.0.name,
                        "LAN lookup queue is full or stopped",
                    )));
                }
                Some(Box::pin(ReceiverStream::new(receiver)))
            }
        }
    }
}

#[derive(Clone, Debug)]
struct SystemLookup(Arc<Inner>);
#[derive(Debug)]
struct Inner {
    name: &'static str,
    publication: watch::Sender<Option<Advertisement>>,
    queries: mpsc::Sender<(EndpointId, mpsc::Sender<Result<Item, Error>>)>,
    cancel: CancellationToken,
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[derive(Debug)]
pub(crate) enum Event {
    Ready,
    #[cfg(test)]
    Barrier(tokio::sync::oneshot::Sender<()>),
    Reset(String),
    Found {
        token: u64,
        instance: String,
        interface: u32,
    },
    Removed(u64),
    Record {
        token: u64,
        name: String,
        kind: u16,
        data: Vec<u8>,
        add: bool,
    },
}

pub(crate) fn error(name: &'static str, message: impl Into<String>) -> Error {
    Error::from_err(name, std::io::Error::other(message.into()))
}

impl SystemLookup {
    fn new(
        name: &'static str,
        own: EndpointId,
        publication: watch::Sender<Option<Advertisement>>,
        mut events: mpsc::Receiver<Event>,
        cancel: CancellationToken,
    ) -> Self {
        let (queries, mut requests) =
            mpsc::channel::<(EndpointId, mpsc::Sender<Result<Item, Error>>)>(128);
        let stop = cancel.clone();
        tokio::spawn(async move {
            let mut services = HashMap::<u64, ServiceRecords>::new();
            let mut pending = Vec::<(
                EndpointId,
                mpsc::Sender<Result<Item, Error>>,
                tokio::time::Instant,
            )>::new();
            let mut failure: Option<String> = None;
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                let changed = tokio::select! {
                    biased;
                    _ = stop.cancelled() => break,
                    query = requests.recv() => {
                        let Some((id, sender)) = query else { break };
                        if let Some(message) = &failure {
                            let _ = sender.try_send(Err(error(name, message.clone())));
                        } else if pending.len() < 128 {
                            if let Some(item) = records::item(name, id, services.values()) {
                                let _ = sender.try_send(Ok(item));
                            }
                            pending.push((id, sender, tokio::time::Instant::now() + Duration::from_secs(10)));
                        } else { let _ = sender.try_send(Err(error(name, "Too many LAN lookups"))); }
                        None
                    }
                    event = events.recv() => match event {
                        #[cfg(test)]
                        Some(Event::Barrier(sender)) => { let _=sender.send(()); None }
                        Some(Event::Ready) => { failure = None; None }
                        Some(Event::Reset(message)) => {
                            tracing::warn!(backend=name, %message, "LAN discovery restarting");
                            services.clear();
                            for (_, sender, _) in pending.drain(..) { let _ = sender.try_send(Err(error(name, message.clone()))); }
                            failure = Some(message);
                            None
                        }
                        Some(Event::Found {token, instance, interface}) => {
                            if services.len() < MAX_SERVICES
                                && let Some(service) = ServiceRecords::new(&instance, interface).filter(|s| s.id != own) { services.insert(token, service); }
                            None
                        }
                        Some(Event::Removed(token)) => { services.remove(&token).map(|s| s.id) }
                        Some(Event::Record {token, name, kind, data, add}) => {
                            services.get_mut(&token).and_then(|s| s.apply(name, kind, data, add).then_some(s.id))
                        }
                        None => break,
                    },
                    _ = tick.tick() => None,
                };
                let now = tokio::time::Instant::now();
                pending.retain(|(id, sender, until)| {
                    if sender.is_closed() || *until <= now {
                        return false;
                    }
                    if changed == Some(*id)
                        && let Some(item) = records::item(name, *id, services.values())
                        && sender.try_send(Ok(item)).is_err()
                    {
                        return false;
                    }
                    true
                });
            }
            // Native workers check cancellation before callbacks and withdraw all
            // registrations. They own callback storage until the OS has stopped.
            stop.cancel();
        });
        Self(Arc::new(Inner {
            name,
            publication,
            queries,
            cancel,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use n0_future::StreamExt;
    async fn flush(events: &mpsc::Sender<Event>) {
        let (send, receive) = tokio::sync::oneshot::channel();
        events.send(Event::Barrier(send)).await.unwrap();
        receive.await.unwrap();
    }
    async fn record(
        events: &mpsc::Sender<Event>,
        token: u64,
        name: String,
        kind: u16,
        data: Vec<u8>,
    ) {
        events
            .send(Event::Record {
                token,
                name,
                kind,
                data,
                add: true,
            })
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn removal_reset_and_shutdown_invalidate_the_old_generation() {
        let own = iroh::SecretKey::from_bytes(&[1; 32]).public();
        let peer = iroh::SecretKey::from_bytes(&[2; 32]).public();
        let (events, receiver) = mpsc::channel(256);
        let (publication, _updates) = watch::channel(None);
        let cancel = CancellationToken::new();
        let lookup = LanDiscovery {
            backend: Backend::System(SystemLookup::new(
                "test",
                own,
                publication,
                receiver,
                cancel.clone(),
            )),
        };
        let full = format!("{}.{SERVICE}.local.", records::instance(peer));
        let srv = vec![
            0, 0, 0, 0, 0x12, 0x34, 4, b'h', b'o', b's', b't', 5, b'l', b'o', b'c', b'a', b'l', 0,
        ];
        events
            .send(Event::Found {
                token: 1,
                instance: records::instance(peer),
                interface: 7,
            })
            .await
            .unwrap();
        record(&events, 1, full.clone(), 33, srv.clone()).await;
        record(&events, 1, "host.local.".into(), 1, vec![192, 0, 2, 1]).await;
        flush(&events).await;
        let mut live = lookup.resolve(peer).unwrap();
        assert_eq!(live.next().await.unwrap().unwrap().endpoint_id(), peer);
        events.send(Event::Removed(1)).await.unwrap();
        record(&events, 1, "host.local.".into(), 1, vec![192, 0, 2, 2]).await;
        events
            .send(Event::Found {
                token: 2,
                instance: records::instance(peer),
                interface: 7,
            })
            .await
            .unwrap();
        record(&events, 2, full, 33, srv).await;
        flush(&events).await;
        let mut fresh = lookup.resolve(peer).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(30), fresh.next())
                .await
                .is_err()
        );
        record(&events, 2, "host.local.".into(), 1, vec![192, 0, 2, 3]).await;
        let item = fresh.next().await.unwrap().unwrap().to_endpoint_addr();
        assert_eq!(
            item.ip_addrs().copied().collect::<Vec<_>>(),
            vec!["192.0.2.3:4660".parse::<std::net::SocketAddr>().unwrap()]
        );
        events
            .send(Event::Reset("network changed".into()))
            .await
            .unwrap();
        assert!(fresh.next().await.unwrap().is_err());
        assert!(fresh.next().await.is_none());
        flush(&events).await;
        let mut failed = lookup.resolve(peer).unwrap();
        assert!(failed.next().await.unwrap().is_err());
        drop(lookup);
        assert!(cancel.is_cancelled());
        tokio::time::timeout(Duration::from_secs(1), events.closed())
            .await
            .unwrap();
    }
}
