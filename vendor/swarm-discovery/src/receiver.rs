use crate::{sender::MdnsMsg, Peer, TxtData};
use acto::{ActoCell, ActoRef, ActoRuntime};
use hickory_proto::{
    op::Message,
    rr::{DNSClass, Name, RData, RecordType},
};
use std::{collections::BTreeMap, net::IpAddr, str::FromStr, sync::Arc, time::Instant};
use thiserror::Error;
use tokio::net::UdpSocket;

/// Errors that can occur when receiving on the socket.
#[derive(Debug, Error)]
#[error("Could not receive from the socket")]
pub struct ReceiverError {
    #[from]
    source: std::io::Error,
}

pub async fn receiver(
    _ctx: ActoCell<(), impl ActoRuntime>,
    service_name: Name,
    socket: Arc<UdpSocket>,
    target: ActoRef<MdnsMsg>,
) -> Result<(), ReceiverError> {
    let mut buf = [0; 1472];
    loop {
        let (len, addr) = socket
            .recv_from(&mut buf)
            .await
            .map_err(ReceiverError::from)?;
        let msg = &buf[..len];
        tracing::trace!("received {} bytes from {}", len, addr);
        if let Some(msg) = handle_msg(msg, &service_name, addr.ip()) {
            target.send(msg);
        }
    }
}

fn handle_msg(buf: &[u8], service_name: &Name, addr: IpAddr) -> Option<MdnsMsg> {
    let packet = match Message::from_vec(buf) {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!("error parsing mDNS packet: {}", e);
            return None;
        }
    };
    for question in &packet.queries {
        if question.query_class() != DNSClass::IN {
            tracing::trace!(
                "received mDNS query with wrong class {}",
                question.query_class()
            );
            continue;
        }
        if question.query_type() != RecordType::PTR {
            tracing::trace!(
                "received mDNS query with wrong type {}",
                question.query_type()
            );
            continue;
        }
        if question.name() != service_name {
            tracing::trace!("received mDNS query for wrong service {}", question.name());
            continue;
        }
        tracing::debug!("received mDNS query for {}", question.name());
        return Some(match addr {
            IpAddr::V4(_) => MdnsMsg::QueryV4,
            IpAddr::V6(_) => MdnsMsg::QueryV6,
        });
    }

    let local = Name::from_str("local.").unwrap();

    let mut peer_ports: BTreeMap<Name, Vec<(u16, String)>> = BTreeMap::new();
    let mut peer_txt: BTreeMap<String, TxtData> = BTreeMap::new();
    // Bonjour browse replies put SRV/TXT in additionals, and address-query
    // replies put A/AAAA in answers. Neither relationship is section-specific.
    let records = || packet.answers.iter().chain(&packet.additionals);
    for response in records() {
        if response.dns_class != DNSClass::IN {
            tracing::trace!(
                "received mDNS response with wrong class {:?}",
                response.dns_class
            );
            continue;
        }
        let name = &response.name;
        if name.base_name() != *service_name {
            tracing::trace!("received mDNS response with wrong service {}", name);
            continue;
        }
        tracing::debug!("received mDNS response for {}", name);
        let Some(peer_id_bytes) = name.iter().next() else {
            continue;
        };
        let Ok(peer_id) = std::str::from_utf8(peer_id_bytes) else {
            tracing::debug!(
                "received mDNS response with invalid peer ID {:?}",
                peer_id_bytes
            );
            continue;
        };
        match &response.data {
            RData::SRV(srv) => {
                peer_ports
                    .entry(srv.target.clone())
                    .or_default()
                    .push((srv.port, peer_id.to_string()));
            }
            RData::TXT(txt) => {
                for s in txt.txt_data.iter() {
                    let Ok(s) = std::str::from_utf8(s) else {
                        continue;
                    };
                    if s.is_empty() {
                        continue;
                    }
                    let (key, value) = match s.split_once('=') {
                        Some((key, value)) => (key, Some(value)),
                        None => (s, None),
                    };
                    let map = peer_txt.entry(peer_id.to_string()).or_default();
                    if !map.contains_key(key) {
                        map.insert(key.to_string(), value.map(ToString::to_string));
                    }
                }
            }
            _ => {
                tracing::trace!("received mDNS response with wrong data {:?}", response.data);
            }
        }
    }

    let mut peer_addrs: BTreeMap<String, Vec<(IpAddr, u16)>> = BTreeMap::new();
    for additional in records() {
        if additional.dns_class != DNSClass::IN {
            tracing::trace!(
                "received mDNS additional with wrong class {:?}",
                additional.dns_class
            );
            continue;
        }
        let name = &additional.name;
        if name.base_name() != local {
            tracing::trace!("received mDNS additional for wrong service {}", name);
            continue;
        }
        tracing::trace!("received mDNS additional for {}", name);
        let ip: IpAddr = match &additional.data {
            RData::A(a) => a.0.into(),
            RData::AAAA(a) => a.0.into(),
            _ => {
                tracing::debug!(
                    "received mDNS additional with wrong data {:?}",
                    additional.data
                );
                continue;
            }
        };
        for (port, peer_id) in peer_ports.get(name).map(|x| &**x).unwrap_or(&[]) {
            peer_addrs
                .entry(peer_id.clone())
                .or_default()
                .push((ip, *port));
        }
    }
    let mut ret = BTreeMap::new();
    for (peer_id, mut addrs) in peer_addrs {
        addrs.sort_unstable();
        addrs.dedup();
        let txt = peer_txt.remove(&peer_id).unwrap_or_default();
        let last_seen = Instant::now();
        let peer = Peer {
            addrs,
            last_seen,
            txt,
        };
        ret.insert(peer_id, peer);
    }
    Some(MdnsMsg::Response(ret))
}

