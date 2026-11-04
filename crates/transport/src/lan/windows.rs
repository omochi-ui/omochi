//! Windows DNS-SD, provided by Dnscache/DNSapi (no Bonjour installation).
//! Browse callbacks carry complete RR sets. Each interface has its own browser,
//! preserving IPv6 scopes. Callback storage survives cancellation until the final
//! ERROR_CANCELLED callback, as required by DnsServiceBrowseCancel.
use super::{
    Event, MAX_RECORDS, MAX_SERVICES, SERVICE,
    records::{self, Advertisement},
};
use anyhow::{Context as _, Result, bail, ensure};
use iroh::EndpointId;
use std::{
    cell::UnsafeCell,
    collections::{BTreeMap, HashMap},
    ffi::c_void,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self as channel, SyncSender},
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use windows_sys::Win32::{
    NetworkManagement::{Dns::*, IpHelper::*, Ndis::IfOperStatusUp},
    System::SystemInformation::{ComputerNameDnsHostname, GetComputerNameExW},
};

const PENDING: u32 = 9506;
const CANCELLED: u32 = 1223;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Record {
    name: String,
    kind: u16,
    data: Vec<u8>,
}
enum Command {
    Records(u32, Vec<(Record, u32)>),
    Failed(u32),
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}
unsafe fn string(p: *const u16) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let mut len = 0;
    while len <= 255 && unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    if len > 255 {
        return None;
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(p, len) }).ok()
}
fn wire_name(name: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for label in name.trim_end_matches('.').split('.') {
        if label.is_empty() || label.len() > 63 || label.contains('\0') {
            return None;
        }
        out.push(label.len() as u8);
        out.extend(label.to_ascii_lowercase().bytes());
    }
    out.push(0);
    (out.len() <= 255).then_some(out)
}
unsafe fn copy_records(mut p: *const DNS_RECORDW) -> Option<Vec<(Record, u32)>> {
    let mut out = Vec::new();
    let mut count = 0;
    while !p.is_null() {
        count += 1;
        if count > MAX_RECORDS {
            return None;
        }
        let rr = unsafe { &*p };
        if let Some(name) = unsafe { string(rr.pName) } {
            let data = match rr.wType {
                1 => Some(unsafe { rr.Data.A.IpAddress }.to_ne_bytes().to_vec()),
                28 => Some(unsafe { rr.Data.AAAA.Ip6Address.IP6Byte }.to_vec()),
                12 => unsafe { string(rr.Data.PTR.pNameHost) }.and_then(|s| wire_name(&s)),
                33 => unsafe { string(rr.Data.SRV.pNameTarget) }
                    .and_then(|s| wire_name(&s))
                    .map(|host| {
                        let srv = unsafe { rr.Data.SRV };
                        [
                            srv.wPriority.to_be_bytes().as_slice(),
                            srv.wWeight.to_be_bytes().as_slice(),
                            srv.wPort.to_be_bytes().as_slice(),
                            &host,
                        ]
                        .concat()
                    }),
                16 => {
                    let txt = unsafe { &rr.Data.TXT };
                    if txt.dwStringCount > 64 {
                        return None;
                    }
                    let mut data = Vec::new();
                    for i in 0..txt.dwStringCount as usize {
                        let s = unsafe { string(*txt.pStringArray.as_ptr().add(i)) }?;
                        if s.len() > 255 || data.len() + s.len() + 1 > 1024 {
                            return None;
                        }
                        data.push(s.len() as u8);
                        data.extend(s.bytes());
                    }
                    Some(data)
                }
                _ => None,
            };
            if let Some(data) = data {
                out.push((
                    Record {
                        name: format!("{}.", name.trim_end_matches('.').to_ascii_lowercase()),
                        kind: rr.wType,
                        data,
                    },
                    rr.dwTtl,
                ));
            }
        }
        p = rr.pNext;
    }
    Some(out)
}

