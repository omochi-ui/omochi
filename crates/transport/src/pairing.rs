//! One-shot enrollment over a separate, authenticated iroh ALPN.
#[cfg(feature = "desktop")]
use crate::store::{Role, Store};
use crate::{Endpoint, EndpointAddr, EndpointId};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
#[cfg(feature = "desktop")]
use omochi_protocol::read_frame_limited;
use omochi_protocol::{PAIR_ALPN, VERSION, read_frame, write_frame};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PREFIX: &str = "omochi://invite/";
const MAX_INVITE_BYTES: usize = 16 * 1024;
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// Deliberately no Debug: the invitation contains a bearer secret.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invite {
    version: u32,
    address: EndpointAddr,
    secret: String,
    expires: u64,
}
impl Invite {
    #[cfg(feature = "desktop")]
    pub fn issue(store: &Store, endpoint: &Endpoint) -> Result<Self> {
        if store.identity(Role::Gateway)?.public() != endpoint.id() {
            bail!("gateway identity mismatch")
        }
        let now = now();
        Ok(Self {
            version: VERSION,
            address: endpoint.addr(),
            secret: store.invite(now, 180)?,
            expires: now + 180,
        })
    }
    pub fn encode(&self) -> Result<String> {
        Ok(format!(
            "{PREFIX}{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(self)?)
        ))
    }
    pub fn gateway_id(&self) -> EndpointId {
        self.address.id
    }
    pub fn decode(text: &str) -> Result<Self> {
        let text = text.trim();
        if text.len() > MAX_INVITE_BYTES {
            bail!("invitation too large")
        }
        let payload = text
            .strip_prefix(PREFIX)
            .or_else(|| {
                let (base, payload) = text.split_once("#g:")?;
                let url = base.parse::<iroh::RelayUrl>().ok()?;
                (url.scheme() == "https"
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none())
                .then_some(payload)
            })
            .context("invalid invitation link")?;
        let bytes = URL_SAFE_NO_PAD
            .decode(payload)
            .context("invalid invitation encoding")?;
        let invite: Self =
            serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("invalid invitation"))?;
        if invite.version != VERSION
            || invite.secret.len() != 64
            || !invite.secret.bytes().all(|b| b.is_ascii_hexdigit())
        {
            bail!("unsupported or invalid invitation")
        }
        if invite.expires <= now() {
            bail!("invitation expired")
        }
        Ok(invite)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    secret: String,
    name: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    accepted: bool,
}

/// Caller bounds concurrent connections and dispatches only the pairing ALPN here.
#[cfg(feature = "desktop")]
pub async fn accept(connection: iroh::endpoint::Connection, store: Store) -> Result<()> {
    if connection.alpn() != PAIR_ALPN {
        bail!("incorrect enrollment protocol")
    }
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let peer = connection.remote_id();
        let (mut tx, mut rx) = connection.accept_bi().await?;
        let request: Request = read_frame_limited(&mut rx, 4096)
            .await?
            .context("missing enrollment request")?;
        if request.version != VERSION {
            bail!("unsupported protocol")
        }
        let accepted = tokio::task::spawn_blocking(move || {
            store.redeem(&request.secret, peer, &request.name, now())
        })
        .await?
        .is_ok();
        write_frame(&mut tx, &Response { accepted }).await?;
        tx.finish()?;
        let _ = tx.stopped().await;
        Ok::<_, anyhow::Error>(())
    })
    .await;
    connection.close(0u32.into(), b"enrollment finished");
    result??;
    Ok(())
}

/// A failed exchange may have enrolled this identity remotely. Never infer local trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrollmentError {
    Invalid,
    ConnectFailed,
    Rejected,
    OutcomeUnknown,
}
impl std::fmt::Display for EnrollmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid enrollment request",
            Self::ConnectFailed => "could not connect to gateway",
            Self::Rejected => "gateway rejected invitation",
            Self::OutcomeUnknown => {
                "gateway enrollment outcome is unknown; retry with the same identity"
            }
        })
    }
}
impl std::error::Error for EnrollmentError {}

struct EnrollmentConnection(iroh::endpoint::Connection);
impl Drop for EnrollmentConnection {
    fn drop(&mut self) {
        self.0.close(0u32.into(), b"enrollment finished");
    }
}

