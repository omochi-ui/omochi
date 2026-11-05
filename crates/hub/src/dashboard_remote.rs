//! Authenticated downstream browser clients. Gateway trust and protocols are unchanged.
use crate::{
    Hub,
    dashboards::{Dashboard, DashboardStore},
    scoped::SurfaceHandle,
    sharing::{Grant, ShareStore},
};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use omochi_protocol::{read_frame, read_frame_limited, write_frame};
use omochi_transport::{Endpoint, EndpointConnection};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{Mutex, Semaphore},
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

pub const ALPN: &[u8] = b"omochi/dashboard/2";
pub const OWNER_ALPN: &[u8] = b"omochi/owner/1";
pub const OWNER_ENROLL_ALPN: &[u8] = b"omochi/owner-enroll/1";
const IO_TIMEOUT: Duration = Duration::from_secs(10);
// Bound active sessions and unauthenticated handshakes across all downstream roles.
const MAX_CONNECTIONS: usize = 128;
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Hello {
    version: u32,
    grant_id: String,
    secret: Option<String>,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
enum Request {
    Ping,
    Control { command: crate::control::Command },
    Action { lease: String, message: Value },
    Layout { value: Value },
    Image { lease: String, uri: String },
}
struct View {
    handle: SurfaceHandle,
    surface: String,
}
struct Session {
    hub: Hub,
    boards: DashboardStore,
    shares: ShareStore,
    peer: String,
    hello: Hello,
    board: Option<Dashboard>,
    owner: bool,
    views: Mutex<BTreeMap<String, View>>,
    alive: Mutex<Instant>,
}
fn error(code: &str) -> Value {
    json!({"kind":"error","code":code})
}
fn api_error(e: crate::dashboards::ApiError) -> anyhow::Error {
    anyhow::anyhow!("shared access error ({})", e.0)
}
impl Session {
    async fn authorize(&self) -> Result<Option<(Grant, Dashboard)>> {
        if self.owner {
            let owners = self.boards.owners();
            let id = self.hello.grant_id.clone();
            let peer = self.peer.clone();
            omochi_transport::blocking::run(move || owners.authorize(&id, &peer, || Ok(())))
                .await?
                .map_err(api_error)?;
            return Ok(None);
        }
        let shares = self.shares.clone();
        let boards = self.boards.clone();
        let id = self.hello.grant_id.clone();
        let secret = self.hello.secret.clone();
        let peer = self.peer.clone();
        omochi_transport::blocking::run(move || {
            shares
                .authorize(&boards, &id, &peer, secret.as_deref())
                .map(Some)
        })
        .await?
        .map_err(api_error)
    }
    async fn view(&self, lease: &str, interact: bool) -> Result<SurfaceHandle> {
        let access = self.authorize().await?;
        if interact
            && access
                .as_ref()
                .is_some_and(|(grant, _)| !grant.permissions.interact)
        {
            bail!("Not authorized");
        }
        let views = self.views.lock().await;
        let view = views.get(lease).context("Expired view")?;
        if access
            .as_ref()
            .is_some_and(|(grant, board)| !grant.allows_surface(board, &view.surface))
            || view.handle.is_cancelled()
        {
            bail!("Expired view");
        }
        Ok(view.handle.clone())
    }
}

impl Hub {
    /// Configure only the static UI address; it carries no grant or invitation.
    pub fn set_dashboard_host(&self, endpoint: Endpoint, web_url: String) -> Result<()> {
        let url = url::Url::parse(&web_url)?;
        if web_url.len() > 2048
            || url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("Invalid dashboard HTTPS base");
        }
        *self.dashboard_host.write().unwrap() = Some((endpoint, web_url));
        Ok(())
    }
    pub(crate) fn dashboard_host(&self) -> Option<Value> {
        self.dashboard_host.read().unwrap().as_ref().map(|(endpoint,web_url)|json!({"hubId":endpoint.id().to_string(),"relayUrls":endpoint.addr().relay_urls().map(ToString::to_string).collect::<Vec<_>>(),"webUrl":web_url}))
    }
    pub(crate) fn share_link(&self, grant: &Grant, secret: &str) -> Option<String> {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let info = self.dashboard_host()?;
        if info["relayUrls"].as_array()?.is_empty() {
            return None;
        }
        let payload = json!({"version":2,"hubId":info["hubId"],"relayUrls":info["relayUrls"],"grantId":grant.id,"mode":"share","secret":secret});
        Some(format!(
            "{}#share:{}",
            info["webUrl"].as_str()?,
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).ok()?)
        ))
    }
}

