//! Blocking operations remain owned until completion, even if their caller is cancelled.
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{sync::Notify, task::JoinHandle};
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static CHANGED: Notify = Notify::const_new();
struct Active;
impl Drop for Active {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
        CHANGED.notify_waiters();
    }
}
pub fn run<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> JoinHandle<T> {
    ACTIVE.fetch_add(1, Ordering::SeqCst);
    let active = Active;
    tokio::task::spawn_blocking(move || {
        let _active = active;
        work()
    })
}
/// Used after old sessions have been joined, before acknowledging their revocation.
pub async fn drain() {
    loop {
        let changed = CHANGED.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        if ACTIVE.load(Ordering::SeqCst) == 0 {
            return;
        }
        changed.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancelled_caller_does_not_lose_dispatched_work() {
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let task = run(move || {
            started.send(()).unwrap();
            wait.recv().unwrap();
        });
        ready.await.unwrap();
        drop(task);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), drain())
                .await
                .is_err()
        );
        release.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), drain())
            .await
            .unwrap();
    }
}
