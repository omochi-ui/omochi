use super::{
    Event, MAX_SERVICES, MAX_TARGETS, SERVICE,
    records::{self, Advertisement},
};
use anyhow::{Context, Result, bail};
use iroh::EndpointId;
use n0_future::StreamExt;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use zbus::{Connection, Proxy, zvariant::OwnedObjectPath};

const DEST: &str = "org.freedesktop.Avahi";

pub(super) async fn connect() -> Option<Connection> {
    tokio::time::timeout(Duration::from_millis(750), async {
        let connection = zbus::connection::Builder::system()
            .ok()?
            .method_timeout(Duration::from_secs(2))
            .build()
            .await
            .ok()?;
        let bus = zbus::fdo::DBusProxy::new(&connection).await.ok()?;
        if !bus.name_has_owner(DEST.try_into().ok()?).await.ok()? {
            return None;
        }
        // Server2's Prepare/Start avoids losing the initial cached records.
        let server = Proxy::new(&connection, DEST, "/", "org.freedesktop.Avahi.Server2")
            .await
            .ok()?;
        let _: u32 = server.call("GetAPIVersion", &()).await.ok()?;
        Some(connection)
    })
    .await
    .ok()
    .flatten()
}

async fn object(
    connection: &Connection,
    path: OwnedObjectPath,
    interface: &'static str,
) -> Result<Proxy<'static>> {
    Ok(Proxy::new_owned(connection.clone(), DEST, path, interface).await?)
}

pub(super) fn start(
    own: EndpointId,
    connection: Connection,
    events: mpsc::Sender<Event>,
    mut updates: watch::Receiver<Option<Advertisement>>,
    cancel: CancellationToken,
) {
    tokio::spawn(async move {
        let mut connection = Some(connection);
        let mut serial = 0;
        while !cancel.is_cancelled() {
            if let Some(conn) = connection.take()
                && let Err(error) =
                    run(own, conn, &events, &mut updates, &cancel, &mut serial).await
            {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    result = events.send(Event::Reset(format!("Avahi: {error}"))) => {
                        if result.is_err() { break }
                    }
                }
            }
            tokio::select! {
                _ = cancel.cancelled() => break,
                next = async {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    connect().await
                } => connection = next,
            }
        }
    });
}