struct BrowseContext {
    interface: u32,
    queue: SyncSender<Command>,
    overflow: Arc<AtomicBool>,
    stop: CancellationToken,
    _name: Vec<u16>,
    native: UnsafeCell<(DNS_SERVICE_BROWSE_REQUEST, DNS_SERVICE_CANCEL)>,
}
// Native state is initialized before Browse, then accessed only by DNSapi and
// the owner's cancellation call. Callbacks access only the immutable fields.
unsafe impl Send for BrowseContext {}
unsafe impl Sync for BrowseContext {}
unsafe extern "system" fn browsed(
    status: u32,
    context: *const c_void,
    records: *const DNS_RECORDW,
) {
    let ctx = unsafe { &*context.cast::<BrowseContext>() };
    if !ctx.stop.is_cancelled() && status != CANCELLED {
        let command = if status == 0 {
            unsafe { copy_records(records) }.map(|rr| Command::Records(ctx.interface, rr))
        } else {
            Some(Command::Failed(status))
        };
        if command.is_none_or(|command| ctx.queue.try_send(command).is_err()) {
            ctx.overflow.store(true, Ordering::Release);
        }
    }
    if !records.is_null() {
        unsafe {
            DnsFree(records.cast_mut().cast(), DnsFreeRecordList);
        }
    }
    if status == CANCELLED {
        // Release the OS-owned Arc only on its documented final callback.
        drop(unsafe { Arc::from_raw(context.cast::<BrowseContext>()) });
    }
}
struct Browser(Arc<BrowseContext>);
impl Browser {
    fn new(
        interface: u32,
        queue: SyncSender<Command>,
        overflow: Arc<AtomicBool>,
        stop: CancellationToken,
    ) -> Result<Self> {
        let name = wide(&format!("{SERVICE}.local"));
        let ctx = Arc::new(BrowseContext {
            interface,
            queue,
            overflow,
            stop,
            _name: name,
            native: UnsafeCell::new(Default::default()),
        });
        let os = Arc::into_raw(ctx.clone());
        let (request, cancel) = unsafe { &mut *ctx.native.get() };
        *request = DNS_SERVICE_BROWSE_REQUEST {
            Version: 1,
            InterfaceIndex: interface,
            QueryName: ctx._name.as_ptr(),
            Anonymous: DNS_SERVICE_BROWSE_REQUEST_0 {
                pBrowseCallback: Some(browsed),
            },
            pQueryContext: os.cast_mut().cast(),
        };
        let status = unsafe { DnsServiceBrowse(request, cancel) } as u32;
        if status != PENDING {
            drop(unsafe { Arc::from_raw(os) });
            bail!("DnsServiceBrowse status {status}")
        }
        Ok(Self(ctx))
    }
}
impl Drop for Browser {
    fn drop(&mut self) {
        self.0.stop.cancel();
        // Keep our Arc alive throughout the call, even if the final callback is
        // synchronous. The OS Arc retains request/name/cancel storage afterward.
        let status = unsafe { DnsServiceBrowseCancel(&(*self.0.native.get()).1) };
        if status != 0 {
            tracing::warn!(status, "DNS-SD browse cancellation failed");
        }
    }
}

struct Instance(*const DNS_SERVICE_INSTANCE);
impl Drop for Instance {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { DnsServiceFreeInstance(self.0) }
        }
    }
}
struct Registration {
    instance: Instance,
}
struct Completion {
    sender: channel::Sender<(u32, usize)>,
}
unsafe extern "system" fn registered(
    status: u32,
    context: *const c_void,
    instance: *const DNS_SERVICE_INSTANCE,
) {
    let context = unsafe { Box::from_raw(context.cast_mut().cast::<Completion>()) };
    if let Err(channel::SendError((_, instance))) = context.sender.send((status, instance as usize))
        && instance != 0
    {
        unsafe { DnsServiceFreeInstance(instance as *const _) }
    }
}
fn registration_call(instance: &Instance, remove: bool) -> Result<Instance> {
    let (sender, receiver) = channel::channel();
    let context = Box::into_raw(Box::new(Completion { sender }));
    let request = DNS_SERVICE_REGISTER_REQUEST {
        Version: 1,
        pServiceInstance: instance.0.cast_mut(),
        pRegisterCompletionCallback: Some(registered),
        pQueryContext: context.cast(),
        ..Default::default()
    };
    let status = unsafe {
        if remove {
            DnsServiceDeRegister(&request, ptr::null_mut())
        } else {
            DnsServiceRegister(&request, ptr::null_mut())
        }
    };
    if status != PENDING {
        drop(unsafe { Box::from_raw(context) });
        bail!("DNS-SD registration status {status}")
    }
    // This owned worker retains the request and input instance until completion,
    // including during endpoint shutdown. No async-runtime thread is blocked.
    let (status, instance) = receiver.recv()?;
    let result = Instance(instance as *const _);
    ensure!(status == 0, "DNS-SD completion status {status}");
    Ok(result)
}
impl Registration {
    fn new(own: EndpointId, ad: &Advertisement) -> Result<Self> {
        let mut host = [0u16; 256];
        let mut len = host.len() as u32;
        ensure!(
            unsafe { GetComputerNameExW(ComputerNameDnsHostname, host.as_mut_ptr(), &mut len) }
                != 0,
            "GetComputerNameExW failed"
        );
        let host = wide(&format!(
            "{}.local",
            String::from_utf16(&host[..len as usize])?
        ));
        let expected = format!("{}.{SERVICE}.local", records::instance(own));
        let name = wide(&expected);
        let properties: Vec<_> = ad
            .txt
            .iter()
            .filter_map(|s| std::str::from_utf8(s).ok()?.split_once('='))
            .map(|(k, v)| (wide(k), wide(v)))
            .collect();
        let keys: Vec<_> = properties.iter().map(|(k, _)| k.as_ptr()).collect();
        let values: Vec<_> = properties.iter().map(|(_, v)| v.as_ptr()).collect();
        let input = Instance(unsafe {
            DnsServiceConstructInstance(
                name.as_ptr(),
                host.as_ptr(),
                ptr::null(),
                ptr::null(),
                ad.port,
                0,
                0,
                properties.len() as u32,
                keys.as_ptr(),
                values.as_ptr(),
            )
        });
        ensure!(!input.0.is_null(), "DnsServiceConstructInstance failed");
        let instance = registration_call(&input, false)?;
        ensure!(
            !instance.0.is_null(),
            "DNS-SD returned no registered instance"
        );
        let registration = Self { instance };
        let actual = unsafe { string((*registration.instance.0).pszInstanceName) }
            .context("Invalid registered name")?;
        ensure!(
            actual.trim_end_matches('.').eq_ignore_ascii_case(&expected),
            "DNS-SD renamed EndpointId; registration withdrawn"
        );
        Ok(registration)
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        if let Err(error) = registration_call(&self.instance, true) {
            tracing::warn!(%error,"DNS-SD withdrawal failed");
        }
    }
}