/// Perform only authenticated enrollment. The caller must durably save the returned
/// address before enabling ordinary traffic. Dropping the future closes the connection.
pub async fn enroll(
    endpoint: &Endpoint,
    invite: Invite,
    hub_name: &str,
) -> std::result::Result<EndpointAddr, EnrollmentError> {
    if invite.expires <= now()
        || invite.address.id == endpoint.id()
        || hub_name.trim().is_empty()
        || hub_name.len() > 256
    {
        return Err(EnrollmentError::Invalid);
    }
    let connection = tokio::time::timeout(
        Duration::from_secs(30),
        endpoint.connect(invite.address.clone(), PAIR_ALPN),
    )
    .await
    .map_err(|_| EnrollmentError::ConnectFailed)?
    .map_err(|_| EnrollmentError::ConnectFailed)?;
    let connection = EnrollmentConnection(connection);
    if connection.0.remote_id() != invite.address.id {
        return Err(EnrollmentError::Invalid);
    }
    let accepted = tokio::time::timeout(Duration::from_secs(10), async {
        let (mut tx, mut rx) = connection.0.open_bi().await?;
        write_frame(
            &mut tx,
            &Request {
                version: VERSION,
                secret: invite.secret,
                name: hub_name.into(),
            },
        )
        .await?;
        tx.finish()?;
        let response: Response = read_frame(&mut rx).await?.context("missing response")?;
        Ok::<_, anyhow::Error>(response.accepted)
    })
    .await
    .map_err(|_| EnrollmentError::OutcomeUnknown)?
    .map_err(|_| EnrollmentError::OutcomeUnknown)?;
    if !accepted {
        return Err(EnrollmentError::Rejected);
    }
    Ok(invite.address)
}

#[cfg(feature = "desktop")]
pub async fn redeem(
    endpoint: &Endpoint,
    store: Store,
    invite: Invite,
    gateway_name: &str,
    hub_name: &str,
) -> Result<()> {
    // Validate the local identity before granting it access remotely.
    if store.identity(Role::Hub)?.public() != endpoint.id() {
        bail!("hub identity mismatch")
    }
    if gateway_name.trim().is_empty() || gateway_name.len() > 256 {
        bail!("invalid gateway name")
    }
    let address = enroll(endpoint, invite, hub_name).await?;
    let name = gateway_name.to_owned();
    tokio::task::spawn_blocking(move || store.register_gateway(address.id, &name))
        .await?
        .context("gateway accepted enrollment, but local registration could not be saved")?;
    Ok(())
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    use super::*;
    #[test]
    fn shared_invitation_fixtures() {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/gateway-invitations.json"
        ))
        .unwrap();
        for case in fixtures["cases"].as_array().unwrap() {
            assert_eq!(
                Invite::decode(case["input"].as_str().unwrap()).is_ok(),
                case["valid"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
    #[tokio::test]
    async fn enrollment_binds_both_authenticated_identities() {
        let directory = tempfile::tempdir().unwrap();
        let gs = Store::open(directory.path().join("gateway")).unwrap();
        let hs = Store::open(directory.path().join("hub")).unwrap();
        let gateway = crate::bind(gs.identity(Role::Gateway).unwrap(), true, None)
            .await
            .unwrap();
        let hub = crate::bind(hs.identity(Role::Hub).unwrap(), true, None)
            .await
            .unwrap();
        let issued = Invite::issue(&gs, &gateway).unwrap();
        let text = issued.encode().unwrap();
        let payload = text.strip_prefix(PREFIX).unwrap();
        assert_eq!(
            URL_SAFE_NO_PAD.decode(payload).unwrap(),
            serde_json::to_vec(&issued).unwrap()
        );
        assert_eq!(
            Invite::decode(&format!("https://app.omochi.fyi/#g:{payload}"))
                .unwrap()
                .gateway_id(),
            gateway.id()
        );
        assert!(
            Invite::decode(&format!(
                "{PREFIX}{}",
                hex::encode(serde_json::to_vec(&issued).unwrap())
            ))
            .is_err()
        );
        assert!(Invite::decode(&format!("{PREFIX}{payload}=")).is_err());
        let invite = Invite::decode(&text).unwrap();
        let gid = gateway.id();
        let hid = hub.id();
        let copy = gs.clone();
        let server = tokio::spawn(async move {
            let connection = gateway.accept().await.unwrap().await.unwrap();
            accept(connection, copy).await.unwrap();
            gateway.close().await;
        });
        redeem(&hub, hs.clone(), invite, "workstation", "laptop")
            .await
            .unwrap();
        server.await.unwrap();
        hub.close().await;
        assert_eq!(gs.peers().unwrap().1[&hid.to_string()].role, Role::Hub);
        assert_eq!(hs.peers().unwrap().1[&gid.to_string()].role, Role::Gateway);
    }
}
