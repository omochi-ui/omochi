//! QUIC, enrollment, timeouts and cancellation for the native adapter.
use super::*;
use omochi_downstream_core::RenderFrame;
use omochi_protocol::{read_frame, write_frame};
use omochi_transport::{Endpoint, EndpointAddr, EndpointConnection, RecvStream, SendStream};
use std::time::Duration;
const IO: Duration = Duration::from_secs(10);

async fn receive(connection: &EndpointConnection, reader: &mut RecvStream) -> Result<Value> {
    match read_frame(reader).await {
        Ok(Some(value)) => Ok(value),
        failure => {
            if matches!(connection.close_reason(), Some(omochi_transport::ConnectionError::ApplicationClosed(reason)) if u64::from(reason.error_code) == 1)
            {
                bail!(Fault::NotAuthorized);
            }
            if failure.is_err_and(|error| {
                matches!(
                    error.kind(),
                    std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof
                )
            }) {
                bail!(Fault::ProtocolError);
            }
            bail!(Fault::NetworkUnavailable)
        }
    }
}

pub(super) struct Active {
    pub(super) endpoint: Endpoint,
    pub(super) connection: EndpointConnection,
    pub(super) stream: AsyncMutex<(SendStream, RecvStream)>,
    pub(super) state: AsyncMutex<SessionState>,
    pub(super) tokens: Mutex<BTreeMap<String, CancellationToken>>,
    pub(super) stop: CancellationToken,
}
impl Active {
    pub(super) async fn rpc(&self, request: Rpc) -> Result<Value> {
        tokio::select! { _=self.stop.cancelled()=>bail!(Fault::ScopeExpired), result=tokio::time::timeout(IO, async {
            let (mut tx,mut rx)=self.connection.open_bi().await?;
            write_frame(&mut tx,&request).await?;
            tx.finish()?;
            let response = receive(&self.connection, &mut rx).await?;
            if response["kind"]=="error" {
                let code: Fault = serde_json::from_value(response["code"].clone()).unwrap_or(Fault::ProtocolError);
                bail!(code);
            }
            Ok(response)
        })=>result? }
    }
    pub(super) async fn next(&self, ack: Option<String>) -> Result<Value> {
        let mut stream = self.stream.lock().await;
        self.state.lock().await.check_ack(ack.as_deref())?;
        if let Some(ack) = ack {
            write_frame(&mut stream.0, &json!({"ack": ack})).await?;
            self.state.lock().await.acknowledge(&ack)?;
        }
        let value: Value = tokio::select! { _ = self.stop.cancelled() => bail!(Fault::ScopeExpired), value = receive(&self.connection, &mut stream.1) => value? };
        let frame: RenderFrame =
            serde_json::from_value(value.clone()).map_err(|_| Fault::ProtocolError)?;
        let effect = self.state.lock().await.accept(&frame)?;
        let mut tokens = self.tokens.lock().unwrap();
        for lease in effect.invalidated_leases {
            if let Some(token) = tokens.remove(&lease) {
                token.cancel();
            }
        }
        if let Some(lease) = effect.lease {
            tokens.entry(lease).or_insert_with(CancellationToken::new);
        }
        Ok(value)
    }
    pub(super) async fn image(&self, request: Rpc) -> Result<Value> {
        let value = serde_json::to_value(&request)?;
        tokio::time::timeout(Duration::from_secs(25), async {
                let (mut tx, mut rx) = self.connection.open_bi().await?;
                write_frame(&mut tx, &value).await?;
                tx.finish()?;
                let mut data = Vec::new();
                let mut metadata: Option<(String,u64,u64)> = None;
                loop {
                    let frame = receive(&self.connection,&mut rx).await?;
                    if frame["kind"] == "imageEnd" {
                        break;
                    }
                    anyhow::ensure!(frame["kind"] == "imageChunk", Fault::InvalidImage);
                    let dimensions = (frame["mime"].as_str().with_context(|| Fault::InvalidImage)?.to_owned(), frame["width"].as_u64().with_context(|| Fault::InvalidImage)?, frame["height"].as_u64().with_context(|| Fault::InvalidImage)?);
                    anyhow::ensure!(dimensions.1 > 0 && dimensions.2 > 0 && dimensions.1.saturating_mul(dimensions.2) <= 4*1024*1024, Fault::InvalidImage);
                    if let Some(previous) = &metadata {anyhow::ensure!(previous == &dimensions, Fault::InvalidImage);} else { metadata = Some(dimensions); }
                    let chunk = frame["data"].as_str().with_context(|| Fault::InvalidImage)?;
                    anyhow::ensure!(chunk.len() <= 32768, Fault::CapacityExceeded);
                    data.extend(
                        STANDARD
                            .decode(chunk)
                            .map_err(|_| anyhow::anyhow!(Fault::InvalidImage))?,
                    );
                    anyhow::ensure!(data.len() <= 8 * 1024 * 1024, Fault::CapacityExceeded);
                }
                anyhow::ensure!(!self.stop.is_cancelled(), Fault::ScopeExpired);
                self.state.lock().await.authorize(&request)?;
                let (mime,width,height)=metadata.with_context(|| Fault::InvalidImage)?;
                let image=omochi_hub::images::validate_image(data,&mime).map_err(|_| Fault::InvalidImage)?;
                anyhow::ensure!(u64::from(image.width)==width && u64::from(image.height)==height,Fault::InvalidImage);
                Ok(json!({"data":STANDARD.encode(image.bytes),"mime":mime,"width":width,"height":height}))
            })
            .await?
    }
}

