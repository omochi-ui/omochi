//! Native invitation adapter: canonical link validation and endpoint addresses.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use omochi_transport::EndpointAddr;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Link {
    pub(super) version: u8,
    pub(super) hub_id: String,
    pub(super) relay_urls: Vec<String>,
    pub(super) grant_id: String,
    pub(super) mode: Mode,
    pub(super) secret: String,
}
impl Link {
    pub(super) fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.version == 1
                && self.hub_id.len() == 64
                && self
                    .hub_id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            Fault::InvalidInvitation
        );
        let _: omochi_transport::EndpointId = self
            .hub_id
            .parse()
            .map_err(|_| anyhow::anyhow!(Fault::InvalidInvitation))?;
        let id = uuid::Uuid::parse_str(&self.grant_id)
            .map_err(|_| anyhow::anyhow!(Fault::InvalidInvitation))?;
        anyhow::ensure!(id.to_string() == self.grant_id, Fault::InvalidInvitation);
        anyhow::ensure!(
            self.secret.len() == 64
                && self
                    .secret
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            Fault::InvalidInvitation
        );
        anyhow::ensure!(
            (1..=4).contains(&self.relay_urls.len()),
            Fault::InvalidInvitation
        );
        let mut unique = std::collections::BTreeSet::new();
        for relay in &self.relay_urls {
            // Relay URL parsing also happens in the transport. Reject URL credentials here.
            let url =
                url::Url::parse(relay).map_err(|_| anyhow::anyhow!(Fault::InvalidInvitation))?;
            anyhow::ensure!(
                url.scheme() == "https"
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none(),
                Fault::InvalidInvitation
            );
            let address: EndpointAddr =
                serde_json::from_value(json!({"id":self.hub_id,"addrs":[{"Relay":relay}]}))
                    .map_err(|_| anyhow::anyhow!(Fault::InvalidInvitation))?;
            anyhow::ensure!(
                relay.len() <= 2048
                    && relay.starts_with("https://")
                    && !relay.contains(['@', '?', '#'])
                    && address.relay_urls().next().is_some()
                    && unique.insert(relay),
                Fault::InvalidInvitation
            );
        }
        Ok(())
    }
    pub(super) fn parse(input: &str) -> Result<Self> {
        anyhow::ensure!(input.len() <= 32768, Fault::InvalidInvitation);
        let (kind, payload) = if let Some(rest) = input.strip_prefix("omochi://") {
            rest.split_once('/')
                .with_context(|| Fault::InvalidInvitation)?
        } else {
            let url =
                url::Url::parse(input).map_err(|_| anyhow::anyhow!(Fault::InvalidInvitation))?;
            anyhow::ensure!(
                url.scheme() == "https"
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none(),
                Fault::InvalidInvitation
            );
            input
                .split_once('#')
                .with_context(|| Fault::InvalidInvitation)?
                .1
                .split_once(':')
                .with_context(|| Fault::InvalidInvitation)?
        };
        anyhow::ensure!(matches!(kind, "share" | "owner"), Fault::InvalidInvitation);
        let bytes = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| anyhow::anyhow!(Fault::InvalidInvitation))?;
        let link: Self = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!(Fault::InvalidInvitation))?;
        link.validate()?;
        anyhow::ensure!(
            (kind == "owner") == (link.mode == Mode::Owner),
            Fault::InvalidInvitation
        );
        Ok(link)
    }
    pub(super) fn address(&self) -> Result<EndpointAddr> {
        Ok(serde_json::from_value(
            json!({"id":self.hub_id,"addrs":self.relay_urls.iter().map(|r|json!({"Relay":r})).collect::<Vec<_>>()}),
        )?)
    }
    pub(super) fn id(&self) -> String {
        format!("{}/{}", self.hub_id, self.grant_id)
    }
    pub(super) fn metadata(&self) -> Value {
        json!({"id":self.id(),"hubId":self.hub_id,"grantId":self.grant_id,"mode":self.mode})
    }
}