#[cfg(test)]
mod bonjour_tests {
    use super::*;
    use hickory_proto::{
        op::{MessageType, OpCode},
        rr::{rdata, Record},
    };

    fn records(cache_flush: bool) -> Vec<Record> {
        let instance = Name::from_str("peer._irohv1._udp.local.").unwrap();
        let host = Name::from_str("mac.local.").unwrap();
        let mut records = vec![
            Record::from_rdata(
                instance.clone(),
                120,
                RData::SRV(rdata::SRV::new(0, 0, 41973, host.clone())),
            ),
            Record::from_rdata(
                instance,
                120,
                RData::TXT(rdata::TXT::new(vec!["user-data=probe".into()])),
            ),
            Record::from_rdata(
                host.clone(),
                120,
                RData::A(rdata::A::from(
                    "192.0.2.4".parse::<std::net::Ipv4Addr>().unwrap(),
                )),
            ),
            Record::from_rdata(
                host,
                120,
                RData::AAAA(rdata::AAAA::from(
                    "2001:db8::4".parse::<std::net::Ipv6Addr>().unwrap(),
                )),
            ),
        ];
        if cache_flush {
            // Encode the real Bonjour wire CLASS without requiring the mdns feature
            // on the test's record builder. The decoder must strip bit 15.
            for record in &mut records {
                record.dns_class = DNSClass::Unknown(0x8001);
            }
        }
        records
    }

    fn parse(message: Message) -> BTreeMap<String, Peer> {
        let service = Name::from_str("_irohv1._udp.local.").unwrap();
        match handle_msg(
            &message.to_vec().unwrap(),
            &service,
            "192.0.2.4".parse().unwrap(),
        ) {
            Some(MdnsMsg::Response(peers)) => peers,
            _ => panic!("expected discovery response"),
        }
    }

    fn assert_peer(peers: BTreeMap<String, Peer>) {
        let peer = peers.get("peer").expect("Bonjour peer was discarded");
        assert_eq!(
            peer.addrs(),
            &[
                ("192.0.2.4".parse().unwrap(), 41973),
                ("2001:db8::4".parse().unwrap(), 41973),
            ]
        );
        assert_eq!(peer.txt_attribute("user-data"), Some(Some("probe")));
    }

    #[test]
    fn bonjour_cache_flush_records_resolve() {
        let mut message = Message::new(0, MessageType::Response, OpCode::Query);
        let mut records = records(true).into_iter();
        message.add_answer(records.next().unwrap());
        message.add_answer(records.next().unwrap());
        for record in records {
            message.add_additional(record);
        }
        assert_peer(parse(message));
    }

    #[test]
    fn bonjour_browse_additional_srv_and_answer_addresses_resolve() {
        let mut message = Message::new(0, MessageType::Response, OpCode::Query);
        for record in records(false) {
            message.add_additional(record);
        }
        assert_peer(parse(message));
        let mut message = Message::new(0, MessageType::Response, OpCode::Query);
        for record in records(false) {
            message.add_answer(record);
        }
        assert_peer(parse(message));
    }

    #[test]
    fn ordinary_swarm_records_remain_compatible() {
        let mut message = Message::new(0, MessageType::Response, OpCode::Query);
        let mut records = records(false).into_iter();
        message.add_answer(records.next().unwrap());
        message.add_answer(records.next().unwrap());
        for record in records {
            message.add_additional(record);
        }
        assert_peer(parse(message));
    }
}