async fn run(
    own: EndpointId,
    connection: Connection,
    events: &mpsc::Sender<Event>,
    updates: &mut watch::Receiver<Option<Advertisement>>,
    cancel: &CancellationToken,
    serial: &mut u64,
) -> Result<()> {
    let bus = zbus::fdo::DBusProxy::new(&connection).await?;
    let mut owners = bus.receive_name_owner_changed().await?;
    let server = Proxy::new(&connection, DEST, "/", "org.freedesktop.Avahi.Server2").await?;
    let path: OwnedObjectPath = server
        .call(
            "ServiceBrowserPrepare",
            &(-1i32, -1i32, SERVICE, "local", 0u32),
        )
        .await?;
    let browser = object(&connection, path, "org.freedesktop.Avahi.ServiceBrowser").await?;
    let mut signals = browser.receive_all_signals().await?;
    let path: OwnedObjectPath = server.call("EntryGroupNew", &()).await?;
    let group = object(&connection, path, "org.freedesktop.Avahi.EntryGroup").await?;
    let mut group_signals = group.receive_all_signals().await?;
    let _: () = browser.call("Start", &()).await?;
    let stop = cancel.child_token();
    let mut jobs = JoinSet::new();
    let mut services = HashMap::<(i32, i32, String), (u64, CancellationToken)>::new();
    let mut published: Option<Advertisement> = None;
    // Mark the initial value changed even if publish preceded backend startup.
    updates.mark_changed();
    // Race the entire event loop, including awaits inside select branches.
    // Otherwise backpressure or a D-Bus call can prevent shutdown cleanup.
    let processing = async {
        events.send(Event::Ready).await?;
        loop {
            tokio::select! {
                biased;
                owner = owners.next() => {
                    let owner = owner.context("D-Bus owner stream ended")?;
                    if owner.args()?.name.as_str() == DEST { bail!("Avahi daemon owner changed") }
                }
                change = updates.changed() => {
                    change?;
                    let advertisement = updates.borrow_and_update().clone();
                    if advertisement == published { continue }
                    let _: () = group.call("Reset", &()).await?;
                    if let Some(ad) = &advertisement {
                        let _: () = group.call("AddService", &(-1i32,-1i32,0u32, records::instance(own), SERVICE,"local","",ad.port,&ad.txt)).await?;
                        let _: () = group.call("Commit", &()).await?;
                    }
                    published = advertisement;
                }
                signal = group_signals.next() => {
                    let signal = signal.context("Avahi registration signal stream ended")?;
                    if signal.header().member().map(|s| s.as_str()) == Some("StateChanged") {
                        let (state, _): (i32,String) = signal.body().deserialize()?;
                        if state == 3 || state == 4 { bail!("Avahi registration failed; EndpointId cannot be renamed") }
                    }
                }
                signal = signals.next() => {
                    let signal = signal.context("Avahi browser ended")?;
                    let member = signal.header().member().map(|s| s.as_str().to_owned());
                    if member.as_deref() == Some("Failure") { bail!("Avahi browser failed") }
                    if !matches!(member.as_deref(),Some("ItemNew"|"ItemRemove")) { continue }
                    let (interface, protocol, name, _, _, _): (i32,i32,String,String,String,u32) = signal.body().deserialize()?;
                    if records::endpoint(&name).is_none_or(|id| id==own) { continue }
                    let key = (interface,protocol,name.clone());
                    if member.as_deref()==Some("ItemRemove") {
                        if let Some((token, child))=services.remove(&key) { child.cancel(); events.send(Event::Removed(token)).await?; }
                    } else if !services.contains_key(&key) && services.len()<MAX_SERVICES && jobs.len()<MAX_SERVICES*2 {
                        *serial = serial.checked_add(1).context("Avahi generation exhausted")?;
                        let token = *serial;
                        let child = stop.child_token();
                        events.send(Event::Found {token,instance:name.clone(),interface:interface.max(0) as u32}).await?;
                        jobs.spawn(service(connection.clone(),events.clone(),token,interface,protocol,name,child.clone()));
                        services.insert(key,(token,child));
                    }
                }
                done = jobs.join_next(), if !jobs.is_empty() => { done.unwrap()??; }
            }
        }
    };
    let result: Result<()> = tokio::select! {
        biased;
        _ = cancel.cancelled() => Ok(()),
        result = processing => result,
    };
    stop.cancel();
    // Complete Free calls while the D-Bus connection is still alive. On an
    // abrupt runtime shutdown, connection loss withdraws the whole client.
    while let Some(done) = jobs.join_next().await {
        let _ = done;
    }
    let _: Result<(), _> = group.call("Free", &()).await;
    let _: Result<(), _> = browser.call("Free", &()).await;
    result
}

async fn service(
    connection: Connection,
    events: mpsc::Sender<Event>,
    token: u64,
    interface: i32,
    protocol: i32,
    instance: String,
    cancel: CancellationToken,
) -> Result<()> {
    let (targets, mut discovered) = mpsc::channel::<String>(MAX_TARGETS);
    let mut jobs = JoinSet::new();
    let name = format!("{instance}.{SERVICE}.local.");
    for kind in [16, 33] {
        jobs.spawn(record(
            connection.clone(),
            events.clone(),
            token,
            (interface, protocol),
            (name.clone(), kind),
            targets.clone(),
            cancel.clone(),
        ));
    }
    let mut hosts = HashSet::new();
    let result: Result<()> = async {
        loop { tokio::select! {
            biased;
            _ = cancel.cancelled() => return Ok(()),
            host = discovered.recv() => {
                let host = host.context("Avahi target stream ended")?;
                if hosts.contains(&host) { continue }
                if hosts.len()>=MAX_TARGETS { bail!("Too many Avahi SRV targets") }
                hosts.insert(host.clone());
                for kind in [1,28] {
                    jobs.spawn(record(connection.clone(),events.clone(),token,(interface,protocol),(host.clone(),kind),targets.clone(),cancel.clone()));
                }
            }
            done = jobs.join_next(), if !jobs.is_empty() => { done.unwrap()??; }
        }}
    }.await;
    cancel.cancel();
    while let Some(done) = jobs.join_next().await {
        let _ = done;
    }
    result
}

async fn record(
    connection: Connection,
    events: mpsc::Sender<Event>,
    token: u64,
    link: (i32, i32),
    query: (String, u16),
    targets: mpsc::Sender<String>,
    cancel: CancellationToken,
) -> Result<()> {
    let (name, kind) = query;
    // Keep each service browser's transport family. An all-family record
    // browser can report the same RR twice and remove only one copy later.
    let (interface, protocol) = link;
    let server = Proxy::new(&connection, DEST, "/", "org.freedesktop.Avahi.Server2").await?;
    let path: OwnedObjectPath = server
        .call(
            "RecordBrowserPrepare",
            &(interface, protocol, &name, 1u16, kind, 0u32),
        )
        .await?;
    let browser = object(&connection, path, "org.freedesktop.Avahi.RecordBrowser").await?;
    let signals = browser.receive_all_signals().await?;
    let _: () = browser.call("Start", &()).await?;
    let result = receive_records(signals, &events, token, &targets, &cancel).await;
    let _: Result<(), _> = browser.call("Free", &()).await;
    result
}