// Include unicast addresses in the signature so DHCP/interface changes rebuild
// browsers and registrations even when the interface indices stay the same.
fn interfaces() -> Result<BTreeMap<u32, Vec<Vec<u8>>>> {
    let mut size = 16384u32;
    for _ in 0..3 {
        ensure!(
            size <= 1024 * 1024,
            "Windows adapter list exceeded capacity"
        );
        // u64 storage provides the alignment required by IP_ADAPTER_ADDRESSES.
        let mut storage = vec![0u64; (size as usize).div_ceil(8)];
        let first = storage.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        let status = unsafe {
            GetAdaptersAddresses(
                0,
                GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER,
                ptr::null(),
                first,
                &mut size,
            )
        };
        if status == 111 {
            continue;
        }
        ensure!(status == 0, "GetAdaptersAddresses status {status}");
        let mut out = BTreeMap::new();
        let mut cursor = first;
        while !cursor.is_null() && out.len() < 64 {
            let ad = unsafe { &*cursor };
            if ad.OperStatus == IfOperStatusUp && ad.IfType != 24 {
                let index = unsafe { ad.Anonymous1.Anonymous.IfIndex };
                let index = if index == 0 { ad.Ipv6IfIndex } else { index };
                let mut addresses = Vec::new();
                let mut unicast = ad.FirstUnicastAddress;
                while !unicast.is_null() && addresses.len() < 64 {
                    let a = unsafe { &*unicast };
                    let sa = a.Address;
                    if !sa.lpSockaddr.is_null() && (1..=128).contains(&sa.iSockaddrLength) {
                        addresses.push(
                            unsafe {
                                std::slice::from_raw_parts(
                                    sa.lpSockaddr.cast::<u8>(),
                                    sa.iSockaddrLength as usize,
                                )
                            }
                            .to_vec(),
                        );
                    }
                    unicast = a.Next;
                }
                addresses.sort();
                if index != 0 && !addresses.is_empty() {
                    out.insert(index, addresses);
                }
            }
            cursor = ad.Next;
        }
        return Ok(out);
    }
    bail!("Windows adapter list kept changing")
}

