//! Bonjour callbacks are processed on one owned thread. No raw multicast socket
//! is opened here, including on iOS. DNSServiceRefDeallocate precedes context drop.
use super::{
    Event, MAX_SERVICES, MAX_TARGETS, SERVICE,
    records::{self, Advertisement},
};
use anyhow::{Context as _, Result, bail, ensure};
use iroh::EndpointId;
use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, CString, c_char, c_void},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{SyncSender, sync_channel},
    },
    time::Duration,
};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

type Ref = *mut c_void;
type BrowseReply = unsafe extern "C" fn(
    Ref,
    u32,
    u32,
    i32,
    *const c_char,
    *const c_char,
    *const c_char,
    *mut c_void,
);
type QueryReply = unsafe extern "C" fn(
    Ref,
    u32,
    u32,
    i32,
    *const c_char,
    u16,
    u16,
    u16,
    *const c_void,
    u32,
    *mut c_void,
);
type RegisterReply =
    unsafe extern "C" fn(Ref, u32, i32, *const c_char, *const c_char, *const c_char, *mut c_void);
#[link(name = "System")]
unsafe extern "C" {
    fn DNSServiceBrowse(
        out: *mut Ref,
        flags: u32,
        interface: u32,
        kind: *const c_char,
        domain: *const c_char,
        callback: BrowseReply,
        context: *mut c_void,
    ) -> i32;
    fn DNSServiceQueryRecord(
        out: *mut Ref,
        flags: u32,
        interface: u32,
        name: *const c_char,
        kind: u16,
        class: u16,
        callback: QueryReply,
        context: *mut c_void,
    ) -> i32;
    fn DNSServiceRegister(
        out: *mut Ref,
        flags: u32,
        interface: u32,
        name: *const c_char,
        kind: *const c_char,
        domain: *const c_char,
        host: *const c_char,
        port: u16,
        txt_len: u16,
        txt: *const c_void,
        callback: RegisterReply,
        context: *mut c_void,
    ) -> i32;
    fn DNSServiceRefSockFD(reference: Ref) -> i32;
    fn DNSServiceProcessResult(reference: Ref) -> i32;
    fn DNSServiceRefDeallocate(reference: Ref);
}

enum Command {
    Service(u32, String, bool),
    Record(u64, String, u16, Vec<u8>, bool),
    Failed(i32),
}
struct Context {
    token: u64,
    queue: SyncSender<Command>,
    overflow: Arc<AtomicBool>,
    cancel: CancellationToken,
}
impl Context {
    fn send(&self, command: Command) {
        if !self.cancel.is_cancelled() && self.queue.try_send(command).is_err() {
            self.overflow.store(true, Ordering::Release);
        }
    }
}
unsafe fn text<'a>(value: *const c_char) -> Option<&'a str> {
    if value.is_null() {
        return None;
    }
    let value = unsafe { CStr::from_ptr(value) };
    (value.to_bytes().len() <= 255)
        .then(|| value.to_str().ok())
        .flatten()
}
unsafe extern "C" fn browse(
    _: Ref,
    flags: u32,
    index: u32,
    error: i32,
    name: *const c_char,
    _: *const c_char,
    _: *const c_char,
    context: *mut c_void,
) {
    let context = unsafe { &*context.cast::<Context>() };
    if error != 0 {
        context.send(Command::Failed(error));
        return;
    }
    if let Some(name) = unsafe { text(name) } {
        context.send(Command::Service(index, name.to_owned(), flags & 2 != 0));
    }
}
unsafe extern "C" fn query(
    _: Ref,
    flags: u32,
    _: u32,
    error: i32,
    name: *const c_char,
    kind: u16,
    class: u16,
    len: u16,
    data: *const c_void,
    ttl: u32,
    context: *mut c_void,
) {
    let context = unsafe { &*context.cast::<Context>() };
    if error != 0 {
        context.send(Command::Failed(error));
        return;
    }
    if class != 1 || len > 1024 || (len > 0 && data.is_null()) {
        return;
    }
    if let Some(name) = unsafe { text(name) } {
        let data = if len == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len as usize) }.to_vec()
        };
        context.send(Command::Record(
            context.token,
            name.to_owned(),
            kind,
            data,
            flags & 2 != 0 && ttl > 0,
        ));
    }
}
unsafe extern "C" fn registered(
    _: Ref,
    _: u32,
    error: i32,
    _: *const c_char,
    _: *const c_char,
    _: *const c_char,
    context: *mut c_void,
) {
    if error != 0 {
        unsafe { &*context.cast::<Context>() }.send(Command::Failed(error));
    }
}

struct Operation {
    reference: Ref,
    _context: Box<Context>,
}
impl Drop for Operation {
    fn drop(&mut self) {
        unsafe {
            DNSServiceRefDeallocate(self.reference);
        }
    }
}
struct Service {
    token: u64,
    interface: u32,
    operations: Vec<Operation>,
    hosts: HashSet<String>,
}