/// Start before exposing shares. A process lease is registered before any admission.
/// On every grant generation change, old sessions are joined before acknowledgement.
pub fn listen(
    hub: Hub,
    boards: DashboardStore,
    endpoint: Endpoint,
) -> Result<JoinHandle<Result<()>>> {
    let shares = boards.shares();

    endpoint.set_alpns(vec![
        ALPN.to_vec(),
        OWNER_ALPN.to_vec(),
        OWNER_ENROLL_ALPN.to_vec(),
    ]);
    let owners = boards.owners();

    Ok(tokio::spawn(async move {
        let copy = shares.clone();
        let lease = Arc::new(
            crate::control::storage(move || crate::sharing_live::Lease::register(&copy))
                .await
                .map_err(api_error)?,
        );
        let copy = owners.clone();
        let owner_lease = Arc::new(
            crate::control::storage(move || crate::owners_live::Lease::register(&copy))
                .await
                .map_err(api_error)?,
        );
        let mut handshakes = JoinSet::new();
        let mut owner_sessions = JoinSet::new();
        let mut owner_generation = {
            let copy = owners.clone();
            crate::control::storage(move || copy.generation())
                .await
                .map_err(api_error)?
        };
        {
            let copy = owner_lease.clone();
            crate::control::storage(move || copy.acknowledge(owner_generation))
                .await
                .map_err(api_error)?;
        }
        let mut owner_epoch = hub.stop.child_token();
        let capacity = Arc::new(Semaphore::new(MAX_CONNECTIONS));
        let mut sessions = JoinSet::new();
        let mut generation = {
            let copy = shares.clone();
            crate::control::storage(move || copy.generation())
                .await
                .map_err(api_error)?
        };
        {
            let copy = lease.clone();
            crate::control::storage(move || copy.acknowledge(generation))
                .await
                .map_err(api_error)?;
        }
        let mut epoch = hub.stop.child_token();
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        let result=async {
            loop { tokio::select! {
                _=hub.stop.cancelled()=>break,
                _=tick.tick()=> {
                    let next_owner={ let copy = owners.clone(); crate::control::storage(move || copy.generation()).await.map_err(api_error)? };
                    if next_owner!=owner_generation { owner_epoch.cancel(); while owner_sessions.join_next().await.is_some() {} omochi_transport::blocking::drain().await; { let copy = owner_lease.clone(); crate::control::storage(move || copy.acknowledge(next_owner)).await.map_err(api_error)?; } owner_generation=next_owner; owner_epoch=hub.stop.child_token(); }
                    let next={ let copy = shares.clone(); crate::control::storage(move || copy.generation()).await.map_err(api_error)? };
                    if next!=generation {
                        epoch.cancel(); while sessions.join_next().await.is_some() {} omochi_transport::blocking::drain().await;
                        { let copy = lease.clone(); crate::control::storage(move || copy.acknowledge(next)).await.map_err(api_error)?; } generation=next;
                        epoch = hub.stop.child_token();
                    }
                },
                Some(_)=sessions.join_next(),if !sessions.is_empty()=>{},
                Some(_)=owner_sessions.join_next(),if !owner_sessions.is_empty()=>{},
                incoming=endpoint.accept()=> {
                    let Some(incoming)=incoming else {break;};
                    let Ok(permit)=capacity.clone().try_acquire_owned() else {incoming.refuse();continue;};
                    handshakes.spawn(async move { tokio::time::timeout(IO_TIMEOUT,incoming).await.map(|result| result.map(|connection| (connection,permit))) });
                },
                Some(result)=handshakes.join_next(),if !handshakes.is_empty()=> {
                    let Ok(Ok(Ok((connection,permit))))=result else {continue;};
                    let owner=connection.alpn()==OWNER_ALPN || connection.alpn()==OWNER_ENROLL_ALPN;
                    let hub=hub.clone(); let boards=boards.clone();
                    let stop=if owner {owner_epoch.child_token()} else {epoch.child_token()};
                    let task=async move {let _permit=permit;let _guard=ConnectionGuard(connection.clone());let _=serve(hub,boards,connection,stop).await;};
                    if owner {owner_sessions.spawn(task);} else {sessions.spawn(task);}

                }
            }}
            Ok::<(),anyhow::Error>(())
        }.await;
        handshakes.abort_all();
        while handshakes.join_next().await.is_some() {}
        owner_epoch.cancel();
        while owner_sessions.join_next().await.is_some() {}
        omochi_transport::blocking::drain().await;
        epoch.cancel();
        while sessions.join_next().await.is_some() {}
        omochi_transport::blocking::drain().await;
        endpoint.close().await;
        result
    }))
}
struct ConnectionGuard(EndpointConnection);
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.close(2u32.into(), b"Session ended");
    }
}

