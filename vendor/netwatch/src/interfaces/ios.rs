//! iOS exposes addresses through getifaddrs, not macOS routing-socket types.
pub(super) use super::netdev_impl::get_state;
use super::{DefaultRouteDetails, HomeRouter};
pub(super) async fn default_route() -> Option<DefaultRouteDetails> {
    None
}
pub(super) fn home_router() -> Option<HomeRouter> {
    None
}
