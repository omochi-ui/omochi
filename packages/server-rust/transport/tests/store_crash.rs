//! Abrupt process exit after commit must not reopen an invitation to another peer.
use omochi_transport::store::{Role, Store};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

#[test]
fn crash_worker() {
    let Ok(root) = std::env::var("OMOCHI_CRASH_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let store = Store::open(root.join("gateway")).unwrap();
    match std::env::var("OMOCHI_CRASH_TEST_STAGE").unwrap().as_str() {
        "issue" => {
            let invite = store.invite(100, 180).unwrap();
            std::fs::write(root.join("invite"), invite).unwrap();
        }
        "redeem" => {
            let hub = Store::open(root.join("hub")).unwrap();
            let peer = hub.identity(Role::Hub).unwrap().public();
            let invite = std::fs::read_to_string(root.join("invite")).unwrap();
            store.redeem(&invite, peer, "PC", 101).unwrap();
        }
        other => panic!("unexpected stage {other}"),
    }
    // No destructors, application shutdown or enrollment acknowledgement.
    std::process::exit(99);
}

#[test]
fn issue_and_redemption_survive_process_exit_before_acknowledgement() {
    let root = tempfile::tempdir().unwrap();
    let original = Store::open(root.path().join("gateway"))
        .unwrap()
        .identity(Role::Gateway)
        .unwrap()
        .public();
    for stage in ["issue", "redeem"] {
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "crash_worker", "--nocapture"])
            .env("OMOCHI_CRASH_TEST_ROOT", root.path())
            .env("OMOCHI_CRASH_TEST_STAGE", stage)
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(99));
    }
    let store = Store::open(root.path().join("gateway")).unwrap();
    assert_eq!(store.identity(Role::Gateway).unwrap().public(), original);
    let peer = Store::open(root.path().join("hub"))
        .unwrap()
        .identity(Role::Hub)
        .unwrap()
        .public();
    let stranger = Store::open(root.path().join("other"))
        .unwrap()
        .identity(Role::Hub)
        .unwrap()
        .public();
    let invite = std::fs::read_to_string(root.path().join("invite")).unwrap();
    let generation = store.peers().unwrap().0;
    assert!(store.redeem(&invite, stranger, "other", 102).is_err());
    store.redeem(&invite, peer, "PC", 102).unwrap();
    assert_eq!(
        store.peers().unwrap().0,
        generation,
        "retry must be idempotent"
    );
    assert_eq!(store.peers().unwrap().1.len(), 1);
}
