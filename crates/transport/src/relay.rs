//! Shared native/browser relay admission policy. Application authorization is separate.
use iroh::{RelayConfig, RelayUrl};

pub(crate) const PRODUCTION_RELAY_URL: &str = "https://relay.omochi.fyi";
// Shared admission for distributed clients; hubs and gateways authorize application access.
pub(crate) const PRODUCTION_RELAY_TOKEN: &str = "omochi-gateway-relay";

pub(crate) fn production_relay() -> RelayConfig {
    RelayConfig::from(
        PRODUCTION_RELAY_URL
            .parse::<RelayUrl>()
            .expect("valid relay URL"),
    )
    .with_auth_token(PRODUCTION_RELAY_TOKEN)
}

pub(crate) fn relay_config(url: RelayUrl) -> RelayConfig {
    let production = production_relay();
    if url == production.url {
        production
    } else {
        RelayConfig::from(url)
    }
}