pub(super) fn start(
    own: EndpointId,
    events: mpsc::Sender<Event>,
    mut updates: watch::Receiver<Option<Advertisement>>,
    cancel: CancellationToken,
) -> Result<()> {
    std::thread::Builder::new()
        .name("omochi-bonjour".into())
        .spawn(move || {
            let mut serial = 0;
            while !cancel.is_cancelled() {
                if let Err(error) = run(own, &events, &mut updates, &cancel, &mut serial) {
                    let _ = events.blocking_send(Event::Reset(format!("Bonjour: {error}")));
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
    let (commands, received) = sync_channel(256);
    let overflow = Arc::new(AtomicBool::new(false));
    let context = |token| {
        Box::new(Context {
            token,
            queue: commands.clone(),
            overflow: overflow.clone(),
            cancel: cancel.clone(),
        })
    };
    let operation = |mut context: Box<Context>,
                     start: &dyn Fn(*mut Ref, *mut c_void) -> i32|
     -> Result<Operation> {
        let mut reference = std::ptr::null_mut();
        let status = start(&mut reference, (&mut *context as *mut Context).cast());
        ensure!(status == 0, "DNS-SD status {status}");
        Ok(Operation {
            reference,
            _context: context,
        })
    };
    let kind = CString::new(SERVICE)?;
    let domain = c"local.";
    let browser = operation(context(0), &|out, ctx| unsafe {
        DNSServiceBrowse(out, 0, 0, kind.as_ptr(), domain.as_ptr(), browse, ctx)
    })?;
    let mut registration: Option<Operation> = None;
    let mut services = HashMap::<(u32, String), Service>::new();
    updates.mark_changed();
    events.blocking_send(Event::Ready)?;
    while !cancel.is_cancelled() {
        if updates.has_changed()? {
            let ad = updates.borrow_and_update().clone();
            registration.take();
            if let Some(ad) = ad {
                let name = CString::new(records::instance(own))?;
                let txt = ad.wire_txt();
                registration = Some(operation(context(0), &|out, ctx| unsafe {
                    DNSServiceRegister(
                        out,
                        8,
                        0,
                        name.as_ptr(),
                        kind.as_ptr(),
                        domain.as_ptr(),
                        std::ptr::null(),
                        ad.port.to_be(),
                        txt.len() as u16,
                        txt.as_ptr().cast(),
                        registered,
                        ctx,
                    )
                })?);
            }
        }
        let references: Vec<_> = std::iter::once(browser.reference)
            .chain(registration.iter().map(|o| o.reference))
            .chain(
                services
                    .values()
                    .flat_map(|s| s.operations.iter().map(|o| o.reference)),
            )
            .collect();
        let mut fds: Vec<_> = references
            .iter()
            .map(|r| libc::pollfd {
                fd: unsafe { DNSServiceRefSockFD(*r) },
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();
        let status = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 25) };
        if status < 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(std::io::Error::last_os_error().into());
        }
        ensure!(
            !fds.iter()
                .any(|fd| fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0),
            "Bonjour connection ended"
        );
        for (fd, reference) in fds.iter().zip(references) {
            if cancel.is_cancelled() {
                return Ok(());
            }
            if fd.revents & libc::POLLIN != 0 {
                let status = unsafe { DNSServiceProcessResult(reference) };
                ensure!(status == 0, "DNS-SD processing status {status}");
            }
        }
        ensure!(
            !overflow.load(Ordering::Acquire),
            "Bonjour event queue exceeded capacity"
        );
        while let Ok(command) = received.try_recv() {
            if cancel.is_cancelled() {
                return Ok(());
            }
            match command {
                Command::Failed(status) => bail!("DNS-SD callback status {status}"),
                Command::Service(interface, name, add) => {
                    if records::endpoint(&name).is_none_or(|id| id == own) {
                        continue;
                    }
                    let key = (interface, name.clone());
                    if !add {
                        if let Some(service) = services.remove(&key) {
                            events.blocking_send(Event::Removed(service.token))?;
                        }
                    } else if !services.contains_key(&key) && services.len() < MAX_SERVICES {
                        *serial = serial
                            .checked_add(1)
                            .context("Bonjour generation exhausted")?;
                        let token = *serial;
                        let fullname = CString::new(format!("{name}.{SERVICE}.local."))?;
                        let mut operations = Vec::new();
                        for kind in [16, 33] {
                            operations.push(operation(context(token), &|out, ctx| unsafe {
                                DNSServiceQueryRecord(
                                    out,
                                    0,
                                    interface,
                                    fullname.as_ptr(),
                                    kind,
                                    1,
                                    query,
                                    ctx,
                                )
                            })?);
                        }
                        events.blocking_send(Event::Found {
                            token,
                            instance: name,
                            interface,
                        })?;
                        services.insert(
                            key,
                            Service {
                                token,
                                interface,
                                operations,
                                hosts: HashSet::new(),
                            },
                        );
                    }
                }
                Command::Record(token, name, kind, data, add) => {
                    let Some(service) = services.values_mut().find(|s| s.token == token) else {
                        continue;
                    };
                    if kind == 33
                        && add
                        && let Some((host, _)) = records::srv(&data)
                        && !service.hosts.contains(&host)
                    {
                        ensure!(
                            service.hosts.len() < MAX_TARGETS,
                            "Too many Bonjour SRV targets"
                        );
                        service.hosts.insert(host.clone());
                        let host = CString::new(host)?;
                        for kind in [1, 28] {
                            service.operations.push(operation(
                                context(token),
                                &|out, ctx| unsafe {
                                    DNSServiceQueryRecord(
                                        out,
                                        0,
                                        service.interface,
                                        host.as_ptr(),
                                        kind,
                                        1,
                                        query,
                                        ctx,
                                    )
                                },
                            )?);
                        }
                    }
                    events.blocking_send(Event::Record {
                        token,
                        name,
                        kind,
                        data,
                        add,
                    })?;
                }
            }
        }
    }
    Ok(())
}
