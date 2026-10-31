//! One-shot enrollment over a separate, authenticated iroh ALPN.
use crate::{
    Endpoint, EndpointAddr,
    store::{Role, Store},
};
use anyhow::{Context, Result, bail};
use omochi_protocol::{PAIR_ALPN, VERSION, read_frame, read_frame_limited, write_frame};
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
            hex::encode(serde_json::to_vec(self)?)
        ))
    }
    pub fn decode(text: &str) -> Result<Self> {
        let text = text.trim();
        if text.len() > MAX_INVITE_BYTES {
            bail!("invitation too large")
        }
        let bytes = hex::decode(
            text.strip_prefix(PREFIX)
                .context("invalid invitation link")?,
        )
        .context("invalid invitation encoding")?;
        let invite: Self = serde_json::from_slice(&bytes).context("invalid invitation")?;
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
    let gateway = invite.address.id;
    let connection = tokio::time::timeout(
        Duration::from_secs(30),
        endpoint.connect(invite.address, PAIR_ALPN),
    )
    .await??;
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let (mut tx, mut rx) = connection.open_bi().await?;
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
        let response: Response = read_frame(&mut rx)
            .await?
            .context("missing enrollment response; retry the invitation")?;
        if !response.accepted {
            bail!("invitation rejected")
        }
        let name = gateway_name.to_owned();
        tokio::task::spawn_blocking(move || store.register_gateway(gateway, &name)).await??;
        Ok::<_, anyhow::Error>(())
    })
    .await;
    connection.close(0u32.into(), b"enrollment finished");
    result??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let invite =
            Invite::decode(&Invite::issue(&gs, &gateway).unwrap().encode().unwrap()).unwrap();
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
