//! Count- and encoded-byte-bounded fanout for A2UI updates.
//!
//! Slow readers get Lagged and must reconnect for a snapshot. The ring owns at
//! most the configured JSON wire bytes; a reader holds at most its current frame
//! outside it. JSON allocation overhead is additional, not an RSS guarantee.
use serde_json::Value;
use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex},
};
use tokio::sync::{broadcast::error::RecvError, watch};

pub const DEFAULT_BYTES: usize = 16 * 1024 * 1024;
struct State {
    queue: VecDeque<(Arc<Value>, usize)>,
    bytes: usize,
    next: u64,
    max_count: usize,
    max_bytes: usize,
}
pub struct Sender {
    state: Arc<Mutex<State>>,
    signal: watch::Sender<u64>,
}
pub struct Receiver {
    state: Arc<Mutex<State>>,
    signal: watch::Receiver<u64>,
    cursor: u64,
}
pub fn channel(max_count: usize, max_bytes: usize) -> (Sender, Receiver) {
    assert!(max_count > 0 && max_bytes > 0);
    let (signal, receiver) = watch::channel(0);
    let state = Arc::new(Mutex::new(State {
        queue: VecDeque::new(),
        bytes: 0,
        next: 0,
        max_count,
        max_bytes,
    }));
    (
        Sender {
            state: state.clone(),
            signal,
        },
        Receiver {
            state,
            signal: receiver,
            cursor: 0,
        },
    )
}
impl Sender {
    pub fn subscribe(&self) -> Receiver {
        let state = self.state.lock().unwrap();
        Receiver {
            state: self.state.clone(),
            signal: self.signal.subscribe(),
            cursor: state.next,
        }
    }
    pub fn send(&self, value: Arc<Value>) -> Result<(), serde_json::Error> {
        // Count without allocating another serialized copy of a large frame.
        struct Counter(usize);
        impl io::Write for Counter {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0 += bytes.len();
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut counter = Counter(0);
        serde_json::to_writer(&mut counter, &*value)?;
        let bytes = counter.0;
        let mut state = self.state.lock().unwrap();
        while !state.queue.is_empty()
            && (state.queue.len() >= state.max_count
                || state.bytes.saturating_add(bytes) > state.max_bytes)
        {
            let (_, removed) = state.queue.pop_front().unwrap();
            state.bytes -= removed;
        }
        state.next += 1;
        if bytes <= state.max_bytes {
            state.bytes += bytes;
            state.queue.push_back((value, bytes));
        }
        // An oversized frame creates a sequence gap. Existing readers must
        // resnapshot rather than silently continuing after a missing update.
        self.signal.send_replace(state.next);
        Ok(())
    }
}
impl Receiver {
    pub async fn recv(&mut self) -> Result<Arc<Value>, RecvError> {
        loop {
            {
                let state = self.state.lock().unwrap();
                let oldest = state.next - state.queue.len() as u64;
                if self.cursor < oldest {
                    let lost = oldest - self.cursor;
                    self.cursor = oldest;
                    return Err(RecvError::Lagged(lost));
                }
                if self.cursor < state.next {
                    let value = state.queue[(self.cursor - oldest) as usize].0.clone();
                    self.cursor += 1;
                    return Ok(value);
                }
            }
            self.signal.changed().await.map_err(|_| RecvError::Closed)?;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[tokio::test]
    async fn byte_eviction_reports_gap_and_new_readers_only_receive_new_updates() {
        let (sender, mut slow) = channel(100, 10);
        sender.send(Arc::new(json!("1234"))).unwrap(); // six wire bytes
        sender.send(Arc::new(json!("5678"))).unwrap();
        assert!(matches!(slow.recv().await, Err(RecvError::Lagged(1))));
        assert_eq!(*slow.recv().await.unwrap(), json!("5678"));
        let mut fresh = sender.subscribe();
        sender.send(Arc::new(json!(true))).unwrap();
        assert_eq!(*fresh.recv().await.unwrap(), json!(true));
        assert!(sender.state.lock().unwrap().bytes <= 10);
        drop(sender);
        assert!(matches!(fresh.recv().await, Err(RecvError::Closed)));
    }
    #[tokio::test]
    async fn count_limit_oversized_frames_and_pending_wakeup_preserve_sequence() {
        let (sender, mut receiver) = channel(1, 8);
        sender.send(Arc::new(json!(1))).unwrap();
        sender.send(Arc::new(json!(2))).unwrap();
        assert!(matches!(receiver.recv().await, Err(RecvError::Lagged(1))));
        assert_eq!(*receiver.recv().await.unwrap(), json!(2));
        sender.send(Arc::new(json!("too large for ring"))).unwrap();
        assert!(matches!(receiver.recv().await, Err(RecvError::Lagged(1))));
        let task = tokio::spawn(async move { receiver.recv().await });
        tokio::task::yield_now().await;
        sender.send(Arc::new(json!(3))).unwrap();
        assert_eq!(
            *tokio::time::timeout(std::time::Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            json!(3)
        );
    }
}
