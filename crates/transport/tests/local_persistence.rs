//! Persistence guarantees exercised on both Unix and Windows.
#![cfg(feature = "desktop")]
use omochi_transport::{SecretKey, live, store::Store};
use std::time::Duration;

#[tokio::test]
async fn revocation_waits_for_every_live_lease_and_stale_store_cannot_restore_records() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().join("state");
    let store = Store::open(&directory).unwrap();
    let stale = Store::open(&directory).unwrap();
    let peer = SecretKey::from_bytes(&rand::random()).public();
    store.register_gateway(peer, "gateway").unwrap();
    let first = live::Lease::register(&store).unwrap();
    let second = live::Lease::register(&store).unwrap();
    assert!(
        live::forget(store.clone(), peer, Duration::from_millis(30))
            .await
            .is_err()
    );
    assert!(!store.peers().unwrap().1.contains_key(&peer.to_string()));
    assert!(!stale.peers().unwrap().1.contains_key(&peer.to_string()));

    let before = store.peers().unwrap().0;
    let copy = store.clone();
    let mut forgetting =
        tokio::spawn(async move { live::forget(copy, peer, Duration::from_secs(5)).await });
    let generation = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let generation = store.peers().unwrap().0;
            if generation > before {
                return generation;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    first.acknowledge(generation).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut forgetting)
            .await
            .is_err()
    );
    second.acknowledge(generation).unwrap();
    forgetting.await.unwrap().unwrap();
    let other = SecretKey::from_bytes(&rand::random()).public();
    stale.register_gateway(other, "other").unwrap();
    let peers = store.peers().unwrap().1;
    assert!(!peers.contains_key(&peer.to_string()));
    assert!(peers.contains_key(&other.to_string()));
}

#[test]
fn lock_holder_fixture() {
    let Some(directory) = std::env::var_os("OMOCHI_PERSISTENCE_TEST_ROOT") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let _lock =
        omochi_local::files::FileLock::acquire(&directory.join("transaction.lock"), false).unwrap();
    std::fs::write(directory.join("ready"), b"ready").unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !directory.join("release").exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[tokio::test]
async fn transaction_lock_serializes_processes_without_blocking_async_timers() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().join("private");
    omochi_local::files::private_directory(&directory).unwrap();
    let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_holder_fixture"])
        .env("OMOCHI_PERSISTENCE_TEST_ROOT", &directory)
        .stdout(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !directory.join("ready").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let path = directory.join("transaction.lock");
    let mut locking = omochi_transport::blocking::run(move || {
        omochi_local::files::FileLock::acquire(&path, false).unwrap()
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(30), &mut locking)
            .await
            .is_err()
    );
    std::fs::write(directory.join("release"), b"release").unwrap();
    let _lock = locking.await.unwrap();
    assert!(child.wait().await.unwrap().success());
    let path = directory.join("state.json");
    omochi_local::files::atomic_write(&path, b"first").unwrap();
    let mut old = std::fs::File::open(&path).unwrap();
    omochi_local::files::atomic_write(&path, b"second").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"second");
    let mut previous = Vec::new();
    std::io::Read::read_to_end(&mut old, &mut previous).unwrap();
    assert_eq!(previous, b"first");
    drop(old);
}
