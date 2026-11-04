use std::path::PathBuf;

use omochi_ui::A2uiSnapshot;
use serde_json::{Value, json};

fn fixture() -> Value {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../spec/fixtures/snapshot.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn materializes_the_shared_snapshot_fixture() {
    let fixture = fixture();
    let mut snapshot = A2uiSnapshot::new();
    for message in fixture["messages"].as_array().unwrap() {
        snapshot.apply(message).unwrap();
    }
    assert_eq!(
        Value::Array(snapshot.messages()),
        fixture["snapshot"].clone()
    );
    let mut cursor = omochi_ui::SnapshotCursor::default();
    let mut streamed = Vec::new();
    while let Some(message) = snapshot.next_message(&mut cursor) {
        streamed.push(message);
    }
    assert_eq!(Value::Array(streamed), fixture["snapshot"].clone());
    assert!(snapshot.next_message(&mut cursor).is_none());
}

#[test]
fn rejects_duplicate_and_unknown_surfaces() {
    let mut snapshot = A2uiSnapshot::new();
    let create =
        json!({ "version": "v0.9", "createSurface": { "surfaceId": "main", "catalogId": "test" } });
    snapshot.apply(&create).unwrap();
    assert!(snapshot.apply(&create).is_err());
    assert!(
        snapshot
            .apply(&json!({ "version": "v0.9", "updateComponents": { "surfaceId": "other", "components": [] } }))
            .is_err()
    );
    assert_eq!(snapshot.surface_ids(), vec!["main".to_string()]);
}

#[test]
fn keeps_a_root_deletion_visible_in_the_replay() {
    let mut snapshot = A2uiSnapshot::new();
    snapshot
        .apply(&json!({ "version": "v0.9", "createSurface": { "surfaceId": "main", "catalogId": "test" } }))
        .unwrap();
    snapshot
        .apply(&json!({ "version": "v0.9", "updateDataModel": { "surfaceId": "main", "path": "/", "value": { "a": 1 } } }))
        .unwrap();
    snapshot
        .apply(
            &json!({ "version": "v0.9", "updateDataModel": { "surfaceId": "main", "path": "/" } }),
        )
        .unwrap();
    assert_eq!(
        snapshot.messages().last().unwrap().clone(),
        json!({ "version": "v0.9", "updateDataModel": { "surfaceId": "main", "path": "/" } })
    );
}

fn assert_rejected_updates_preserve_replay(kind: &str) {
    let fixture = fixture();
    let fixture = &fixture["rejectedUpdates"];
    for rejected in fixture["cases"].as_array().unwrap() {
        if rejected["message"].get(kind).is_none() {
            continue;
        }
        let mut snapshot = A2uiSnapshot::new();
        for message in fixture["messages"].as_array().unwrap() {
            snapshot.apply(message).unwrap();
        }
        let before = snapshot.messages();
        assert!(
            snapshot.apply(&rejected["message"]).is_err(),
            "{}",
            rejected["name"]
        );
        assert_eq!(snapshot.messages(), before, "{}", rejected["name"]);
        let mut replay = A2uiSnapshot::new();
        let mut cursor = omochi_ui::SnapshotCursor::default();
        while let Some(message) = snapshot.next_message(&mut cursor) {
            replay.apply(&message).unwrap();
        }
        assert_eq!(replay.messages(), before);
        for message in fixture["recoveryMessages"].as_array().unwrap() {
            snapshot.apply(message).unwrap();
        }
        assert_eq!(Value::Array(snapshot.messages()), fixture["snapshot"]);
    }
}

#[test]
fn rejected_component_updates_preserve_replay() {
    assert_rejected_updates_preserve_replay("updateComponents");
}

#[test]
fn rejected_data_updates_preserve_replay() {
    assert_rejected_updates_preserve_replay("updateDataModel");
}
