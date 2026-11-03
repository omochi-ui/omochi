//! Experimental browser transport only: no native hub or desktop SDK dependency.
use futures_util::lock::Mutex;
use iroh::{Endpoint, EndpointAddr, RelayMode, SecretKey, endpoint::presets};
use wasm_bindgen::prelude::*;

const MAX_FRAME: usize = 4 * 1024 * 1024;
fn js(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[wasm_bindgen]
pub struct BrowserEndpoint(Endpoint);

#[wasm_bindgen]
impl BrowserEndpoint {
    pub async fn create(secret: Option<String>, relay: String) -> Result<BrowserEndpoint, JsValue> {
        console_error_panic_hook::set_once();
        let key = match secret {
            Some(value) => {
                let bytes: [u8; 32] = hex::decode(value)
                    .map_err(js)?
                    .try_into()
                    .map_err(|_| js("invalid key length"))?;
                SecretKey::from_bytes(&bytes)
            }
            None => SecretKey::generate(),
        };
        let relay: iroh::RelayUrl = relay.parse().map_err(js)?;
        let endpoint = Endpoint::builder(presets::Minimal)
            .secret_key(key)
            .relay_mode(RelayMode::custom([relay]))
            .bind()
            .await
            .map_err(js)?;
        Ok(Self(endpoint))
    }
    pub fn id(&self) -> String {
        self.0.id().to_string()
    }
    // Exported only so this spike can test persistence. Never log this value.
    pub fn secret(&self) -> String {
        hex::encode(self.0.secret_key().to_bytes())
    }
    pub async fn connect(
        &self,
        id: String,
        relay: String,
        alpn: String,
    ) -> Result<GatewayConnection, JsValue> {
        let target =
            EndpointAddr::new(id.parse().map_err(js)?).with_relay_url(relay.parse().map_err(js)?);
        let connection = self.0.connect(target, alpn.as_bytes()).await.map_err(js)?;
        Ok(GatewayConnection(connection))
    }
    pub async fn close(&self) {
        self.0.close().await;
    }
}

#[wasm_bindgen]
pub struct GatewayConnection(iroh::endpoint::Connection);

#[wasm_bindgen]
impl GatewayConnection {
    pub fn remote_id(&self) -> String {
        self.0.remote_id().to_string()
    }
    pub fn relay_only(&self) -> bool {
        let paths = self.0.paths();
        paths.iter().any(|p| p.is_relay()) && paths.iter().all(|p| !p.is_ip())
    }
    pub async fn open(&self) -> Result<FrameStream, JsValue> {
        let (tx, rx) = self.0.open_bi().await.map_err(js)?;
        Ok(FrameStream {
            tx: Mutex::new(tx),
            rx: Mutex::new(rx),
        })
    }
    pub fn close(&self) {
        self.0.close(0u32.into(), b"spike disconnect");
    }
    pub async fn closed(&self) -> String {
        self.0.closed().await.to_string()
    }
}

#[wasm_bindgen]
pub struct FrameStream {
    tx: Mutex<iroh::endpoint::SendStream>,
    rx: Mutex<iroh::endpoint::RecvStream>,
}

#[wasm_bindgen]
impl FrameStream {
    pub async fn send(&self, body: String) -> Result<(), JsValue> {
        let bytes = body.as_bytes();
        if bytes.is_empty() || bytes.len() > MAX_FRAME {
            return Err(js("frame capacity"));
        }
        let mut tx = self.tx.lock().await;
        tx.write_all(&(bytes.len() as u32).to_be_bytes())
            .await
            .map_err(js)?;
        tx.write_all(bytes).await.map_err(js)?;
        Ok(())
    }
    pub async fn recv(&self) -> Result<Option<String>, JsValue> {
        let mut rx = self.rx.lock().await;
        let mut header = [0u8; 4];
        if rx.read(&mut header[..1]).await.map_err(js)?.is_none() {
            return Ok(None);
        }
        rx.read_exact(&mut header[1..]).await.map_err(js)?;
        let size = u32::from_be_bytes(header) as usize;
        if size == 0 || size > MAX_FRAME {
            return Err(js("frame capacity"));
        }
        let mut body = vec![0; size];
        rx.read_exact(&mut body).await.map_err(js)?;
        String::from_utf8(body).map(Some).map_err(js)
    }
}
