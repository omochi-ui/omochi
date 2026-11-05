//! QUIC, enrollment, timeouts and cancellation for the native adapter.
use super::*;
use omochi_downstream_core::RenderFrame;
use omochi_protocol::{read_frame, write_frame};
use omochi_transport::{Endpoint, EndpointAddr, EndpointConnection, RecvStream, SendStream};
use std::{io, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt};
const IO: Duration = Duration::from_secs(10);

/// Idle subscriptions may wait indefinitely. Once a frame begins, its entire
/// remainder shares one deadline; trickling bytes must not extend it.
async fn read_with_partial_deadline<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> io::Result<Option<Value>> {
    let mut first = [0];
    if reader.read(&mut first).await? == 0 {
        return Ok(None);
    }
    // Replay the first byte through the common decoder so framing limits and
    // malformed/truncated-frame handling stay identical to other transports.
    let mut frame = first.as_slice().chain(reader);
    tokio::time::timeout(IO, read_frame(&mut frame))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "partial frame deadline exceeded"))?
}

fn receive_fault(failure: io::Result<Option<Value>>, not_authorized: bool) -> Result<Value> {
    match failure {
        Ok(Some(value)) => Ok(value),
        _ if not_authorized => bail!(Fault::NotAuthorized),
        Err(error) => match error.kind() {
            io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof => {
                bail!(Fault::ProtocolError)
            }
            io::ErrorKind::TimedOut => bail!(Fault::Timeout),
            _ => bail!(Fault::NetworkUnavailable),
        },
        Ok(None) => bail!(Fault::NetworkUnavailable),
    }
}

