//! Reject-on-overload queues with a byte permit held through processing.
use std::{fmt, sync::Arc};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};

pub const DEFAULT_BYTES: usize = 8 * 1024 * 1024;
pub struct Entry<T> {
    value: T,
    permit: OwnedSemaphorePermit,
}
impl<T> Entry<T> {
    /// Keep the returned permit alive until processing/writing the value finishes.
    pub fn into_parts(self) -> (T, OwnedSemaphorePermit) {
        (self.value, self.permit)
    }
}
pub struct Sender<T> {
    sender: mpsc::Sender<Entry<T>>,
    budget: Arc<Semaphore>,
    max_item: usize,
}
impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            budget: self.budget.clone(),
            max_item: self.max_item,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum SendError {
    TooLarge,
    Busy,
    Closed,
}
impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "message exceeds queue item limit",
            Self::Busy => "queue byte or item budget exhausted",
            Self::Closed => "queue is closed",
        })
    }
}
impl std::error::Error for SendError {}
pub fn channel<T>(
    count: usize,
    bytes: usize,
    max_item: usize,
) -> (Sender<T>, mpsc::Receiver<Entry<T>>) {
    assert!(bytes > 0 && bytes <= u32::MAX as usize && max_item <= bytes);
    let (sender, receiver) = mpsc::channel(count);
    (
        Sender {
            sender,
            budget: Arc::new(Semaphore::new(bytes)),
            max_item,
        },
        receiver,
    )
}
impl<T> Sender<T> {
    pub fn try_send(&self, value: T, encoded_bytes: usize) -> Result<(), SendError> {
        if encoded_bytes > self.max_item {
            return Err(SendError::TooLarge);
        }
        if self.sender.is_closed() {
            return Err(SendError::Closed);
        }
        let permit = self
            .budget
            .clone()
            .try_acquire_many_owned(encoded_bytes.max(1) as u32)
            .map_err(|_| SendError::Busy)?;
        self.sender
            .try_send(Entry { value, permit })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => SendError::Busy,
                mpsc::error::TrySendError::Closed(_) => SendError::Closed,
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn in_flight_bytes_remain_charged_until_processing_finishes() {
        let (sender, mut receiver) = channel(8, 10, 8);
        sender.try_send("first", 6).unwrap();
        let (_, permit) = receiver.recv().await.unwrap().into_parts();
        assert_eq!(sender.try_send("second", 6), Err(SendError::Busy));
        drop(permit);
        sender.try_send("second", 6).unwrap();
        assert_eq!(sender.try_send("too large", 9), Err(SendError::TooLarge));
        drop(receiver);
        assert_eq!(sender.try_send("closed", 1), Err(SendError::Closed));
    }
    #[tokio::test]
    async fn rejected_entries_release_their_permits_and_sender_clones_share_budget() {
        let (sender, mut receiver) = channel(1, 10, 8);
        let clone = sender.clone();
        sender.try_send(1, 4).unwrap();
        assert_eq!(clone.try_send(2, 4), Err(SendError::Busy));
        drop(receiver.recv().await.unwrap());
        clone.try_send(3, 8).unwrap();
        assert_eq!(sender.try_send(4, 3), Err(SendError::Busy));
    }
}