async fn receive_records(
    mut signals: impl n0_future::Stream<Item = zbus::Message> + Unpin,
    events: &mpsc::Sender<Event>,
    token: u64,
    targets: &mpsc::Sender<String>,
    cancel: &CancellationToken,
) -> Result<()> {
    // Keep cancellation active while either bounded channel is full. The
    // caller still frees the record browser after this future is cancelled.
    let processing = async {
        loop {
            let signal = signals.next().await;
            let signal = signal.context("Avahi record stream ended")?;
            let member = signal.header().member().map(|s| s.as_str().to_owned());
            if member.as_deref() == Some("Failure") {
                bail!("Avahi record query failed")
            }
            if !matches!(member.as_deref(), Some("ItemNew" | "ItemRemove")) {
                continue;
            }
            let (_, _, name, class, kind, data, _): (i32, i32, String, u16, u16, Vec<u8>, u32) =
                signal.body().deserialize()?;
            if class != 1 || data.len() > 1024 {
                continue;
            }
            let add = member.as_deref() == Some("ItemNew");
            if kind == 33
                && add
                && let Some((host, _)) = records::srv(&data)
            {
                targets.send(host).await?;
            }
            events
                .send(Event::Record {
                    token,
                    name,
                    kind,
                    data,
                    add,
                })
                .await?;
        }
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Ok(()),
        result = processing => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use n0_future::{future::now_or_never, stream};

    fn srv_signal() -> zbus::Message {
        let data = vec![
            0u8, 0, 0, 0, 0x12, 0x34, 4, b'h', b'o', b's', b't', 5, b'l', b'o', b'c', b'a', b'l', 0,
        ];
        zbus::Message::signal("/record", "org.freedesktop.Avahi.RecordBrowser", "ItemNew")
            .unwrap()
            .build(&(
                1i32,
                0i32,
                "peer._irohv1._udp.local.",
                1u16,
                33u16,
                data,
                0u32,
            ))
            .unwrap()
    }

    async fn cancel_blocked_record(full_targets: bool) {
        let (events, mut received) = mpsc::channel(1);
        let (targets, mut discovered) = mpsc::channel(1);
        if full_targets {
            targets.send("occupied.local.".into()).await.unwrap();
        } else {
            events.send(Event::Ready).await.unwrap();
        }
        let cancel = CancellationToken::new();
        let signals = stream::iter([srv_signal()]).chain(stream::pending());
        let worker = receive_records(signals, &events, 7, &targets, &cancel);
        tokio::pin!(worker);
        // Poll through signal decoding until the bounded send is blocked. Keep
        // the receiver alive but undrained, as during service/backend shutdown.
        assert!(now_or_never(worker.as_mut()).is_none());
        cancel.cancel();
        now_or_never(worker.as_mut())
            .expect("cancelled Avahi record worker is still blocked on a full queue")
            .unwrap();
        if full_targets {
            assert_eq!(discovered.try_recv().unwrap(), "occupied.local.");
            assert!(received.try_recv().is_err());
        } else {
            assert_eq!(discovered.try_recv().unwrap(), "host.local.");
            assert!(matches!(received.try_recv().unwrap(), Event::Ready));
        }
        assert!(discovered.try_recv().is_err());
        assert!(received.try_recv().is_err());
    }

    #[tokio::test]
    async fn cancellation_interrupts_full_target_queue() {
        cancel_blocked_record(true).await;
    }

    #[tokio::test]
    async fn cancellation_interrupts_full_event_queue() {
        cancel_blocked_record(false).await;
    }

    #[tokio::test]
    async fn records_forward_before_cancellation() {
        let (events, mut received) = mpsc::channel(1);
        let (targets, mut discovered) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let signals = stream::iter([srv_signal()]).chain(stream::pending());
        let worker = receive_records(signals, &events, 7, &targets, &cancel);
        tokio::pin!(worker);
        assert!(now_or_never(worker.as_mut()).is_none());
        assert_eq!(discovered.try_recv().unwrap(), "host.local.");
        assert!(matches!(
            received.try_recv().unwrap(),
            Event::Record {
                token: 7,
                kind: 33,
                add: true,
                ..
            }
        ));
        cancel.cancel();
        now_or_never(worker.as_mut()).unwrap().unwrap();
    }
}
