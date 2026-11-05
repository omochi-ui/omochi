//! Client side of the existing downstream sharing/owner protocol. Never an HTTP proxy.
use super::*;
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use omochi_protocol::{read_frame, write_frame};
use omochi_transport::{
    Endpoint, EndpointAddr, EndpointConnection, RecvStream, SecretKey, SendStream,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const IO: Duration = Duration::from_secs(10);
async fn receive(connection: &EndpointConnection, reader: &mut RecvStream) -> Result<Value> {
    match read_frame(reader).await {
        Ok(Some(value)) => Ok(value),
        _ => {
            if matches!(connection.close_reason(), Some(omochi_transport::ConnectionError::ApplicationClosed(reason)) if u64::from(reason.error_code)==1)
            {
                bail!("NotAuthorized");
            }
            bail!("NetworkUnavailable")
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Link {
    version: u8,
    hub_id: String,
    relay_urls: Vec<String>,
    grant_id: String,
    mode: String,
    secret: String,
}
impl Link {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.version == 1
                && self.hub_id.len() == 64
                && self
                    .hub_id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "InvalidInvitation"
        );
        let _: omochi_transport::EndpointId = self
            .hub_id
            .parse()
            .map_err(|_| anyhow::anyhow!("InvalidInvitation"))?;
        let id = uuid::Uuid::parse_str(&self.grant_id)
            .map_err(|_| anyhow::anyhow!("InvalidInvitation"))?;
        anyhow::ensure!(
            id.to_string() == self.grant_id
                && matches!(self.mode.as_str(), "invite" | "public" | "owner"),
            "InvalidInvitation"
        );
        anyhow::ensure!(
            self.secret.len() == 64
                && self
                    .secret
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "InvalidInvitation"
        );
        anyhow::ensure!(
            (1..=4).contains(&self.relay_urls.len()),
            "InvalidInvitation"
        );
        let mut unique = std::collections::BTreeSet::new();
        for relay in &self.relay_urls {
            // Relay URL parsing also happens in the transport. Reject URL credentials here.
            let url = url::Url::parse(relay).map_err(|_| anyhow::anyhow!("InvalidInvitation"))?;
            anyhow::ensure!(
                url.scheme() == "https"
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none(),
                "InvalidInvitation"
            );
            let address: EndpointAddr =
                serde_json::from_value(json!({"id":self.hub_id,"addrs":[{"Relay":relay}]}))
                    .map_err(|_| anyhow::anyhow!("InvalidInvitation"))?;
            anyhow::ensure!(
                relay.len() <= 2048
                    && relay.starts_with("https://")
                    && !relay.contains(['@', '?', '#'])
                    && address.relay_urls().next().is_some()
                    && unique.insert(relay),
                "InvalidInvitation"
            );
        }
        Ok(())
    }
    pub fn parse(input: &str) -> Result<Self> {
        anyhow::ensure!(input.len() <= 32768, "InvalidInvitation");
        let (kind, payload) = if let Some(rest) = input.strip_prefix("omochi://") {
            rest.split_once('/').context("InvalidInvitation")?
        } else {
            let url = url::Url::parse(input).map_err(|_| anyhow::anyhow!("InvalidInvitation"))?;
            anyhow::ensure!(
                url.scheme() == "https"
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none(),
                "InvalidInvitation"
            );
            input
                .split_once('#')
                .context("InvalidInvitation")?
                .1
                .split_once(':')
                .context("InvalidInvitation")?
        };
        anyhow::ensure!(matches!(kind, "share" | "owner"), "InvalidInvitation");
        let bytes = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| anyhow::anyhow!("InvalidInvitation"))?;
        let link: Self =
            serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("InvalidInvitation"))?;
        link.validate()?;
        anyhow::ensure!(
            (kind == "owner") == (link.mode == "owner"),
            "InvalidInvitation"
        );
        Ok(link)
    }
    fn address(&self) -> Result<EndpointAddr> {
        Ok(serde_json::from_value(
            json!({"id":self.hub_id,"addrs":self.relay_urls.iter().map(|r|json!({"Relay":r})).collect::<Vec<_>>()}),
        )?)
    }
    fn id(&self) -> String {
        format!("{}/{}", self.hub_id, self.grant_id)
    }
    fn metadata(&self) -> Value {
        json!({"id":self.id(),"hubId":self.hub_id,"grantId":self.grant_id,"mode":self.mode})
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    link: Link,
    accepted: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    version: u8,
    key: String,
    records: Vec<Record>,
}
impl Profile {
    fn key(&self) -> Result<SecretKey> {
        let bytes: [u8; 32] = STANDARD
            .decode(&self.key)
            .map_err(|_| anyhow::anyhow!("StorageCorrupt"))?
            .try_into()
            .map_err(|_| anyhow::anyhow!("StorageCorrupt"))?;
        anyhow::ensure!(STANDARD.encode(bytes) == self.key, "StorageCorrupt");
        Ok(SecretKey::from_bytes(&bytes))
    }
    async fn load(store: &CallbackStore) -> Result<(Self, Option<String>)> {
        let raw = store.read().await?;
        let profile: Self = match &raw {
            Some(raw) => {
                serde_json::from_str(raw).map_err(|_| anyhow::anyhow!("StorageCorrupt"))?
            }
            None => Self {
                version: 1,
                key: STANDARD.encode(SecretKey::generate().to_bytes()),
                records: vec![],
            },
        };
        anyhow::ensure!(
            profile.version == 1 && profile.records.len() <= 1000,
            "StorageCorrupt"
        );
        profile.key()?;
        let mut ids = std::collections::BTreeSet::new();
        for record in &profile.records {
            anyhow::ensure!(ids.insert(record.link.id()), "StorageCorrupt");
            record
                .link
                .validate()
                .map_err(|_| anyhow::anyhow!("StorageCorrupt"))?;
        }
        Ok((profile, raw))
    }
    async fn save(&self, store: &CallbackStore, expected: Option<String>) -> Result<()> {
        let replacement = serde_json::to_string(self)?;
        anyhow::ensure!(replacement.len() <= 1024 * 1024, "CapacityExceeded");
        match store.compare_exchange(expected, replacement).await {
            CommitOutcome::Committed => Ok(()),
            CommitOutcome::Conflict => bail!("StorageConflict"),
            CommitOutcome::Failed => bail!("StorageUnavailable"),
            CommitOutcome::Unknown => bail!("StorageOutcomeUnknown"),
        }
    }
}
struct Active {
    endpoint: Endpoint,
    connection: EndpointConnection,
    link: Link,
    stream: AsyncMutex<(SendStream, RecvStream, Option<String>, u64)>,
    leases: AsyncMutex<BTreeMap<String, String>>,
    permissions: Value,
    tokens: Mutex<BTreeMap<String, CancellationToken>>,
    snapshot: AsyncMutex<omochi_ui::A2uiSnapshot>,
    stop: CancellationToken,
}
impl Active {
    async fn rpc(&self, request: Value) -> Result<Value> {
        tokio::select! { _=self.stop.cancelled()=>bail!("ScopeExpired"), result=tokio::time::timeout(IO, async {
            let (mut tx,mut rx)=self.connection.open_bi().await?;
            write_frame(&mut tx,&request).await?;
            tx.finish()?;
            let response = receive(&self.connection, &mut rx).await?;
            if response["kind"]=="error" { bail!("{}",response["code"].as_str().unwrap_or("ProtocolError")); }
            Ok(response)
        })=>result? }
    }
}
#[derive(Default)]
pub struct Client {
    active: RwLock<Option<Arc<Active>>>,
    admin: AsyncMutex<()>,
}
impl Client {
    pub async fn shutdown(&self) {
        let _admin = self.admin.lock().await;
        self.close().await;
    }
    pub async fn lease_token(&self, lease: &str) -> Result<CancellationToken> {
        let active = self.active.read().await.clone().context("ScopeExpired")?;
        let token = active
            .tokens
            .lock()
            .unwrap()
            .get(lease)
            .cloned()
            .context("ScopeExpired")?;
        Ok(token)
    }
    pub async fn close(&self) {
        if let Some(active) = self.active.write().await.take() {
            active.stop.cancel();
            for token in active.tokens.lock().unwrap().values() {
                token.cancel();
            }
            active.connection.close(0u32.into(), b"closed");
            active.endpoint.close().await;
        }
    }
    pub async fn execute(
        &self,
        request: Value,
        store: Arc<CallbackStore>,
        cancel: CancellationToken,
    ) -> Result<Value> {
        tokio::select! { _=cancel.cancelled()=>{self.close().await;bail!("Cancelled")}, result=self.execute_inner(request,store)=>result }
    }
    async fn execute_inner(&self, request: Value, store: Arc<CallbackStore>) -> Result<Value> {
        let op = request["method"].as_str().context("InvalidRequest")?;
        if op == "parse" {
            return Ok(Link::parse(
                request["invitation"]
                    .as_str()
                    .context("InvalidInvitation")?,
            )?
            .metadata());
        }
        if matches!(op, "list" | "register" | "forget" | "connect") {
            let _lock = self.admin.lock().await;
            let (mut profile, raw) = Profile::load(&store).await?;
            if op == "list" {
                return Ok(json!(
                    profile
                        .records
                        .iter()
                        .map(|r| {
                            let mut v = r.link.metadata();
                            v["accepted"] = r.accepted.into();
                            v
                        })
                        .collect::<Vec<_>>()
                ));
            }
            if op == "forget" {
                self.close().await;
                profile
                    .records
                    .retain(|r| r.link.id() != request["id"].as_str().unwrap_or(""));
                profile.save(&store, raw).await?;
                return Ok(Value::Null);
            }
            let link = if op == "register" {
                Link::parse(
                    request["invitation"]
                        .as_str()
                        .context("InvalidInvitation")?,
                )?
            } else {
                profile
                    .records
                    .iter()
                    .find(|r| Some(r.link.id().as_str()) == request["id"].as_str())
                    .context("NotFound")?
                    .link
                    .clone()
            };
            // Persist the identity and pending grant before enrollment can bind it.
            let save = op == "connect" || link.mode != "public" || request["save"] == true;
            if save {
                if !profile.records.iter().any(|r| r.link.id() == link.id()) {
                    profile.records.push(Record {
                        link: Link {
                            secret: if link.mode == "public" {
                                link.secret.clone()
                            } else {
                                "0".repeat(64)
                            },
                            ..link.clone()
                        },
                        accepted: false,
                    });
                }
                profile.save(&store, raw).await?;
            } else if raw.is_none() {
                profile.save(&store, raw).await?;
            }
            self.close().await;
            let endpoint = omochi_transport::bind(profile.key()?, false, None).await?;
            let result = async {
                if op == "register" && link.mode != "public" {
                    // A normal connection first recovers a previous successful redemption.
                    let recovered = Self::open(&endpoint, &link).await;
                    if let Ok((connection, _, _, _)) = &recovered {
                        connection.close(0u32.into(), b"recovered");
                    }
                    if recovered.is_err() {
                        let alpn = if link.mode == "owner" {
                            b"omochi/owner-enroll/1".as_slice()
                        } else {
                            b"omochi/dashboard-enroll/1".as_slice()
                        };
                        let connection =
                            tokio::time::timeout(IO, endpoint.connect(link.address()?, alpn))
                                .await??;
                        let (mut tx, mut rx) = connection.open_bi().await?;
                        write_frame(
                            &mut tx,
                            &json!({"version":1,"grantId":link.grant_id,"secret":link.secret}),
                        )
                        .await?;
                        let accepted: Value = tokio::time::timeout(IO, read_frame(&mut rx))
                            .await??
                            .context("EnrollmentUnknown")?;
                        connection.close(0u32.into(), b"enrolled");
                        anyhow::ensure!(accepted["accepted"] == true, "EnrollmentRejected");
                    }
                }
                Self::open(&endpoint, &link).await
            }
            .await;
            let (connection, tx, rx, welcome) = match result {
                Ok(value) => value,
                Err(error) => {
                    endpoint.close().await;
                    return Err(error);
                }
            };
            if save {
                let (mut current, expected) = match Profile::load(&store).await {
                    Ok(value) => value,
                    Err(error) => {
                        connection.close(0u32.into(), b"unsaved");
                        endpoint.close().await;
                        return Err(error);
                    }
                };
                current
                    .records
                    .iter_mut()
                    .filter(|r| r.link.id() == link.id())
                    .for_each(|r| r.accepted = true);
                if let Err(error) = current.save(&store, expected).await {
                    connection.close(0u32.into(), b"unsaved");
                    endpoint.close().await;
                    return Err(error);
                }
            }
            let permissions = if link.mode == "owner" {
                json!({"interact":true,"editLayout":true})
            } else {
                welcome["permissions"].clone()
            };
            *self.active.write().await = Some(Arc::new(Active {
                endpoint,
                connection,
                link,
                stream: AsyncMutex::new((tx, rx, None, 0)),
                leases: AsyncMutex::default(),
                permissions,
                tokens: Mutex::default(),
                snapshot: AsyncMutex::new(omochi_ui::A2uiSnapshot::new()),
                stop: CancellationToken::new(),
            }));
            return Ok(welcome);
        }
        if op == "disconnect" {
            self.close().await;
            return Ok(Value::Null);
        }
        let active = self
            .active
            .read()
            .await
            .clone()
            .context("SourceUnavailable")?;
        if op == "next" {
            let mut stream = active.stream.lock().await;
            if let Some(ack) = stream.2.take() {
                anyhow::ensure!(request["ack"] == ack, "ProtocolError");
                write_frame(&mut stream.0, &json!({"ack":ack})).await?;
            }
            let frame: Value = tokio::select! {_=active.stop.cancelled()=>bail!("ScopeExpired"),value=receive(&active.connection,&mut stream.1)=>value?};
            let sequence = frame["sequence"].as_str().context("ProtocolError")?;
            let number: u64 = sequence
                .parse()
                .map_err(|_| anyhow::anyhow!("ProtocolError"))?;
            anyhow::ensure!(
                number > stream.3 && number.to_string() == sequence,
                "ProtocolError"
            );
            stream.3 = number;
            stream.2 = Some(sequence.to_owned());
            anyhow::ensure!(
                matches!(frame["kind"].as_str(), Some("message" | "snapshotEnd")),
                "ProtocolError"
            );
            if frame["kind"] == "message" {
                let message = &frame["message"];
                anyhow::ensure!(message["version"] == "v0.9", "ProtocolError");
                let keys = [
                    "createSurface",
                    "updateComponents",
                    "updateDataModel",
                    "deleteSurface",
                ];
                let bodies = keys
                    .iter()
                    .filter_map(|key| message.get(key))
                    .collect::<Vec<_>>();
                anyhow::ensure!(bodies.len() == 1, "ProtocolError");
                let surface = bodies[0]["surfaceId"].as_str().context("ProtocolError")?;
                let mut snapshot = active.snapshot.lock().await;
                snapshot
                    .apply(message)
                    .map_err(|_| anyhow::anyhow!("ProtocolError"))?;
                let surfaces = snapshot.surface_ids();
                let services = surfaces
                    .iter()
                    .filter_map(|id| id.split_once('/').map(|v| v.0))
                    .collect::<std::collections::BTreeSet<_>>();
                let nodes = services
                    .iter()
                    .filter_map(|id| id.split_once('~').map(|v| v.0))
                    .collect::<std::collections::BTreeSet<_>>();
                anyhow::ensure!(nodes.len() <= 2 && services.len() <= 8, "CapacityExceeded");
                let bytes = snapshot
                    .messages()
                    .iter()
                    .map(|v| serde_json::to_vec(v).map(|b| b.len()))
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .into_iter()
                    .sum::<usize>();
                anyhow::ensure!(bytes <= 32 * 1024 * 1024, "CapacityExceeded");
                drop(snapshot);

                let mut leases = active.leases.lock().await;
                if message.get("deleteSurface").is_some() {
                    if let Some(old) = leases.remove(surface) {
                        if let Some(token) = active.tokens.lock().unwrap().remove(&old) {
                            token.cancel();
                        }
                    }
                } else {
                    let lease = frame["lease"].as_str().context("ProtocolError")?;
                    anyhow::ensure!(!lease.is_empty() && lease.len() <= 100, "ProtocolError");
                    if !leases.contains_key(surface) {
                        anyhow::ensure!(leases.len() < 1024, "CapacityExceeded");
                    }
                    if let Some(old) = leases.insert(surface.to_owned(), lease.to_owned()) {
                        if old != lease || message.get("createSurface").is_some() {
                            if let Some(token) = active.tokens.lock().unwrap().remove(&old) {
                                token.cancel();
                            }
                        }
                    }
                    active
                        .tokens
                        .lock()
                        .unwrap()
                        .entry(lease.to_owned())
                        .or_insert_with(CancellationToken::new);
                }
            }
            return Ok(frame);
        }
        let value = request["value"].clone();
        match value["op"].as_str() {
            Some("ping") => {}
            Some("control") => anyhow::ensure!(active.link.mode == "owner", "NotAuthorized"),
            Some("layout") => {
                anyhow::ensure!(active.permissions["editLayout"] == true, "NotAuthorized")
            }
            Some("action" | "image") => {
                anyhow::ensure!(
                    active
                        .leases
                        .lock()
                        .await
                        .values()
                        .any(|v| Some(v.as_str()) == value["lease"].as_str()),
                    "ScopeExpired"
                );
                if value["op"] == "action" {
                    anyhow::ensure!(active.permissions["interact"] == true, "NotAuthorized");
                }
            }
            _ => bail!("InvalidRequest"),
        }
        if value["op"] == "image" {
            return tokio::time::timeout(Duration::from_secs(25), async {
                let (mut tx, mut rx) = active.connection.open_bi().await?;
                write_frame(&mut tx, &value).await?;
                tx.finish()?;
                let mut data = Vec::new();
                let mut metadata: Option<(String,u64,u64)> = None;
                loop {
                    let frame = receive(&active.connection,&mut rx).await?;
                    if frame["kind"] == "imageEnd" {
                        break;
                    }
                    anyhow::ensure!(frame["kind"] == "imageChunk", "InvalidImage");
                    let dimensions = (frame["mime"].as_str().context("InvalidImage")?.to_owned(), frame["width"].as_u64().context("InvalidImage")?, frame["height"].as_u64().context("InvalidImage")?);
                    anyhow::ensure!(dimensions.1 > 0 && dimensions.2 > 0 && dimensions.1.saturating_mul(dimensions.2) <= 4*1024*1024, "InvalidImage");
                    if let Some(previous) = &metadata {anyhow::ensure!(previous == &dimensions, "InvalidImage");} else { metadata = Some(dimensions); }
                    let chunk = frame["data"].as_str().context("InvalidImage")?;
                    anyhow::ensure!(chunk.len() <= 32768, "CapacityExceeded");
                    data.extend(
                        STANDARD
                            .decode(chunk)
                            .map_err(|_| anyhow::anyhow!("InvalidImage"))?,
                    );
                    anyhow::ensure!(data.len() <= 8 * 1024 * 1024, "CapacityExceeded");
                }
                anyhow::ensure!(
                    !active.stop.is_cancelled()
                        && active
                            .leases
                            .lock()
                            .await
                            .values()
                            .any(|v| Some(v.as_str()) == value["lease"].as_str()),
                    "ScopeExpired"
                );
                let (mime,width,height)=metadata.context("InvalidImage")?;
                let image=omochi_hub::images::validate_image(data,&mime)?;
                anyhow::ensure!(u64::from(image.width)==width && u64::from(image.height)==height,"InvalidImage");
                Ok(json!({"data":STANDARD.encode(image.bytes),"mime":mime,"width":width,"height":height}))
            })
            .await?;
        }
        active.rpc(value).await
    }
    async fn open(
        endpoint: &Endpoint,
        link: &Link,
    ) -> Result<(EndpointConnection, SendStream, RecvStream, Value)> {
        Self::open_at(endpoint, link, link.address()?).await
    }
    async fn open_at(
        endpoint: &Endpoint,
        link: &Link,
        address: EndpointAddr,
    ) -> Result<(EndpointConnection, SendStream, RecvStream, Value)> {
        tokio::time::timeout(IO, async {
            let alpn = if link.mode == "owner" {
                b"omochi/owner/1".as_slice()
            } else {
                b"omochi/dashboard/1".as_slice()
            };
            let connection = endpoint.connect(address, alpn).await?;
            let (mut tx, mut rx) = connection.open_bi().await?;
            let hello = if link.mode == "public" {
                json!({"version":1,"grantId":link.grant_id,"secret":link.secret})
            } else {
                json!({"version":1,"grantId":link.grant_id})
            };
            write_frame(&mut tx, &hello).await?;
            let welcome = receive(&connection, &mut rx).await?;
            anyhow::ensure!(
                welcome["kind"]
                    == if link.mode == "owner" {
                        "ownerWelcome"
                    } else {
                        "welcome"
                    },
                "ProtocolError"
            );
            if link.mode == "public" {
                anyhow::ensure!(
                    welcome["permissions"]["interact"] == false
                        && welcome["permissions"]["editLayout"] == false,
                    "ProtocolError"
                );
            }
            Ok((connection, tx, rx, welcome))
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_invitation_fixture() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../../spec/fixtures/hub-invitations.json"))
                .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            assert_eq!(
                Link::parse(case["input"].as_str().unwrap()).is_ok(),
                case["valid"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
    #[test]
    fn invitation_modes_and_secrets_are_checked() {
        let value = json!({"version":1,"hubId":SecretKey::generate().public().to_string(),"grantId":uuid::Uuid::nil(),"relayUrls":["https://relay.example/"],"mode":"owner","secret":"cd".repeat(32)});
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).unwrap());
        assert!(Link::parse(&format!("omochi://owner/{payload}")).is_ok());
        assert!(Link::parse(&format!("https://app.omochi.fyi/#owner:{payload}")).is_ok());
        assert!(Link::parse(&format!("omochi://share/{payload}")).is_err());
        assert!(Link::parse("omochi://owner/invalid").is_err());
    }
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    /// A real QUIC connection, using the same frames as the server implementation.
    #[tokio::test]
    async fn downstream_owner_welcome_and_control_over_quic() {
        let server = omochi_transport::bind(SecretKey::generate(), true, None)
            .await
            .unwrap();
        server.set_alpns(vec![b"omochi/owner/1".to_vec()]);
        let client = omochi_transport::bind(SecretKey::generate(), true, None)
            .await
            .unwrap();
        let address = server.addr();
        let server_task = tokio::spawn(async move {
            let connection = server.accept().await.unwrap().await.unwrap();
            let (mut tx, mut rx) = connection.accept_bi().await.unwrap();
            let hello: Value = read_frame(&mut rx).await.unwrap().unwrap();
            assert_eq!(
                hello,
                json!({"version":1,"grantId":uuid::Uuid::nil().to_string()})
            );
            write_frame(&mut tx, &json!({"kind":"ownerWelcome","services":[]}))
                .await
                .unwrap();
            let (mut reply, mut request) = connection.accept_bi().await.unwrap();
            let command: Value = read_frame(&mut request).await.unwrap().unwrap();
            assert_eq!(command["command"]["method"], "boardsList");
            write_frame(&mut reply, &json!({"kind":"result","value":[]}))
                .await
                .unwrap();
            reply.finish().unwrap();
            let _ = reply.stopped().await;
            connection.close(0u32.into(), b"done");
            server.close().await;
        });
        // Direct loopback avoids depending on external relay availability.
        let connection = tokio::time::timeout(IO, client.connect(address, b"omochi/owner/1"))
            .await
            .unwrap()
            .unwrap();
        let (mut tx, mut rx) = connection.open_bi().await.unwrap();
        write_frame(
            &mut tx,
            &json!({"version":1,"grantId":uuid::Uuid::nil().to_string()}),
        )
        .await
        .unwrap();
        let welcome: Value = read_frame(&mut rx).await.unwrap().unwrap();
        assert_eq!(welcome["kind"], "ownerWelcome");
        let link = Link {
            version: 1,
            hub_id: connection.remote_id().to_string(),
            relay_urls: vec!["https://relay.example/".into()],
            grant_id: uuid::Uuid::nil().to_string(),
            mode: "owner".into(),
            secret: "0".repeat(64),
        };
        let active = Active {
            endpoint: client.clone(),
            connection,
            link,
            stream: AsyncMutex::new((tx, rx, None, 0)),
            leases: AsyncMutex::default(),
            permissions: json!({"interact":true,"editLayout":true}),
            tokens: Mutex::default(),
            snapshot: AsyncMutex::new(omochi_ui::A2uiSnapshot::new()),
            stop: CancellationToken::new(),
        };
        let response = active
            .rpc(json!({"op":"control","command":{"method":"boardsList"}}))
            .await
            .unwrap();
        assert_eq!(response, json!({"kind":"result","value":[]}));
        server_task.await.unwrap();
        client.close().await;
    }
    #[tokio::test]
    async fn leases_expire_on_deletion_and_ack_follows_application() {
        let server = omochi_transport::bind(SecretKey::generate(), true, None)
            .await
            .unwrap();
        server.set_alpns(vec![b"omochi/dashboard/1".to_vec()]);
        let endpoint = omochi_transport::bind(SecretKey::generate(), true, None)
            .await
            .unwrap();
        let address = server.addr();
        let link = Link {
            version: 1,
            hub_id: server.id().to_string(),
            relay_urls: vec!["https://relay.example/".into()],
            grant_id: uuid::Uuid::nil().to_string(),
            mode: "public".into(),
            secret: "cd".repeat(32),
        };
        let serving = tokio::spawn(async move {
            let connection = server.accept().await.unwrap().await.unwrap();
            let (mut tx, mut rx) = connection.accept_bi().await.unwrap();
            let hello: Value = read_frame(&mut rx).await.unwrap().unwrap();
            assert_eq!(hello["secret"], "cd".repeat(32));
            write_frame(&mut tx,&json!({"kind":"welcome","board":{"id":"board","panels":[]},"permissions":{"interact":false,"editLayout":false}})).await.unwrap();
            for (sequence, message, lease) in [
                (
                    "1",
                    json!({"version":"v0.9","createSurface":{"surfaceId":"node~counter/main"}}),
                    json!("lease-1"),
                ),
                (
                    "2",
                    json!({"version":"v0.9","deleteSurface":{"surfaceId":"node~counter/main"}}),
                    Value::Null,
                ),
            ] {
                write_frame(
                    &mut tx,
                    &json!({"kind":"message","message":message,"lease":lease,"sequence":sequence}),
                )
                .await
                .unwrap();
                let ack: Value = read_frame(&mut rx).await.unwrap().unwrap();
                assert_eq!(ack, json!({"ack":sequence}));
            }
            write_frame(&mut tx, &json!({"kind":"snapshotEnd","sequence":"3"}))
                .await
                .unwrap();
            connection.closed().await;
            server.close().await;
        });
        let (connection, tx, rx, welcome) =
            Client::open_at(&endpoint, &link, address).await.unwrap();
        let client = Client::default();
        *client.active.write().await = Some(Arc::new(Active {
            endpoint,
            connection,
            link,
            stream: AsyncMutex::new((tx, rx, None, 0)),
            leases: AsyncMutex::default(),
            permissions: welcome["permissions"].clone(),
            tokens: Mutex::default(),
            snapshot: AsyncMutex::new(omochi_ui::A2uiSnapshot::new()),
            stop: CancellationToken::new(),
        }));
        let store = Arc::new(CallbackStore::new());
        let frame = client
            .execute_inner(json!({"method":"next"}), store.clone())
            .await
            .unwrap();
        assert_eq!(frame["sequence"], "1");
        let token = client.lease_token("lease-1").await.unwrap();
        assert!(!token.is_cancelled());
        let denied = client
            .execute_inner(
                json!({"method":"rpc","value":{"op":"action","lease":"lease-1","message":{}}}),
                store.clone(),
            )
            .await
            .unwrap_err();
        assert_eq!(denied.to_string(), "NotAuthorized");
        client
            .execute_inner(json!({"method":"next","ack":"1"}), store.clone())
            .await
            .unwrap();
        assert!(token.is_cancelled());
        assert!(client.lease_token("lease-1").await.is_err());
        let frame = client
            .execute_inner(json!({"method":"next","ack":"2"}), store)
            .await
            .unwrap();
        assert_eq!(frame["kind"], "snapshotEnd");
        client.close().await;
        serving.await.unwrap();
    }
    #[test]
    fn stored_profile_never_recovers_from_corrupt_key() {
        let profile = Profile {
            version: 1,
            key: "corrupt".into(),
            records: vec![],
        };
        assert!(profile.key().is_err());
    }
}
