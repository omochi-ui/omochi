//! Small, standalone browser bridge. Native tests compile only pure wire logic.
#[cfg(target_arch = "wasm32")]
#[path = "../../../crates/transport/src/relay.rs"]
mod relay;

#[cfg(any(test, target_arch = "wasm32"))]
mod wire {
    use std::{cell::Cell, rc::Rc};

    pub const MAX_FRAME: usize = 4 * 1024 * 1024;
    const FRAME_POOL: usize = 8 * 1024 * 1024;
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Fault {
        ProtocolError,
        CapacityExceeded,
        Cancelled,
        Timeout,
        StorageCorrupt,
        NetworkUnavailable,
    }
    impl Fault {
        pub fn code(self) -> &'static str {
            match self {
                Self::ProtocolError => "ProtocolError",
                Self::CapacityExceeded => "CapacityExceeded",
                Self::Cancelled => "Cancelled",
                Self::Timeout => "Timeout",
                Self::StorageCorrupt => "StorageCorrupt",
                Self::NetworkUnavailable => "NetworkUnavailable",
            }
        }
    }
    pub fn secret_bytes(text: &str) -> Result<[u8; 32], Fault> {
        if !node_id(text) {
            return Err(Fault::StorageCorrupt);
        }
        hex::decode(text)
            .map_err(|_| Fault::StorageCorrupt)?
            .try_into()
            .map_err(|_| Fault::StorageCorrupt)
    }
    pub fn node_id(text: &str) -> bool {
        text.len() == 64
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }
    pub fn frame_size(header: [u8; 4]) -> Result<usize, Fault> {
        let size = u32::from_be_bytes(header) as usize;
        if size == 0 {
            Err(Fault::ProtocolError)
        } else if size > MAX_FRAME {
            Err(Fault::CapacityExceeded)
        } else {
            Ok(size)
        }
    }
    #[derive(Clone, Default)]
    pub struct FramePool(Rc<Cell<usize>>);
    pub struct FrameCharge {
        pool: FramePool,
        bytes: usize,
    }
    impl FramePool {
        pub fn reserve(&self, bytes: usize) -> Result<FrameCharge, Fault> {
            if bytes > MAX_FRAME || bytes > FRAME_POOL - self.0.get() {
                return Err(Fault::CapacityExceeded);
            }
            self.0.set(self.0.get() + bytes);
            Ok(FrameCharge {
                pool: self.clone(),
                bytes,
            })
        }
        pub fn used(&self) -> usize {
            self.0.get()
        }
    }
    impl Drop for FrameCharge {
        fn drop(&mut self) {
            self.pool.0.set(self.pool.0.get() - self.bytes);
        }
    }
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Kind {
        Stream,
    }
    pub fn open_header(json: &str) -> Result<(Kind, Option<String>), Fault> {
        if json != r#"{"kind":"stream"}"# {
            return Err(Fault::ProtocolError);
        }
        Ok((Kind::Stream, None))
    }
    pub fn normalize(_kind: Kind, body: &[u8]) -> Result<String, Fault> {
        let value: serde_json::Value =
            serde_json::from_slice(body).map_err(|_| Fault::ProtocolError)?;
        if !value.is_object() {
            return Err(Fault::ProtocolError);
        }
        String::from_utf8(body.to_vec()).map_err(|_| Fault::ProtocolError)
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn bounded_frames_and_streams() {
            let pool = FramePool::default();
            let a = pool.reserve(MAX_FRAME).unwrap();
            let b = pool.reserve(MAX_FRAME).unwrap();
            assert!(pool.reserve(1).is_err());
            drop(a);
            drop(b);
            assert_eq!(pool.used(), 0);
            assert!(frame_size([0; 4]).is_err());
            assert!(open_header(r#"{"kind":"control","protocol":4}"#).is_err());
            assert!(open_header(r#"{"kind":"stream"}"#).unwrap().1.is_none());
            assert!(
                normalize(Kind::Stream, br#"{"sequence":"18446744073709551615"}"#)
                    .unwrap()
                    .contains("18446744073709551615")
            );
        }
        #[test]
        fn identity_validation_is_pure() {
            for error in [Fault::Cancelled, Fault::Timeout, Fault::NetworkUnavailable] {
                assert!(!error.code().is_empty());
            }
            assert_eq!(secret_bytes(&"0".repeat(64)).unwrap(), [0; 32]);
            assert!(secret_bytes(&"A".repeat(64)).is_err());
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::wire::{self, Fault, FramePool, Kind};
    use futures_util::{
        future::{AbortHandle, Abortable, Either, select},
        lock::Mutex,
    };
    use gloo_timers::future::TimeoutFuture;
    use iroh::{
        Endpoint, EndpointAddr, RelayMode, SecretKey,
        endpoint::{self, presets},
    };
    use js_sys::Promise;
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
        future::Future,
        rc::Rc,
    };
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::future_to_promise;

    fn js(error: Fault) -> JsValue {
        JsValue::from_str(error.code())
    }
    fn rejected(error: Fault) -> Promise {
        Promise::reject(&js(error))
    }
    async fn deadline<T>(
        future: impl Future<Output = Result<T, Fault>>,
        ms: u32,
    ) -> Result<T, Fault> {
        let expires = js_sys::Date::now() + f64::from(ms);
        futures_util::pin_mut!(future);
        let timer = TimeoutFuture::new(ms);
        futures_util::pin_mut!(timer);
        match select(future, timer).await {
            Either::Left((result, _)) => {
                // A resumed page may poll queued I/O before an elapsed timer callback.
                if js_sys::Date::now() >= expires {
                    Err(Fault::Timeout)
                } else {
                    result
                }
            }
            Either::Right(_) => Err(Fault::Timeout),
        }
    }
    fn identity(key: SecretKey) -> String {
        serde_json::json!({"secretHex":hex::encode(key.to_bytes()),"endpointId":key.public().to_string()}).to_string()
    }
    #[wasm_bindgen]
    pub fn generate_identity() -> String {
        identity(SecretKey::generate())
    }
    #[wasm_bindgen]
    pub fn identity_from_secret(secret: &str) -> Result<String, JsValue> {
        Ok(identity(SecretKey::from_bytes(
            &wire::secret_bytes(secret).map_err(js)?,
        )))
    }
    #[wasm_bindgen]
    pub struct RawEndpoint {
        endpoint: Endpoint,
        pool: FramePool,
        dead: Rc<Cell<bool>>,
        connecting: Rc<RefCell<BTreeMap<u32, AbortHandle>>>,
    }
    #[wasm_bindgen]
    impl RawEndpoint {
        pub async fn create(secret: String, relays: String) -> Result<RawEndpoint, JsValue> {
            let key = SecretKey::from_bytes(&wire::secret_bytes(&secret).map_err(js)?);
            let relays: Vec<String> =
                serde_json::from_str(&relays).map_err(|_| js(Fault::ProtocolError))?;
            if relays.len() > 8 {
                return Err(js(Fault::CapacityExceeded));
            }
            let relays = relays
                .into_iter()
                .map(|url| {
                    let parsed: iroh::RelayUrl =
                        url.parse().map_err(|_| js(Fault::ProtocolError))?;
                    if url.len() > 2048
                        || parsed.scheme() != "https"
                        || !parsed.username().is_empty()
                        || parsed.password().is_some()
                        || parsed.query().is_some()
                        || parsed.fragment().is_some()
                    {
                        return Err(js(Fault::ProtocolError));
                    }
                    Ok(parsed)
                })
                .collect::<Result<Vec<_>, JsValue>>()?;
            let endpoint = Endpoint::builder(presets::Minimal)
                .secret_key(key)
                .relay_mode(if relays.is_empty() {
                    RelayMode::Disabled
                } else {
                    RelayMode::Custom(relays.into_iter().map(super::relay::relay_config).collect())
                })
                .bind()
                .await
                .map_err(|_| js(Fault::NetworkUnavailable))?;
            Ok(Self {
                endpoint,
                pool: FramePool::default(),
                dead: Rc::new(Cell::new(false)),
                connecting: Rc::default(),
            })
        }
        /// Register before returning the JS promise, so an immediate abort cannot be lost.
        pub fn connect(&self, address: String, alpn: String, operation: u32) -> Promise {
            if self.dead.get() {
                return rejected(Fault::Cancelled);
            }
            if ![
                "omochi/dashboard-enroll/1",
                "omochi/dashboard/1",
                "omochi/owner/1",
                "omochi/owner-enroll/1",
            ]
            .contains(&alpn.as_str())
            {
                return rejected(Fault::ProtocolError);
            }
            let mut address: EndpointAddr = match serde_json::from_str(&address) {
                Ok(a) => a,
                Err(_) => return rejected(Fault::ProtocolError),
            };
            address.addrs.retain(|addr| addr.is_relay());
            let relays: Vec<_> = address.relay_urls().collect();
            if relays.is_empty()
                || relays.len() > 4
                || relays.iter().any(|url| {
                    url.as_str().len() > 2048
                        || url.scheme() != "https"
                        || !url.username().is_empty()
                        || url.password().is_some()
                        || url.query().is_some()
                        || url.fragment().is_some()
                })
            {
                return rejected(Fault::ProtocolError);
            }
            if self.connecting.borrow().contains_key(&operation) {
                return rejected(Fault::ProtocolError);
            }
            let (handle, registration) = AbortHandle::new_pair();
            self.connecting.borrow_mut().insert(operation, handle);
            let endpoint = self.endpoint.clone();
            let pool = self.pool.clone();
            let tasks = self.connecting.clone();
            let dead = self.dead.clone();
            future_to_promise(async move {
                let result =
                    Abortable::new(endpoint.connect(address, alpn.as_bytes()), registration).await;
                tasks.borrow_mut().remove(&operation);
                let connection = result
                    .map_err(|_| js(Fault::Cancelled))?
                    .map_err(|_| js(Fault::NetworkUnavailable))?;
                if dead.get() {
                    connection.close(0u32.into(), b"closed");
                    return Err(js(Fault::Cancelled));
                }
                Ok(RawConnection {
                    connection,
                    pool,
                    dead: Rc::new(Cell::new(false)),
                    opening: Rc::default(),
                }
                .into())
            })
        }
        pub fn cancel_connect(&self, operation: u32) {
            if let Some(handle) = self.connecting.borrow().get(&operation) {
                handle.abort();
            }
        }
        pub async fn close(&self) {
            self.dead.set(true);
            for handle in self.connecting.borrow().values() {
                handle.abort();
            }
            self.endpoint.close().await;
        }
        pub fn frame_bytes(&self) -> usize {
            self.pool.used()
        }
    }
    #[wasm_bindgen]
    pub struct RawConnection {
        connection: endpoint::Connection,
        pool: FramePool,
        dead: Rc<Cell<bool>>,
        opening: Rc<RefCell<BTreeMap<u32, AbortHandle>>>,
    }
    #[wasm_bindgen]
    impl RawConnection {
        pub fn remote_id(&self) -> String {
            self.connection.remote_id().to_string()
        }
        pub fn open(&self, header: String, operation: u32) -> Promise {
            if self.dead.get() {
                return rejected(Fault::Cancelled);
            }
            let (kind, header) = match wire::open_header(&header) {
                Ok(open) => open,
                Err(error) => return rejected(error),
            };
            if self.opening.borrow().contains_key(&operation) {
                return rejected(Fault::ProtocolError);
            }
            let (handle, registration) = AbortHandle::new_pair();
            self.opening.borrow_mut().insert(operation, handle);
            let connection = self.connection.clone();
            let pool = self.pool.clone();
            let tasks = self.opening.clone();
            let dead = self.dead.clone();
            let candidate: Rc<RefCell<Option<Rc<FrameState>>>> = Rc::default();
            let pending = candidate.clone();
            future_to_promise(async move {
                let opening = async move {
                    let (tx, rx) = connection
                        .open_bi()
                        .await
                        .map_err(|_| Fault::NetworkUnavailable)?;
                    let state = Rc::new(FrameState {
                        tx: Mutex::new(tx),
                        rx: Mutex::new(rx),
                        dead: Cell::new(false),
                        sending: RefCell::new(None),
                        receiving: RefCell::new(None),
                        pool,
                        kind,
                        finished: Cell::new(false),
                    });
                    *pending.borrow_mut() = Some(state.clone());
                    if let Some(header) = header {
                        deadline(write(&state, header), 5000).await?;
                    }
                    Ok::<_, Fault>(state)
                };
                let result = Abortable::new(opening, registration)
                    .await
                    .unwrap_or(Err(Fault::Cancelled));
                tasks.borrow_mut().remove(&operation);
                if let Err(error) = result {
                    let state = candidate.borrow_mut().take();
                    if let Some(state) = state {
                        cancel(&state).await;
                    }
                    return Err(js(error));
                }
                let state = result.unwrap();
                if dead.get() {
                    cancel(&state).await;
                    return Err(js(Fault::Cancelled));
                }
                Ok(RawStream { state }.into())
            })
        }
        pub fn cancel_open(&self, operation: u32) {
            if let Some(handle) = self.opening.borrow().get(&operation) {
                handle.abort();
            }
        }
        pub fn close(&self) {
            self.dead.set(true);
            for handle in self.opening.borrow().values() {
                handle.abort();
            }
            self.connection.close(0u32.into(), b"browser closed");
        }
        pub async fn closed(&self) -> String {
            let error = self.connection.closed().await;
            let code = match error {
                endpoint::ConnectionError::ApplicationClosed(close) => close.error_code.to_string(),
                _ => "0".into(),
            };
            serde_json::json!({"code":code,"reason":if code == "1" { "Not authorized" } else { "Connection closed" }}).to_string()
        }
    }
    struct FrameState {
        tx: Mutex<endpoint::SendStream>,
        rx: Mutex<endpoint::RecvStream>,
        dead: Cell<bool>,
        finished: Cell<bool>,
        sending: RefCell<Option<AbortHandle>>,
        receiving: RefCell<Option<AbortHandle>>,
        pool: FramePool,
        kind: Kind,
    }
    async fn write(state: &FrameState, json: String) -> Result<(), Fault> {
        let bytes = json.as_bytes();
        wire::frame_size((bytes.len() as u32).to_be_bytes())?;
        let mut tx = state.tx.lock().await;
        tx.write_all(&(bytes.len() as u32).to_be_bytes())
            .await
            .map_err(|_| Fault::NetworkUnavailable)?;
        tx.write_all(bytes)
            .await
            .map_err(|_| Fault::NetworkUnavailable)
    }
    async fn cancel(state: &FrameState) {
        state.dead.set(true);
        if let Some(handle) = state.sending.borrow().as_ref() {
            handle.abort();
        }
        if let Some(handle) = state.receiving.borrow().as_ref() {
            handle.abort();
        }
        // Abortable drops its I/O future before these mutexes become available.
        let _ = state.tx.lock().await.reset(0u32.into());
        let _ = state.rx.lock().await.stop(0u32.into());
    }
    #[wasm_bindgen]
    pub struct RawStream {
        state: Rc<FrameState>,
    }
    #[wasm_bindgen]
    impl RawStream {
        pub fn send(&self, json: String) -> Promise {
            if self.state.dead.get() || self.state.finished.get() {
                return rejected(Fault::Cancelled);
            }
            if self.state.sending.borrow().is_some() {
                return rejected(Fault::ProtocolError);
            }
            if json.is_empty() {
                return rejected(Fault::ProtocolError);
            }
            if json.len() > wire::MAX_FRAME {
                return rejected(Fault::CapacityExceeded);
            }
            let (handle, registration) = AbortHandle::new_pair();
            *self.state.sending.borrow_mut() = Some(handle);
            let state = self.state.clone();
            future_to_promise(async move {
                let result = Abortable::new(deadline(write(&state, json), 5000), registration)
                    .await
                    .unwrap_or(Err(Fault::Cancelled));
                state.sending.borrow_mut().take();
                match result {
                    Ok(()) => Ok(JsValue::UNDEFINED),
                    Err(error) => {
                        cancel(&state).await;
                        Err(js(error))
                    }
                }
            })
        }
        pub fn receive(&self) -> Promise {
            if self.state.dead.get() {
                return rejected(Fault::Cancelled);
            }
            if self.state.receiving.borrow().is_some() {
                return rejected(Fault::ProtocolError);
            }
            let (handle, registration) = AbortHandle::new_pair();
            *self.state.receiving.borrow_mut() = Some(handle);
            let state = self.state.clone();
            future_to_promise(async move {
                let read = async {
                    let mut rx = state.rx.lock().await;
                    let mut header = [0u8; 4];
                    // Healthy quiet services have no idle-read deadline.
                    if rx
                        .read(&mut header[..1])
                        .await
                        .map_err(|_| Fault::NetworkUnavailable)?
                        .is_none()
                    {
                        return Ok(None);
                    }
                    deadline(
                        async {
                            rx.read_exact(&mut header[1..])
                                .await
                                .map_err(|_| Fault::ProtocolError)?;
                            let size = wire::frame_size(header)?;
                            let _charge = state.pool.reserve(size)?;
                            let mut body = vec![0; size];
                            rx.read_exact(&mut body)
                                .await
                                .map_err(|_| Fault::ProtocolError)?;
                            let json = wire::normalize(state.kind, &body)?;
                            Ok(Some(
                                serde_json::json!({"json":json,"wireBytes":size}).to_string(),
                            ))
                        },
                        5000,
                    )
                    .await
                };
                let result = Abortable::new(read, registration)
                    .await
                    .unwrap_or(Err(Fault::Cancelled));
                state.receiving.borrow_mut().take();
                match result {
                    Ok(Some(frame)) => Ok(JsValue::from_str(&frame)),
                    Ok(None) => Ok(JsValue::NULL),
                    Err(error) => {
                        cancel(&state).await;
                        Err(js(error))
                    }
                }
            })
        }
        pub async fn finish(&self) -> Result<(), JsValue> {
            if self.state.dead.get() {
                return Err(js(Fault::Cancelled));
            }
            if self.state.sending.borrow().is_some() {
                return Err(js(Fault::ProtocolError));
            }
            self.state.finished.set(true);
            self.state
                .tx
                .lock()
                .await
                .finish()
                .map_err(|_| js(Fault::NetworkUnavailable))
        }
        pub fn cancel(&self) -> Promise {
            self.state.dead.set(true);
            if let Some(handle) = self.state.sending.borrow().as_ref() {
                handle.abort();
            }
            if let Some(handle) = self.state.receiving.borrow().as_ref() {
                handle.abort();
            }
            let state = self.state.clone();
            future_to_promise(async move {
                cancel(&state).await;
                Ok(JsValue::UNDEFINED)
            })
        }
    }
}