async fn serve(
    hub: Hub,
    boards: DashboardStore,
    connection: EndpointConnection,
    cancelled: CancellationToken,
) -> Result<()> {
    let peer = connection.remote_id().to_string();
    let (mut tx, mut rx) = tokio::select! { _=cancelled.cancelled()=>return Ok(()), result=tokio::time::timeout(IO_TIMEOUT, connection.accept_bi())=>result?? };
    let hello: Hello = tokio::time::timeout(IO_TIMEOUT, read_frame_limited(&mut rx, 16 * 1024))
        .await??
        .context("Missing authentication")?;
    if hello.version != if connection.alpn() == ALPN { 2 } else { 1 }
        || hello.grant_id.len() > 100
        || hello.secret.as_ref().is_some_and(|s| {
            s.len() != 64
                || !s
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
    {
        bail!("Invalid authentication");
    }
    let shares = boards.shares();
    if connection.alpn() == OWNER_ENROLL_ALPN {
        let id = hello.grant_id;
        let secret = hello.secret.context("Missing invitation")?;
        let owners = boards.owners();
        let accepted =
            omochi_transport::blocking::run(move || owners.redeem(&id, &secret, &peer).map(|_| ()))
                .await?
                .is_ok();
        tokio::time::timeout(
            IO_TIMEOUT,
            write_frame(&mut tx, &json!({"accepted":accepted})),
        )
        .await??;
        tx.finish()?;
        let _ = tokio::time::timeout(IO_TIMEOUT, tx.stopped()).await;
        return Ok(());
    }
    if connection.alpn() != ALPN && connection.alpn() != OWNER_ALPN {
        bail!("Wrong protocol");
    }
    let owner = connection.alpn() == OWNER_ALPN;
    let copy = boards.clone();
    let access_shares = shares.clone();
    let auth = hello.clone();
    let who = peer.clone();
    let access = crate::control::storage(move || {
        let boards = copy;
        let shares = access_shares;
        let hello = auth;
        let peer = who;
        if owner {
            boards
                .owners()
                .authorize(&hello.grant_id, &peer, || Ok(()))
                .map(|_| None)
        } else {
            shares
                .authorize(&boards, &hello.grant_id, &peer, hello.secret.as_deref())
                .map(Some)
        }
    })
    .await;
    let access = match access {
        Ok(value) => value,
        Err(e) => {
            connection.close(1u32.into(), b"Not authorized");
            return Err(api_error(e));
        }
    };
    let welcome = if let Some((grant, board)) = &access {
        json!({"kind":"welcome","board":board,"permissions":grant.permissions})
    } else {
        json!({"kind":"ownerWelcome","services":hub.services().await})
    };
    let session = Arc::new(Session {
        hub,
        boards,
        shares,
        peer,
        hello,
        board: access.map(|(_, board)| board),
        owner,
        views: Mutex::new(BTreeMap::new()),
        alive: Mutex::new(Instant::now()),
    });
    tokio::time::timeout(IO_TIMEOUT, write_frame(&mut tx, &welcome)).await??;
    let stop = CancellationToken::new();
    let _guard = stop.clone().drop_guard();
    let copy = session.clone();
    let owner = connection.clone();
    let mut tasks = JoinSet::new();
    let subscription_stop = stop.clone();
    tasks.spawn(async move {
        let _ = deliver(copy, tx, rx, subscription_stop).await;
        owner.close(2u32.into(), b"Subscription ended");
    });
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let concurrency = Arc::new(Semaphore::new(8));
    let result=async {loop {tokio::select! {
        _=connection.closed()=>break,
        _=cancelled.cancelled()=>break,
        _=tick.tick()=> {
            let access=session.authorize().await?;
            if access.as_ref().is_some_and(|(_,board)|Some(board.revision)!=session.board.as_ref().map(|b|b.revision)) || session.alive.lock().await.elapsed()>Duration::from_secs(45) {bail!("Session refresh required");}
        },
        Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},
        stream=connection.accept_bi()=> {
            let (tx,rx)=stream?;
            let Ok(permit)=concurrency.clone().try_acquire_owned() else {bail!("Request capacity reached");};
            let copy=session.clone();let request_stop=stop.clone();tasks.spawn(async move {let _permit=permit;let _=request(copy,tx,rx,request_stop).await;});
        }
    }} Ok::<(),anyhow::Error>(())}.await;
    stop.cancel();
    connection.close(2u32.into(), b"Session ended");
    while tasks.join_next().await.is_some() {}
    result
}

async fn deliver(
    session: Arc<Session>,
    mut tx: omochi_transport::SendStream,
    mut rx: omochi_transport::RecvStream,
    stop: CancellationToken,
) -> Result<()> {
    let mut subscription = session.hub.subscribe_browser().await?;
    loop {
        let batch = tokio::select! { _=stop.cancelled()=>return Ok(()), result=subscription.next_batch()=>result? };
        let access = session.authorize().await?;
        let mut frame = None;
        if let Some(message) = &batch.message {
            let surface = crate::scoped::surface_id(message).context("Missing surface")?;
            if access
                .as_ref()
                .is_none_or(|(grant, board)| grant.allows_surface(board, surface))
            {
                let mut views = session.views.lock().await;
                if message.get("createSurface").is_some() || message.get("deleteSurface").is_some()
                {
                    views.retain(|_, v| v.surface != surface);
                }
                let lease = if message.get("deleteSurface").is_some() {
                    None
                } else if let Some(handle) = batch.handle.clone() {
                    let existing = views
                        .iter()
                        .find(|(_, v)| v.surface == surface && !v.handle.is_cancelled())
                        .map(|(id, _)| id.clone());
                    let id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    views.insert(
                        id.clone(),
                        View {
                            handle,
                            surface: surface.into(),
                        },
                    );
                    Some(id)
                } else {
                    None
                };
                frame = Some(json!({"kind":"message","message":message,"lease":lease}));
            }
        } else if batch.snapshot_end {
            frame = Some(json!({"kind":"snapshotEnd"}));
        }
        if let Some(mut frame) = frame {
            frame["sequence"] = batch.sequence.to_string().into();
            tokio::time::timeout(IO_TIMEOUT, write_frame(&mut tx, &frame)).await??;
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Ack {
                ack: String,
            }
            let ack: Ack = tokio::time::timeout(IO_TIMEOUT, read_frame_limited(&mut rx, 128))
                .await??
                .context("Missing acknowledgement")?;
            if ack.ack != batch.sequence.to_string() {
                bail!("Invalid acknowledgement");
            }
        }
        subscription.acknowledge(batch.sequence)?;
    }
}

fn contains_uri(value: &Value, uri: &str) -> bool {
    match value {
        Value::String(s) => s == uri,
        Value::Array(a) => a.iter().any(|v| contains_uri(v, uri)),
        Value::Object(o) => o.values().any(|v| contains_uri(v, uri)),
        _ => false,
    }
}
async fn published(hub: &Hub, handle: &SurfaceHandle, uri: &str) -> Result<bool> {
    let state = hub.state.lock().await;
    let channel = handle.validate(&state)?;
    Ok(channel.snapshot.messages().iter().any(|message| {
        crate::scoped::surface_id(message) == Some(handle.surface_id())
            && contains_uri(message, uri)
    }))
}
async fn request(
    session: Arc<Session>,
    mut tx: omochi_transport::SendStream,
    mut rx: omochi_transport::RecvStream,
    stop: CancellationToken,
) -> Result<()> {
    let req: Request = tokio::time::timeout(IO_TIMEOUT, read_frame(&mut rx))
        .await??
        .context("Missing request")?;
    let result=async {
        session.authorize().await?;*session.alive.lock().await=Instant::now();
        match req {
            Request::Ping=>Ok(json!({"kind":"ok","services":if session.owner {session.hub.services().await}else{vec![]}})),
            Request::Control{command}=>{
                if !session.owner {bail!("Not authorized");}
                let control=crate::control::HubControl{hub:session.hub.clone(),boards:session.boards.clone()};
                match control.execute_owner(session.hello.grant_id.clone(),session.peer.clone(),command).await {Ok(value)=>Ok(json!({"kind":"result","value":value})),Err(e)=>Ok(json!({"kind":"error","code":if e.0==409 {"Conflict"}else if e.0==404 {"NotFound"}else if e.0==400 {"InvalidBoard"}else{"StorageUnavailable"}}))}
            },
            Request::Action {lease,message}=> {session.view(&lease,true).await?.action(message).await?;Ok(json!({"kind":"ok"}))},
            Request::Layout {value}=> {
                if session.owner {bail!("Use owner control");}
                let control=crate::control::HubControl{hub:session.hub.clone(),boards:session.boards.clone()};
                let result=control.shared_layout(session.hello.grant_id.clone(),session.hello.secret.clone().context("Missing share credential")?,value).await;
                match result {Ok(board)=>Ok(json!({"kind":"board","board":board})),Err(e)=>Ok(error(match e.0 {409=>"Conflict",403=>"NotAuthorized",400|413=>"InvalidBoard",404=>"NotFound",_=>"StorageUnavailable"}))}
            },
            Request::Image {lease,uri}=> {
                let handle=session.view(&lease,false).await?;
                if !published(&session.hub,&handle,&uri).await? {bail!("Unpublished resource");}
                let cancel=stop.child_token();
                let fetch=handle.image(&uri,cancel.clone()); tokio::pin!(fetch);
                let mut tick=tokio::time::interval(Duration::from_millis(100));
                let image=loop {tokio::select! {
                    result=&mut fetch=>break result?,
                    _=tick.tick()=> {
                        if session.view(&lease,false).await.is_err() || !published(&session.hub,&handle,&uri).await.unwrap_or(false) {
                            cancel.cancel();let _=fetch.await;bail!("Resource withdrawn");
                        }
                    }
                }};
                for chunk in image.bytes.chunks(24576) {
                    session.view(&lease,false).await?;
                    if !published(&session.hub,&handle,&uri).await? {bail!("Resource withdrawn");}
                    tokio::time::timeout(IO_TIMEOUT,write_frame(&mut tx,&json!({"kind":"imageChunk","mime":image.mime,"width":image.width,"height":image.height,"data":STANDARD.encode(chunk)}))).await??;
                }
                Ok(json!({"kind":"imageEnd"}))
            }
        }
    }.await;
    let reply = result.unwrap_or_else(|_| error("NotAuthorized"));
    tokio::time::timeout(IO_TIMEOUT, write_frame(&mut tx, &reply)).await??;
    tx.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sharing::Permissions;
    use omochi_transport::{EndpointAddr, SecretKey};
    fn key() -> SecretKey {
        let id = uuid::Uuid::new_v4();
        let mut bytes = [0; 32];
        bytes[..16].copy_from_slice(id.as_bytes());
        bytes[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        SecretKey::from_bytes(&bytes)
    }
    async fn connect(
        endpoint: &Endpoint,
        address: EndpointAddr,
        grant: &Grant,
        secret: Option<&str>,
    ) -> (
        EndpointConnection,
        omochi_transport::SendStream,
        omochi_transport::RecvStream,
        Value,
    ) {
        let connection = endpoint.connect(address, ALPN).await.unwrap();
        let (mut tx, mut rx) = connection.open_bi().await.unwrap();
        write_frame(
            &mut tx,
            &json!({"version":2,"grantId":grant.id,"secret":secret}),
        )
        .await
        .unwrap();
        let welcome = read_frame(&mut rx).await.unwrap().unwrap();
        (connection, tx, rx, welcome)
    }
    async fn req(connection: &EndpointConnection, value: Value) -> Value {
        let (mut tx, mut rx) = connection.open_bi().await.unwrap();
        write_frame(&mut tx, &value).await.unwrap();
        read_frame(&mut rx).await.unwrap().unwrap()
    }
    async fn snapshot(
        tx: &mut omochi_transport::SendStream,
        rx: &mut omochi_transport::RecvStream,
    ) -> Vec<Value> {
        let mut frames = vec![];
        loop {
            let frame: Value = read_frame(rx).await.unwrap().unwrap();
            write_frame(tx, &json!({"ack":frame["sequence"]}))
                .await
                .unwrap();
            let end = frame["kind"] == "snapshotEnd";
            frames.push(frame);
            if end {
                return frames;
            }
        }
    }
    #[tokio::test]
    async fn real_iroh_sharing_filters_actions_layout_and_live_revocation() {
        tokio::time::timeout(Duration::from_secs(20),async {
            let hub=Hub::new();let (surface,mut actions)=crate::scoped::tests::install(&hub,Default::default()).await;
            let dir=tempfile::tempdir().unwrap();let boards=DashboardStore::open(dir.path().join("state")).unwrap();
            let board=boards.mutate(None,json!({"name":"Shared","panels":[{"id":"p","title":"Panel","reference":{"nodeId":"a".repeat(64),"serviceId":"app","surfaceId":"main"},"x":0,"y":0,"w":4,"h":4}]}),false).unwrap().unwrap();
            let shares=boards.shares();let (public,secret)=shares.create(&boards,&board.id,Permissions::default()).unwrap();
            let (private,invite)=shares.create(&boards,&board.id,Permissions{interact:true,edit_layout:true}).unwrap();
            let server=omochi_transport::bind(key(),true,None).await.unwrap();let address=server.addr();let listener=listen(hub.clone(),boards.clone(),server).unwrap();
            let client=omochi_transport::bind(key(),true,None).await.unwrap();
            let (connection,mut tx,mut rx,welcome)=connect(&client,address.clone(),&public,Some(&secret)).await;
            assert_eq!(welcome["board"]["id"],board.id);assert_eq!(welcome["permissions"]["interact"],false);
            let frames=snapshot(&mut tx,&mut rx).await;let lease=frames.iter().find_map(|f|f["lease"].as_str()).unwrap();
            assert!(frames.iter().filter(|f|f["kind"]=="message").all(|f|crate::scoped::surface_id(&f["message"])==Some(surface.as_str())));
            assert_eq!(req(&connection,json!({"op":"action","lease":lease,"message":{"version":"v0.9","action":{"surfaceId":surface,"name":"click"}}})).await["code"],"NotAuthorized");
            assert!(actions.try_recv().is_err());
            assert_eq!(req(&connection,json!({"op":"layout","value":{"revision":1,"name":"Injected","panels":[]}})).await["code"],"NotAuthorized");
            assert_eq!(req(&connection,json!({"op":"image","lease":lease,"uri":"omochi://resources/00000000000000000000000000000000"})).await["code"],"NotAuthorized");
            // Invitations cannot be sent to the ordinary listener as authority.
            let rejected=client.connect(address.clone(),ALPN).await.unwrap();let (mut t,mut r)=rejected.open_bi().await.unwrap();write_frame(&mut t,&json!({"version":1,"grantId":private.id,"secret":invite})).await.unwrap();assert!(read_frame::<_,Value>(&mut r).await.is_err());
            // Adding another identity grant must not interrupt existing public viewers.
            assert_eq!(req(&connection,json!({"op":"ping"})).await["kind"],"ok");
            connection.close(0u32.into(),b"test complete");
            let (registered,mut t,mut r,_)=connect(&client,address.clone(),&private,Some(&invite)).await;
            let frames=snapshot(&mut t,&mut r).await;let lease=frames.iter().find_map(|f|f["lease"].as_str()).unwrap();
            assert_eq!(req(&registered,json!({"op":"action","lease":lease,"message":{"version":"v0.9","action":{"surfaceId":"forged/target","name":"click"}}})).await["kind"],"ok");
            assert_eq!(actions.recv().await.unwrap().into_parts().0.message["action"]["surfaceId"],"main");
            let edited=req(&registered,json!({"op":"layout","value":{"revision":1,"name":"Edited","panels":board.input.panels}})).await;
            assert_eq!(edited["board"]["revision"],2);
            let _=registered.closed().await;
            let target=address.clone();
            let (live,mut t,mut r,welcome)=connect(&client,address,&public,Some(&secret)).await;
            assert_eq!(welcome["board"]["name"],"Edited");snapshot(&mut t,&mut r).await;
            shares.revoke_live(&public.id).await.unwrap();
            assert!(tokio::time::timeout(Duration::from_secs(1),live.closed()).await.is_ok());
            let (queued,mut t,mut r,_)=connect(&client,target,&private,Some(&invite)).await;
            let frames=snapshot(&mut t,&mut r).await;let lease=frames.iter().find_map(|f|f["lease"].as_str()).unwrap();
            assert_eq!(req(&queued,json!({"op":"action","lease":lease,"message":{"version":"v0.9","action":{"surfaceId":surface,"name":"queued"}}})).await["kind"],"ok");
            let (pending,_charge)=actions.recv().await.unwrap().into_parts();
            let scope=pending.scope.unwrap();
            shares.revoke_live(&private.id).await.unwrap();
            assert!(!scope.0.is_cancelled(),"revoking a share must preserve the source service");
            assert!(scope.1.is_cancelled(),"queued work must be cancelled before revocation succeeds");
            hub.shutdown();listener.await.unwrap().unwrap();client.close().await;
        }).await.unwrap();
    }
    #[tokio::test]
    async fn owner_control_and_revocation_are_separate_from_shares() {
        tokio::time::timeout(Duration::from_secs(20), async {
            let hub = Hub::new();
            let dir = tempfile::tempdir().unwrap();
            let boards = DashboardStore::open(dir.path().join("state")).unwrap();
            let board = boards.mutate(None,json!({"name":"Board","panels":[]}),false).unwrap().unwrap();
            let (public, secret) = boards.shares().create(&boards,&board.id,Permissions::default()).unwrap();
            let owners = boards.owners();
            let (owner, token) = owners.create().unwrap();
            let client = omochi_transport::bind(key(),true,None).await.unwrap();
            let server = omochi_transport::bind(key(),true,None).await.unwrap();
            let address = server.addr();
            let listener = listen(hub.clone(),boards.clone(),server).unwrap();
            let (viewer,mut vtx,mut vrx,_) = connect(&client,address.clone(),&public,Some(&secret)).await;
            snapshot(&mut vtx,&mut vrx).await;
            assert_eq!(req(&viewer,json!({"op":"control","command":{"method":"boardsList"}})).await["code"],"NotAuthorized");
            let enrollment = client.connect(address.clone(),OWNER_ENROLL_ALPN).await.unwrap();
            let (mut tx,mut rx) = enrollment.open_bi().await.unwrap();
            write_frame(&mut tx,&json!({"version":1,"grantId":owner.id,"secret":token})).await.unwrap();
            assert_eq!(read_frame::<_,Value>(&mut rx).await.unwrap().unwrap()["accepted"],true);
            let remote = client.connect(address,OWNER_ALPN).await.unwrap();
            let (mut tx,mut rx) = remote.open_bi().await.unwrap();
            write_frame(&mut tx,&json!({"version":1,"grantId":owner.id})).await.unwrap();
            assert_eq!(read_frame::<_,Value>(&mut rx).await.unwrap().unwrap()["kind"],"ownerWelcome");
            snapshot(&mut tx,&mut rx).await;
            let created=req(&remote,json!({"op":"control","command":{"method":"boardCreate","value":{"name":"Remote","panels":[]}}})).await;
            assert_eq!(created["value"]["name"],"Remote");
            // An owner's share revocation must not close the connection awaiting its reply.
            assert_eq!(req(&remote,json!({"op":"control","command":{"method":"shareRevoke","id":public.id}})).await["kind"],"result");
            assert_eq!(req(&remote,json!({"op":"ping"})).await["kind"],"ok");
            assert!(tokio::time::timeout(Duration::from_secs(1),viewer.closed()).await.is_ok());
            owners.revoke_live(&owner.id).await.unwrap();
            assert!(tokio::time::timeout(Duration::from_secs(1),remote.closed()).await.is_ok());
            assert!(owners.authorize(&owner.id,&client.id().to_string(),||Ok(())).is_err());
            hub.shutdown(); listener.await.unwrap().unwrap(); client.close().await;
        }).await.unwrap();
    }
}
