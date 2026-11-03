//! Foreground interface polling avoids macOS-only route socket APIs on iOS.
use super::actor::NetworkMessage;
use n0_error::stack_error;
use n0_future::task::{self, AbortOnDropHandle};
use tokio::sync::mpsc;
#[stack_error(derive, add_meta)]
pub struct Error;
#[derive(Debug)]
pub(super) struct RouteMonitor {
    _task: AbortOnDropHandle<()>,
}
impl RouteMonitor {
    pub(super) fn new(sender: mpsc::Sender<NetworkMessage>) -> Result<Self, Error> {
        let handle = task::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if sender.send(NetworkMessage::Change).await.is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            _task: AbortOnDropHandle::new(handle),
        })
    }
}
