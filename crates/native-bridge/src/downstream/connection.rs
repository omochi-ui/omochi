//! QUIC, enrollment, timeouts and cancellation for the native adapter.
use super::*;
use omochi_downstream_core::RenderFrame;
use omochi_protocol::{read_frame, write_frame};
use omochi_transport::{Endpoint, EndpointAddr, EndpointConnection, RecvStream, SendStream};
use std::{future::Future, io, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
const IO: Duration = Duration::from_secs(10);

/// Dialing, opening the enrollment stream and exchanging authentication share
/// one deadline. A peer must not retain the identity lease using stream credit
/// or write backpressure after its authenticated QUIC connection is established.
async fn enrollment_exchange<W: AsyncWrite + Unpin, R: AsyncRead + Unpin>(
    open: impl Future<Output = Result<(W, R)>>,
    link: &Link,
    deadline: tokio::time::Instant,
) -> Result<()> {
    tokio::time::timeout_at(deadline, async {
        let (mut tx, mut rx) = open.await?;
        write_frame(
            &mut tx,
            &json!({"version":1,"grantId":link.grant_id,"secret":link.secret}),
        )
        .await?;
        let accepted: Value = read_frame(&mut rx)
            .await?
            .with_context(|| Fault::EnrollmentUnknown)?;
        anyhow::ensure!(accepted["accepted"] == true, Fault::EnrollmentRejected);
        Ok(())
    })
    .await?
}

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
                    Self::enroll_at(&endpoint, &link, link.address()?).await?;
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
    async fn enroll_at(endpoint: &Endpoint, link: &Link, address: EndpointAddr) -> Result<()> {
        let deadline = tokio::time::Instant::now() + IO;
        let alpn = if link.mode == Mode::Owner {
            b"omochi/owner-enroll/1".as_slice()
        } else {
            b"omochi/dashboard-enroll/1".as_slice()
        };
        let connection =
            tokio::time::timeout_at(deadline, endpoint.connect(address, alpn)).await??;
        let result =
            enrollment_exchange(async { Ok(connection.open_bi().await?) }, link, deadline).await;
        connection.close(0u32.into(), b"enrollment complete");
        result
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

    fn enrollment_link(mode: Mode) -> Link {
        Link {
            version: 1,
            hub_id: omochi_transport::SecretKey::generate().public().to_string(),
            relay_urls: vec!["https://relay.example/".into()],
            grant_id: uuid::Uuid::nil().to_string(),
            mode,
            secret: "cd".repeat(32),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn enrollment_stream_open_uses_the_remaining_dial_deadline() {
        let link = enrollment_link(Mode::Owner);
        let deadline = tokio::time::Instant::now() + IO;
        // Dialing has already spent part of the single authentication budget.
        tokio::time::advance(Duration::from_secs(3)).await;
        let (_release, opened) =
            tokio::sync::oneshot::channel::<(tokio::io::DuplexStream, tokio::io::DuplexStream)>();
        let exchange = enrollment_exchange(async { Ok(opened.await?) }, &link, deadline);
        tokio::pin!(exchange);
        assert!(poll_once(exchange.as_mut()).await.is_pending());
        tokio::time::advance(Duration::from_secs(6)).await;
        assert!(poll_once(exchange.as_mut()).await.is_pending());
        tokio::time::advance(Duration::from_secs(1)).await;
        let Poll::Ready(result) = poll_once(exchange.as_mut()).await else {
            panic!("lack of bidirectional stream credit must expire at the dial deadline");
        };
        assert_eq!(
            normalize_error(result.unwrap_err()).downcast_ref::<Fault>(),
            Some(&Fault::Timeout)
        );
        assert!(_release.is_closed());
    }

    #[tokio::test(start_paused = true)]
    async fn enrollment_write_backpressure_does_not_extend_the_open_deadline() {
        let link = enrollment_link(Mode::Invite);
        let deadline = tokio::time::Instant::now() + IO;
        let (release, opened) = tokio::sync::oneshot::channel();
        let exchange = enrollment_exchange(async { Ok(opened.await?) }, &link, deadline);
        tokio::pin!(exchange);
        assert!(poll_once(exchange.as_mut()).await.is_pending());
        tokio::time::advance(Duration::from_secs(8)).await;
        let (stream, mut peer) = tokio::io::duplex(1);
        let (rx, tx) = tokio::io::split(stream);
        release.send((tx, rx)).unwrap();
        assert!(poll_once(exchange.as_mut()).await.is_pending());
        // The one-byte window lets the request start but prevents write_frame
        // from finishing. A trickle of new credit must not reset the deadline.
        assert_eq!(peer.read_u8().await.unwrap(), 0);
        assert!(poll_once(exchange.as_mut()).await.is_pending());
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(peer.read_u8().await.unwrap(), 0);
        assert!(poll_once(exchange.as_mut()).await.is_pending());
        tokio::time::advance(Duration::from_secs(1)).await;
        let Poll::Ready(result) = poll_once(exchange.as_mut()).await else {
            panic!("enrollment writes must share the stream-open deadline");
        };
        assert_eq!(
            normalize_error(result.unwrap_err()).downcast_ref::<Fault>(),
            Some(&Fault::Timeout)
        );
        // Cancellation drops the write half, so the peer gets EOF rather than
        // retaining a stream that can later complete enrollment.
        assert_eq!(peer.read_u8().await.unwrap(), 0);
        assert_eq!(peer.read(&mut [0]).await.unwrap(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn enrollment_reply_cannot_start_a_fresh_deadline_after_open_and_write() {
        let link = enrollment_link(Mode::Owner);
        let deadline = tokio::time::Instant::now() + IO;
        tokio::time::advance(Duration::from_secs(8)).await;
        let (stream, mut peer) = tokio::io::duplex(1024);
        let (rx, tx) = tokio::io::split(stream);
        let exchange = enrollment_exchange(async { Ok((tx, rx)) }, &link, deadline);
        tokio::pin!(exchange);
        assert!(poll_once(exchange.as_mut()).await.is_pending());
        let request: Value = read_frame(&mut peer).await.unwrap().unwrap();
        assert_eq!(
            request,
            json!({"version":1,"grantId":link.grant_id,"secret":link.secret})
        );
        tokio::time::advance(Duration::from_secs(1)).await;
        // A partial reply also cannot reset the remaining authentication time.
        peer.write_all(&[0]).await.unwrap();
        assert!(poll_once(exchange.as_mut()).await.is_pending());
        tokio::time::advance(Duration::from_secs(1)).await;
        let Poll::Ready(result) = poll_once(exchange.as_mut()).await else {
            panic!("the enrollment reply must finish within the original deadline");
        };
        assert_eq!(
            normalize_error(result.unwrap_err()).downcast_ref::<Fault>(),
            Some(&Fault::Timeout)
        );
    }

    #[tokio::test]
    async fn enrollment_alpns_accept_reject_and_close_the_exact_identity_connection() {
        tokio::time::timeout(IO, async {
            for mode in [Mode::Owner, Mode::Invite] {
                for accepted in [true, false] {
                    let server =
                        omochi_transport::bind(omochi_transport::SecretKey::generate(), true, None)
                            .await
                            .unwrap();
                    server.set_alpns(vec![if mode == Mode::Owner {
                        b"omochi/owner-enroll/1".to_vec()
                    } else {
                        b"omochi/dashboard-enroll/1".to_vec()
                    }]);
                    let endpoint =
                        omochi_transport::bind(omochi_transport::SecretKey::generate(), true, None)
                            .await
                            .unwrap();
                    let link = Link {
                        hub_id: server.id().to_string(),
                        ..enrollment_link(mode)
                    };
                    let (result, ()) =
                        tokio::join!(Client::enroll_at(&endpoint, &link, server.addr()), async {
                            let connection = server.accept().await.unwrap().await.unwrap();
                            assert_eq!(connection.remote_id(), endpoint.id());
                            let (mut tx, mut rx) = connection.accept_bi().await.unwrap();
                            let request: Value = read_frame(&mut rx).await.unwrap().unwrap();
                            assert_eq!(
                                request,
                                json!({"version":1,"grantId":link.grant_id,"secret":link.secret})
                            );
                            write_frame(&mut tx, &json!({"accepted":accepted}))
                                .await
                                .unwrap();
                            tx.finish().unwrap();
                            connection.closed().await;
                        });
                    if accepted {
                        result.unwrap();
                    } else {
                        assert_eq!(
                            result.unwrap_err().downcast_ref::<Fault>(),
                            Some(&Fault::EnrollmentRejected)
                        );
                    }
                    endpoint.close().await;
                    server.close().await;
                }
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn enrollment_timeout_preserves_the_pending_identity_for_ordinary_recovery() {
        tokio::time::timeout(IO * 3, async {
            let server =
                omochi_transport::bind(omochi_transport::SecretKey::generate(), true, None)
                    .await
                    .unwrap();
            server.set_alpns(vec![
                b"omochi/owner-enroll/1".to_vec(),
                b"omochi/owner/1".to_vec(),
            ]);
            let link = Link {
                hub_id: server.id().to_string(),
                ..enrollment_link(Mode::Owner)
            };
            let store = Arc::new(CallbackStore::new());
            let client = Client::default();
            let pending_raw;
            // Exercise actual registration persistence, stopping at the session
            // lock immediately before transport binding. No relay is required.
            let active = client.active.write().await;
            {
                let registering = client.connect(
                    Command::Register {
                        invitation: format!(
                            "omochi://owner/{}",
                            base64::engine::general_purpose::URL_SAFE_NO_PAD
                                .encode(serde_json::to_vec(&link).unwrap())
                        ),
                        save: true,
                    },
                    store.clone(),
                );
                tokio::pin!(registering);
                assert!(poll_once(registering.as_mut()).await.is_pending());
                let read: Value =
                    serde_json::from_str(&store.requests.lock().unwrap().pop_front().unwrap())
                        .unwrap();
                assert_eq!(read["op"], "read");
                assert!(store.reply(
                    read["id"].as_u64().unwrap(),
                    json!({"ok":true,"value":null})
                ));
                assert!(poll_once(registering.as_mut()).await.is_pending());
                let commit: Value =
                    serde_json::from_str(&store.requests.lock().unwrap().pop_front().unwrap())
                        .unwrap();
                assert_eq!(commit["op"], "commit");
                assert_eq!(commit["expected"], Value::Null);
                pending_raw = commit["replacement"].as_str().unwrap().to_owned();
                assert!(store.reply(
                    commit["id"].as_u64().unwrap(),
                    json!({"outcome":"committed"})
                ));
                assert!(poll_once(registering.as_mut()).await.is_pending());
            }
            drop(active);
            let profile: Profile = serde_json::from_str(&pending_raw).unwrap();
            let key = profile.key().unwrap();
            assert_eq!(profile.records.len(), 1);
            assert!(!profile.records[0].accepted);
            assert_eq!(profile.records[0].link.id(), link.id());
            assert_eq!(profile.records[0].link.secret, "0".repeat(64));

            let endpoint = omochi_transport::bind(key.clone(), true, None)
                .await
                .unwrap();
            let registering = tokio::spawn({
                let endpoint = endpoint.clone();
                let link = link.clone();
                let address = server.addr();
                async move { Client::enroll_at(&endpoint, &link, address).await }
            });
            let peer = server.accept().await.unwrap().await.unwrap();
            assert_eq!(peer.remote_id(), key.public());
            let (_reply, mut request) = peer.accept_bi().await.unwrap();
            let request: Value = read_frame(&mut request).await.unwrap().unwrap();
            assert_eq!(request["secret"], link.secret);
            // The hub may already have bound the grant when its reply stalls.
            // Cross the timer tick as well as the remaining deadline.
            tokio::time::pause();
            tokio::time::advance(IO + Duration::from_millis(1)).await;
            let error = registering.await.unwrap().unwrap_err();
            assert_eq!(
                normalize_error(error).downcast_ref::<Fault>(),
                Some(&Fault::Timeout)
            );
            tokio::time::resume();
            peer.closed().await;
            endpoint.close().await;
            assert!(store.requests.lock().unwrap().is_empty());
            assert!(store.pending.lock().unwrap().is_empty());

            // Reload exactly the committed pending profile after the timeout.
            let (recovered, _) = {
                let loading = Profile::load(&store);
                tokio::pin!(loading);
                assert!(poll_once(loading.as_mut()).await.is_pending());
                let read: Value =
                    serde_json::from_str(&store.requests.lock().unwrap().pop_front().unwrap())
                        .unwrap();
                assert!(store.reply(
                    read["id"].as_u64().unwrap(),
                    json!({"ok":true,"value":pending_raw})
                ));
                let Poll::Ready(loaded) = poll_once(loading.as_mut()).await else {
                    panic!("the pending registration must remain readable after timeout");
                };
                loaded.unwrap()
            };
            assert!(!recovered.records[0].accepted);
            let endpoint = omochi_transport::bind(recovered.key().unwrap(), true, None)
                .await
                .unwrap();
            let (opened, peer) = tokio::join!(
                Client::open_at(&endpoint, &recovered.records[0].link, server.addr()),
                async {
                    let connection = server.accept().await.unwrap().await.unwrap();
                    assert_eq!(connection.remote_id(), key.public());
                    let (mut tx, mut rx) = connection.accept_bi().await.unwrap();
                    let hello: Value = read_frame(&mut rx).await.unwrap().unwrap();
                    assert_eq!(hello, json!({"version":1,"grantId":link.grant_id}));
                    write_frame(&mut tx, &json!({"kind":"ownerWelcome","services":[]}))
                        .await
                        .unwrap();
                    (connection, tx, rx)
                }
            );
            let (connection, _, _, _) = opened.unwrap();
            connection.close(0u32.into(), b"recovered");
            peer.0.closed().await;
            endpoint.close().await;
            server.close().await;
        })
        .await
        .unwrap();
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
