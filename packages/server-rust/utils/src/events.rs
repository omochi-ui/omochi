//! Non-consuming cursors, bounded retention and cancellable long polling.
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct EventBuffer {
    state: Arc<Mutex<State>>,
    changed: watch::Sender<u64>,
}
struct State {
    events: VecDeque<Value>,
    sequence: u64,
    capacity: usize,
    bytes: usize,
    closed: bool,
}
const MAX_BYTES: usize = 16 * 1024 * 1024;
impl Default for EventBuffer {
    fn default() -> Self {
        Self::new(1000)
    }
}
impl EventBuffer {
    pub fn new(capacity: usize) -> Self {
        let (changed, _) = watch::channel(0);
        Self {
            state: Arc::new(Mutex::new(State {
                events: VecDeque::new(),
                sequence: 0,
                capacity,
                bytes: 0,
                closed: false,
            })),
            changed,
        }
    }
    pub fn push(&self, message: Value) {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return;
        }
        state.sequence += 1;
        let event = json!({"sequence":state.sequence,"receivedAt":chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis,true),"message":message});
        state.bytes += event.to_string().len();
        state.events.push_back(event);
        while state.events.len() > state.capacity || state.bytes > MAX_BYTES {
            if let Some(event) = state.events.pop_front() {
                state.bytes -= event.to_string().len();
            }
        }
        self.changed.send_replace(state.sequence);
    }
    pub fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        self.changed.send_replace(state.sequence);
    }
    pub async fn read(
        &self,
        after: u64,
        timeout_ms: u64,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        if timeout_ms > 30000 {
            bail!("timeoutMs must be between 0 and 30000");
        }
        let mut changed = self.changed.subscribe();
        let wait = {
            let state = self.state.lock().unwrap();
            if after > state.sequence {
                bail!("after must be an existing cursor (or 0)");
            }
            !state.closed && after == state.sequence && timeout_ms > 0
        };
        if cancel.is_cancelled() {
            bail!("cancelled");
        }
        if wait {
            tokio::select! {
                _ = cancel.cancelled() => bail!("cancelled"),
                _ = changed.changed() => {},
                _ = tokio::time::sleep(Duration::from_millis(timeout_ms)) => {},
            }
        }
        if cancel.is_cancelled() {
            bail!("cancelled");
        }
        let state = self.state.lock().unwrap();
        let oldest = state
            .events
            .front()
            .and_then(|v| v["sequence"].as_u64())
            .unwrap_or(state.sequence + 1);
        let events: Vec<_> = state
            .events
            .iter()
            .filter(|e| e["sequence"].as_u64().unwrap() > after)
            .cloned()
            .collect();
        Ok(
            json!({"events":events,"nextCursor":state.sequence,"dropped":oldest.saturating_sub(after+1),"closed":state.closed}),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn replay_gaps_wakeup_cancel_and_close() {
        let events = EventBuffer::new(2);
        let cancel = CancellationToken::new();
        for name in ["one", "two", "three"] {
            events.push(json!({"action":{"name":name}}));
        }
        let page = events.read(0, 0, &cancel).await.unwrap();
        assert_eq!(page["dropped"], 1);
        assert_eq!(page["nextCursor"], 3);
        assert_eq!(events.read(0, 0, &cancel).await.unwrap(), page);
        assert!(events.read(4, 0, &cancel).await.is_err());
        let clone = events.clone();
        let token = cancel.clone();
        let pending = tokio::spawn(async move { clone.read(3, 30000, &token).await });
        tokio::task::yield_now().await;
        events.push(json!({"action":{}}));
        assert_eq!(
            pending.await.unwrap().unwrap()["events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(
            events.read(4, 1, &cancel).await.unwrap()["events"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let clone = events.clone();
        let token = cancel.clone();
        let pending = tokio::spawn(async move { clone.read(4, 30000, &token).await });
        cancel.cancel();
        assert!(pending.await.unwrap().is_err());
        events.close();
        assert_eq!(
            events
                .read(4, 30000, &CancellationToken::new())
                .await
                .unwrap()["closed"],
            true
        );
    }
}
