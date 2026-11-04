use super::{MAX_RECORDS, SERVICE};
use iroh::{
    EndpointId,
    address_lookup::{EndpointData, EndpointInfo, Item},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    net::{IpAddr, SocketAddr, SocketAddrV6},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Advertisement {
    pub port: u16,
    pub txt: Vec<Vec<u8>>,
}
impl Advertisement {
    pub fn from_data(data: &EndpointData, local_port: u16) -> Option<Self> {
        // Published addresses include NAT-mapped ports. Native DNS-SD supplies
        // the host's interface addresses, so pair them with the bound UDP port.
        // An empty IP set still withdraws an advertisement.
        data.ip_addrs().next()?;
        let mut txt = Vec::new();
        if let Some(relay) = data.relay_urls().next() {
            let value = format!("relay={relay}");
            if value.len() <= 255 {
                txt.push(value.into_bytes());
            }
        }
        if let Some(user) = data.user_data() {
            let value = format!("user-data={user}");
            if value.len() <= 255 {
                txt.push(value.into_bytes());
            }
        }
        Some(Self {
            port: local_port,
            txt,
        })
    }
    #[cfg(any(target_vendor = "apple", test))]
    pub fn wire_txt(&self) -> Vec<u8> {
        if self.txt.is_empty() {
            return vec![0];
        }
        self.txt
            .iter()
            .flat_map(|s| std::iter::once(s.len() as u8).chain(s.iter().copied()))
            .collect()
    }
}

pub(crate) fn instance(id: EndpointId) -> String {
    data_encoding::BASE32_NOPAD
        .encode(id.as_bytes())
        .to_ascii_lowercase()
}
pub(crate) fn endpoint(name: &str) -> Option<EndpointId> {
    let label = name.trim_end_matches('.').split('.').next()?;
    if label.len() != 52
        || label
            .bytes()
            .any(|c| !c.is_ascii_lowercase() && !(b'2'..=b'7').contains(&c))
    {
        return None;
    }
    let bytes = data_encoding::BASE32_NOPAD
        .decode(label.to_ascii_uppercase().as_bytes())
        .ok()?;
    let id = EndpointId::from_bytes(bytes.as_slice().try_into().ok()?).ok()?;
    (instance(id) == label).then_some(id)
}

pub(crate) fn dns_name(data: &[u8]) -> Option<String> {
    let mut pos = 0;
    let mut labels = Vec::new();
    while pos < data.len() && pos < 255 {
        let len = data[pos] as usize;
        pos += 1;
        if len == 0 {
            return (pos == data.len()).then(|| format!("{}.", labels.join(".")));
        }
        if len > 63 || pos + len > data.len() {
            return None;
        }
        let label = std::str::from_utf8(&data[pos..pos + len]).ok()?;
        if label.contains('.') || label.contains('\0') {
            return None;
        }
        labels.push(label.to_ascii_lowercase());
        pos += len;
    }
    None
}
pub(crate) fn srv(data: &[u8]) -> Option<(String, u16)> {
    if data.len() < 8 {
        return None;
    }
    let port = u16::from_be_bytes([data[4], data[5]]);
    (port != 0).then_some((dns_name(&data[6..])?, port))
}

#[derive(Debug)]
pub(crate) struct ServiceRecords {
    pub id: EndpointId,
    fullname: String,
    interface: u32,
    records: BTreeSet<(String, u16, Vec<u8>)>,
}
impl ServiceRecords {
    pub fn new(name: &str, interface: u32) -> Option<Self> {
        let id = endpoint(name)?;
        Some(Self {
            id,
            fullname: format!("{}.{SERVICE}.local.", instance(id)),
            interface,
            records: BTreeSet::new(),
        })
    }
    pub fn apply(&mut self, name: String, kind: u16, data: Vec<u8>, add: bool) -> bool {
        if name.len() > 255 || data.len() > 1024 || ![1, 16, 28, 33].contains(&kind) {
            return false;
        }
        let key = (
            format!("{}.", name.trim_end_matches('.').to_ascii_lowercase()),
            kind,
            data,
        );
        if add {
            if self.records.len() >= MAX_RECORDS {
                return false;
            }
            self.records.insert(key)
        } else {
            self.records.remove(&key)
        }
    }
    fn addresses(&self) -> BTreeSet<SocketAddr> {
        let mut out = BTreeSet::new();
        for (name, kind, data) in &self.records {
            if *kind != 33 || name != &self.fullname {
                continue;
            }
            let Some((host, port)) = srv(data) else {
                continue;
            };
            for (address_name, kind, data) in &self.records {
                if address_name != &host {
                    continue;
                }
                let ip = match (*kind, data.as_slice()) {
                    (1, [a, b, c, d]) => IpAddr::from([*a, *b, *c, *d]),
                    (28, value) if value.len() == 16 => {
                        IpAddr::from(<[u8; 16]>::try_from(value).unwrap())
                    }
                    _ => continue,
                };
                if ip.is_unspecified() || ip.is_multicast() || ip.is_loopback() {
                    continue;
                }
                let address = match ip {
                    IpAddr::V6(ip) if ip.is_unicast_link_local() => {
                        if self.interface == 0 {
                            continue;
                        }
                        SocketAddr::V6(SocketAddrV6::new(ip, port, 0, self.interface))
                    }
                    _ => SocketAddr::new(ip, port),
                };
                if out.len() < 64 {
                    out.insert(address);
                }
            }
        }
        out
    }
}

pub(crate) fn item<'a>(
    source: &'static str,
    id: EndpointId,
    services: impl Iterator<Item = &'a ServiceRecords>,
) -> Option<Item> {
    let mut addresses = BTreeSet::new();
    let mut txt = BTreeMap::new();
    for service in services.filter(|s| s.id == id) {
        addresses.extend(service.addresses().into_iter().take(64 - addresses.len()));
        for (name, kind, data) in &service.records {
            if *kind != 16 || name != &service.fullname {
                continue;
            }
            let mut remaining = data.as_slice();
            while let Some((&len, tail)) = remaining.split_first() {
                let len = len as usize;
                if len > tail.len() {
                    break;
                }
                if let Ok(s) = std::str::from_utf8(&tail[..len])
                    && let Some((key, value)) = s.split_once('=')
                {
                    txt.insert(key.to_ascii_lowercase(), value.to_owned());
                }
                remaining = &tail[len..];
            }
        }
    }
    if addresses.is_empty() {
        return None;
    }
    let mut data = EndpointData::from(addresses);
    if let Some(url) = txt.get("relay").and_then(|s| s.parse().ok()) {
        data.add_relay_url(url);
    }
    data.set_user_data(txt.get("user-data").and_then(|s| s.parse().ok()));
    Some(Item::new(EndpointInfo::from_parts(id, data), source, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn name(value: &str) -> Vec<u8> {
        value
            .trim_end_matches('.')
            .split('.')
            .flat_map(|s| std::iter::once(s.len() as u8).chain(s.bytes()))
            .chain([0])
            .collect()
    }
    #[test]
    fn records_aggregate_ports_families_and_scopes_and_remove_without_resurrection() {
        let id = iroh::SecretKey::from_bytes(&[7; 32]).public();
        let full = format!("{}.{SERVICE}.local.", instance(id));
        let mut service = ServiceRecords::new(&instance(id), 12).unwrap();
        for port in [1234u16, 4321] {
            let data = [
                vec![0, 0, 0, 0],
                port.to_be_bytes().to_vec(),
                name("host.local."),
            ]
            .concat();
            service.apply(full.clone(), 33, data, true);
        }
        service.apply("host.local.".into(), 1, vec![192, 0, 2, 1], true);
        let ipv6 = "fe80::42"
            .parse::<std::net::Ipv6Addr>()
            .unwrap()
            .octets()
            .to_vec();
        service.apply("host.local.".into(), 28, ipv6.clone(), true);
        let addresses = item("test", id, [&service].into_iter())
            .unwrap()
            .to_endpoint_addr();
        assert_eq!(addresses.ip_addrs().count(), 4);
        assert!(
            addresses
                .ip_addrs()
                .any(|a| matches!(a,SocketAddr::V6(s) if s.scope_id()==12))
        );
        service.apply("host.local.".into(), 28, ipv6, false);
        assert_eq!(
            item("test", id, [&service].into_iter())
                .unwrap()
                .to_endpoint_addr()
                .ip_addrs()
                .count(),
            2
        );
        assert!(item("test", id, std::iter::empty()).is_none());
    }
    #[test]
    fn advertisement_uses_bound_port_and_encodes_empty_txt() {
        let data = EndpointData::from(BTreeSet::from([
            "192.0.2.1:1234".parse::<SocketAddr>().unwrap(),
            "[2001:db8::1]:1234".parse().unwrap(),
        ]));
        let ad = Advertisement::from_data(&data, 1234).unwrap();
        assert_eq!(ad.port, 1234);
        assert_eq!(ad.wire_txt(), [0]);
        let data = EndpointData::from(BTreeSet::from([
            "192.0.2.1:1234".parse::<SocketAddr>().unwrap(),
            "[2001:db8::1]:4321".parse().unwrap(),
        ]));
        assert_eq!(Advertisement::from_data(&data, 1234).unwrap().port, 1234);
        let empty = EndpointData::from(BTreeSet::<SocketAddr>::new());
        assert!(Advertisement::from_data(&empty, 1234).is_none());
    }
    #[test]
    fn endpoint_and_wire_names_are_strict_and_bounded() {
        let id = iroh::SecretKey::from_bytes(&[8; 32]).public();
        assert_eq!(endpoint(&instance(id)), Some(id));
        assert!(endpoint(&(instance(id) + " (2)")).is_none());
        assert!(dns_name(&[0xc0, 0]).is_none());
        assert!(dns_name(&[64; 128]).is_none());
        assert!(srv(&[0; 7]).is_none());
    }
}