impl Client {
    pub(super) async fn connect(
        &self,
        command: Command,
        store: Arc<CallbackStore>,
    ) -> Result<Value> {
        let (mut profile, raw) = Profile::load(&store).await?;
        let (link, register, requested_save) = match command {
            Command::Register { invitation, save } => (Link::parse(&invitation)?, true, save),
            Command::Connect { id } => (
                profile
                    .records
                    .iter()
                    .find(|r| r.link.id() == id)
                    .ok_or(Fault::NotFound)?
                    .link
                    .clone(),
                false,
                true,
            ),
            _ => bail!(Fault::InvalidRequest),
        };
        // Persist the identity and pending grant before enrollment can bind it.
        let save = !register || link.mode != Mode::Public || requested_save;
        if save {
            if !profile.records.iter().any(|r| r.link.id() == link.id()) {
                anyhow::ensure!(
                    profile.records.len() < profile::MAX_RECORDS,
                    Fault::CapacityExceeded
                );
                profile.records.push(Record {
                    link: Link {
                        secret: if link.mode == Mode::Public {
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
            if register && link.mode != Mode::Public {
                // A normal connection first recovers a previous successful redemption.
                let recovered = Self::open(&endpoint, &link).await;
                if let Ok((connection, _, _, _)) = &recovered {
                    connection.close(0u32.into(), b"recovered");
                }
                if recovered.is_err() {
                    let alpn = if link.mode == Mode::Owner {
                        b"omochi/owner-enroll/1".as_slice()
                    } else {
                        b"omochi/dashboard-enroll/1".as_slice()
                    };
                    let connection =
                        tokio::time::timeout(IO, endpoint.connect(link.address()?, alpn)).await??;
                    let (mut tx, mut rx) = connection.open_bi().await?;
                    write_frame(
                        &mut tx,
                        &json!({"version":1,"grantId":link.grant_id,"secret":link.secret}),
                    )
                    .await?;
                    let accepted: Value = tokio::time::timeout(IO, read_frame(&mut rx))
                        .await??
                        .with_context(|| Fault::EnrollmentUnknown)?;
                    connection.close(0u32.into(), b"enrolled");
                    anyhow::ensure!(accepted["accepted"] == true, Fault::EnrollmentRejected);
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
        let typed: Welcome =
            serde_json::from_value(welcome.clone()).map_err(|_| Fault::ProtocolError)?;
        let permissions = typed.permissions(link.mode)?;
        *self.active.write().await = Some(Arc::new(Active {
            endpoint,
            connection,
            stream: AsyncMutex::new((tx, rx)),
            state: AsyncMutex::new(SessionState::new(link.mode, permissions)?),
            tokens: Mutex::default(),
            stop: CancellationToken::new(),
        }));
        Ok(welcome)
    }
    pub(super) async fn open(
        endpoint: &Endpoint,
        link: &Link,
    ) -> Result<(EndpointConnection, SendStream, RecvStream, Value)> {
        Self::open_at(endpoint, link, link.address()?).await
    }
    pub(super) async fn open_at(
        endpoint: &Endpoint,
        link: &Link,
        address: EndpointAddr,
    ) -> Result<(EndpointConnection, SendStream, RecvStream, Value)> {
        tokio::time::timeout(IO, async {
            let alpn = if link.mode == Mode::Owner {
                b"omochi/owner/1".as_slice()
            } else {
                b"omochi/dashboard/1".as_slice()
            };
            let connection = endpoint.connect(address, alpn).await?;
            let (mut tx, mut rx) = connection.open_bi().await?;
            let hello = if link.mode == Mode::Public {
                json!({"version":1,"grantId":link.grant_id,"secret":link.secret})
            } else {
                json!({"version":1,"grantId":link.grant_id})
            };
            write_frame(&mut tx, &hello).await?;
            let welcome = receive(&connection, &mut rx).await?;
            let typed: Welcome =
                serde_json::from_value(welcome.clone()).map_err(|_| Fault::ProtocolError)?;
            typed.permissions(link.mode)?;
            Ok((connection, tx, rx, welcome))
        })
        .await?
    }
}