pub(super) fn start(
    own: EndpointId,
    events: mpsc::Sender<Event>,
    mut updates: watch::Receiver<Option<Advertisement>>,
    cancel: CancellationToken,
) -> Result<()> {
    std::thread::Builder::new()
        .name("omochi-windows-dnssd".into())
        .spawn(move || {
            let mut serial = 0;
            while !cancel.is_cancelled() {
                if let Err(error) = run(own, &events, &mut updates, &cancel, &mut serial) {
                    let _ = events.blocking_send(Event::Reset(format!("Windows DNS-SD: {error}")));
                }
                for _ in 0..100 {
                    if cancel.is_cancelled() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        })?;
    Ok(())
}
fn run(
    own: EndpointId,
    events: &mpsc::Sender<Event>,
    updates: &mut watch::Receiver<Option<Advertisement>>,
    cancel: &CancellationToken,
    serial: &mut u64,
) -> Result<()> {
    let (sender, receiver) = channel::sync_channel(256);
    let overflow = Arc::new(AtomicBool::new(false));
    let network = interfaces()?;
    let _browsers: Vec<_> = network
        .keys()
        .map(|i| Browser::new(*i, sender.clone(), overflow.clone(), cancel.child_token()))
        .collect::<Result<_>>()?;
    let mut registration = None;
    let mut cache = HashMap::<(u32, Record), Instant>::new();
    let mut services = HashMap::<(u32, String), u64>::new();
    let mut check = Instant::now();
    updates.mark_changed();
    events.blocking_send(Event::Ready)?;
    while !cancel.is_cancelled() {
        if updates.has_changed()? {
            registration.take();
            if let Some(ad) = updates.borrow_and_update().clone() {
                registration = Some(Registration::new(own, &ad)?);
            }
        }
        if check.elapsed() > Duration::from_secs(3) {
            if interfaces()? != network {
                bail!("Network interfaces changed")
            }
            check = Instant::now();
        }
        ensure!(
            !overflow.load(Ordering::Acquire),
            "Windows DNS-SD event queue exceeded capacity"
        );
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(Command::Failed(status)) => bail!("DnsServiceBrowse callback status {status}"),
            Ok(Command::Records(interface, records)) => {
                for (record, ttl) in records {
                    let key = (interface, record.clone());
                    if ttl == 0 {
                        cache.remove(&key);
                    } else if cache.len() < MAX_SERVICES * MAX_RECORDS || cache.contains_key(&key) {
                        cache.insert(
                            key,
                            Instant::now() + Duration::from_secs(ttl.min(4500) as u64),
                        );
                    } else {
                        bail!("Windows DNS-SD cache exceeded capacity")
                    }
                    if record.kind == 33 || record.kind == 12 {
                        let name = if record.kind == 12 {
                            records::dns_name(&record.data)
                        } else {
                            Some(record.name.clone())
                        };
                        if let Some(name) =
                            name.filter(|s| records::endpoint(s).is_some_and(|id| id != own))
                        {
                            let key = (interface, name.clone());
                            if ttl == 0 && record.kind == 12 {
                                if let Some(token) = services.remove(&key) {
                                    events.blocking_send(Event::Removed(token))?;
                                }
                                cache.retain(|(i, rr), _| *i != interface || rr.name != name);
                            }
                            if ttl > 0
                                && !services.contains_key(&key)
                                && services.len() < MAX_SERVICES
                            {
                                *serial = serial
                                    .checked_add(1)
                                    .context("Windows DNS-SD generation exhausted")?;
                                events.blocking_send(Event::Found {
                                    token: *serial,
                                    instance: name,
                                    interface,
                                })?;
                                // Initial records can precede PTR/SRV in the same RR set.
                                for ((_, rr), _) in
                                    cache.iter().filter(|((i, _), _)| *i == interface)
                                {
                                    events.blocking_send(Event::Record {
                                        token: *serial,
                                        name: rr.name.clone(),
                                        kind: rr.kind,
                                        data: rr.data.clone(),
                                        add: true,
                                    })?;
                                }
                                services.insert(key, *serial);
                            }
                        }
                    }
                    for ((i, _), token) in &services {
                        if *i == interface {
                            events.blocking_send(Event::Record {
                                token: *token,
                                name: record.name.clone(),
                                kind: record.kind,
                                data: record.data.clone(),
                                add: ttl > 0,
                            })?;
                        }
                    }
                }
            }
            Err(channel::RecvTimeoutError::Timeout) => {}
            Err(error) => return Err(error.into()),
        }
        let now = Instant::now();
        let expired: Vec<_> = cache
            .iter()
            .filter(|(_, until)| **until <= now)
            .map(|(key, _)| key.clone())
            .collect();
        for (interface, record) in expired {
            cache.remove(&(interface, record.clone()));
            for ((i, _), token) in &services {
                if *i == interface {
                    events.blocking_send(Event::Record {
                        token: *token,
                        name: record.name.clone(),
                        kind: record.kind,
                        data: record.data.clone(),
                        add: false,
                    })?;
                }
            }
        }
        let gone: Vec<_> = services
            .keys()
            .filter(|(interface, name)| {
                !cache.keys().any(|(i, rr)| {
                    i == interface
                        && ((rr.kind == 33 && &rr.name == name)
                            || (rr.kind == 12
                                && records::dns_name(&rr.data).as_ref() == Some(name)))
                })
            })
            .cloned()
            .collect();
        for key in gone {
            events.blocking_send(Event::Removed(services.remove(&key).unwrap()))?;
        }
    }
    Ok(())
}