async fn receive<R: AsyncRead + Unpin>(
    connection: &EndpointConnection,
    reader: &mut R,
) -> Result<Value> {
    let failure = read_with_partial_deadline(reader).await;
    let not_authorized = matches!(connection.close_reason(), Some(omochi_transport::ConnectionError::ApplicationClosed(reason)) if u64::from(reason.error_code) == 1);
    receive_fault(failure, not_authorized)
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
        let value = self.receive_next(&mut stream.1).await?;
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
    async fn receive_next<R: AsyncRead + Unpin>(&self, reader: &mut R) -> Result<Value> {
        let received = tokio::select! { _ = self.stop.cancelled() => bail!(Fault::ScopeExpired), value = receive(&self.connection, reader) => value };
        if received
            .as_ref()
            .is_err_and(|error| error.downcast_ref::<Fault>() == Some(&Fault::Timeout))
        {
            // The decoder has consumed a partial frame. Neither another poll nor
            // a successful ping can make this stream usable again.
            self.stop.cancel();
            self.state.lock().await.close();
            for token in self.tokens.lock().unwrap().values() {
                token.cancel();
            }
            self.connection.close(0u32.into(), b"partial frame timeout");
        }
        received
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

#[cfg(test)]
mod deadline_tests {
    use super::*;
    use std::{future::Future, pin::Pin, task::Poll};
    use tokio::io::AsyncWriteExt;

    async fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
        let mut future = future;
        std::future::poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx))).await
    }

    #[tokio::test(start_paused = true)]
    async fn idle_render_stream_waits_without_starting_the_frame_deadline() {
        let (mut tx, mut rx) = tokio::io::duplex(256);
        let read = read_with_partial_deadline(&mut rx);
        tokio::pin!(read);
        assert!(poll_once(read.as_mut()).await.is_pending());
        tokio::time::advance(IO * 3).await;
        assert!(poll_once(read.as_mut()).await.is_pending());

        let expected = json!({"kind":"snapshotEnd","sequence":"1"});
        write_frame(&mut tx, &expected).await.unwrap();
        let Poll::Ready(result) = poll_once(read.as_mut()).await else {
            panic!("a complete frame must be readable after a long idle interval");
        };
        assert_eq!(result.unwrap(), Some(expected));
    }

    #[tokio::test(start_paused = true)]
    async fn partial_header_and_body_share_a_fixed_ten_second_deadline() {
        for prefix in [vec![0], vec![0, 0, 0, 4, b'n']] {
            let (mut tx, mut rx) = tokio::io::duplex(256);
            tx.write_all(&prefix).await.unwrap();
            let read = read_with_partial_deadline(&mut rx);
            tokio::pin!(read);
            assert!(poll_once(read.as_mut()).await.is_pending());

            tokio::time::advance(IO - Duration::from_secs(1)).await;
            tx.write_all(&[0]).await.unwrap();
            assert!(poll_once(read.as_mut()).await.is_pending());
            tokio::time::advance(Duration::from_secs(1)).await;
            let Poll::Ready(result) = poll_once(read.as_mut()).await else {
                panic!("a partial frame must time out even when another byte arrives");
            };
            let error = receive_fault(result, false).unwrap_err();
            assert_eq!(error.downcast_ref::<Fault>(), Some(&Fault::Timeout));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn completing_a_frame_allows_another_idle_interval() {
        let (mut tx, mut rx) = tokio::io::duplex(256);
        tx.write_all(&[0]).await.unwrap();
        {
            let read = read_with_partial_deadline(&mut rx);
            tokio::pin!(read);
            assert!(poll_once(read.as_mut()).await.is_pending());
            tokio::time::advance(IO - Duration::from_secs(1)).await;
            tx.write_all(&[0, 0, 4, b'n', b'u', b'l', b'l'])
                .await
                .unwrap();
            let Poll::Ready(result) = poll_once(read.as_mut()).await else {
                panic!("a complete frame before the deadline must succeed");
            };
            assert_eq!(result.unwrap(), Some(Value::Null));
        }
        let read = read_with_partial_deadline(&mut rx);
        tokio::pin!(read);
        assert!(poll_once(read.as_mut()).await.is_pending());
        tokio::time::advance(IO * 3).await;
        assert!(poll_once(read.as_mut()).await.is_pending());
        write_frame(&mut tx, &json!({"next":true})).await.unwrap();
        let Poll::Ready(result) = poll_once(read.as_mut()).await else {
            panic!("the next frame must have its own deadline");
        };
        assert_eq!(result.unwrap(), Some(json!({"next":true})));
    }

    #[tokio::test]
    async fn partial_render_timeout_expires_the_session_and_cannot_resume() {
        // Establish transport handles with the real clock. The deadline itself
        // uses a controlled reader, so QUIC scheduling cannot race clock advances.
        let server = omochi_transport::bind(omochi_transport::SecretKey::generate(), true, None)
            .await
            .unwrap();
        let endpoint = omochi_transport::bind(omochi_transport::SecretKey::generate(), true, None)
            .await
            .unwrap();
        let (connection, peer) = tokio::join!(
            endpoint.connect(server.addr(), omochi_protocol::ALPN),
            async { server.accept().await.unwrap().await.unwrap() }
        );
        let connection = connection.unwrap();
        let (mut tx, rx) = connection.open_bi().await.unwrap();
        write_frame(&mut tx, &Value::Null).await.unwrap();
        let (_updates, mut commands) = peer.accept_bi().await.unwrap();
        assert_eq!(
            read_frame::<_, Value>(&mut commands).await.unwrap(),
            Some(Value::Null)
        );
        let lease = CancellationToken::new();
        let active = Arc::new(Active {
            endpoint,
            connection,
            stream: AsyncMutex::new((tx, rx)),
            state: AsyncMutex::new(SessionState::new(Mode::Public, Default::default()).unwrap()),
            tokens: Mutex::new(BTreeMap::from([("lease-1".into(), lease.clone())])),
            stop: CancellationToken::new(),
        });
        let client = Client::default();
        *client.active.write().await = Some(active.clone());
        tokio::time::pause();
        let (mut tx, mut rx) = tokio::io::duplex(256);
        tx.write_all(&[0, 0, 0, 4, b'n']).await.unwrap();
        let received = tokio::task::unconstrained(active.receive_next(&mut rx));
        tokio::pin!(received);
        assert!(poll_once(received.as_mut()).await.is_pending());
        // Pausing after transport setup need not land on Tokio's millisecond
        // timer tick. Cross that tick as well as the partial-frame deadline.
        tokio::time::advance(IO + Duration::from_millis(1)).await;
        let Poll::Ready(result) = poll_once(received.as_mut()).await else {
            panic!("partial render data must stop the active subscription at its deadline");
        };
        assert_eq!(
            result.unwrap_err().downcast_ref::<Fault>(),
            Some(&Fault::Timeout)
        );
        assert!(active.stop.is_cancelled());
        assert!(lease.is_cancelled());
        assert!(active.connection.close_reason().is_some());
        assert_eq!(
            active
                .state
                .lock()
                .await
                .authorize(&Rpc::Ping {})
                .unwrap_err(),
            Fault::ScopeExpired
        );
        assert_eq!(
            active.next(None).await.unwrap_err().downcast_ref::<Fault>(),
            Some(&Fault::ScopeExpired)
        );
        assert_eq!(
            client
                .execute(
                    Command::Rpc {
                        value: Rpc::Ping {}
                    },
                    Arc::new(CallbackStore::new()),
                    CancellationToken::new()
                )
                .await
                .unwrap_err()
                .downcast_ref::<Fault>(),
            Some(&Fault::ScopeExpired)
        );
        tokio::time::resume();
        active.endpoint.close().await;
        server.close().await;
    }

    #[tokio::test]
    async fn frame_errors_keep_protocol_faults_and_authorization_precedence() {
        for bytes in [
            vec![0],
            vec![0, 0, 0, 2, b'{'],
            vec![0; 4],
            vec![255; 4],
            vec![0, 0, 0, 1, b'{'],
        ] {
            let result = read_with_partial_deadline(&mut bytes.as_slice()).await;
            assert_eq!(
                receive_fault(result, false)
                    .unwrap_err()
                    .downcast_ref::<Fault>(),
                Some(&Fault::ProtocolError)
            );
        }
        for kind in [
            io::ErrorKind::UnexpectedEof,
            io::ErrorKind::InvalidData,
            io::ErrorKind::TimedOut,
        ] {
            assert_eq!(
                receive_fault(Err(io::Error::from(kind)), true)
                    .unwrap_err()
                    .downcast_ref::<Fault>(),
                Some(&Fault::NotAuthorized)
            );
        }
        assert_eq!(
            receive_fault(Ok(None), false)
                .unwrap_err()
                .downcast_ref::<Fault>(),
            Some(&Fault::NetworkUnavailable)
        );
        assert_eq!(
            receive_fault(Ok(None), true)
                .unwrap_err()
                .downcast_ref::<Fault>(),
            Some(&Fault::NotAuthorized)
        );
    }
}
