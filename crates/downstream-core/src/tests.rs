use super::*;
use serde_json::json;

#[test]
fn common_retry_policy_stops_terminal_and_ambiguous_storage_failures() {
    for fault in [
        Fault::ProtocolError,
        Fault::NotAuthorized,
        Fault::CapacityExceeded,
        Fault::RenderError,
        Fault::StorageCorrupt,
        Fault::StorageOutcomeUnknown,
        Fault::InvalidInvitation,
        Fault::NotFound,
    ] {
        assert!(!Fault::retryable_code(fault.code()));
    }
    for fault in [
        Fault::NetworkUnavailable,
        Fault::Timeout,
        Fault::SourceUnavailable,
    ] {
        assert!(Fault::retryable_code(fault.code()));
    }
}

#[test]
fn readonly_permissions_and_snapshot_completion_are_explicit() {
    let mut state = SessionState::new(Mode::Public, Permissions::default()).unwrap();
    state.accept(&create("1", "lease")).unwrap();
    assert_eq!(
        state.authorize(&Rpc::Action {
            lease: "lease".into(),
            message: json!({})
        }),
        Err(Fault::NotAuthorized)
    );
    assert!(
        state
            .authorize(&Rpc::Image {
                lease: "lease".into(),
                uri: "uri".into()
            })
            .is_ok()
    );
    assert_eq!(
        state.authorize(&Rpc::Control {
            command: ControlCommand::BoardsList {}
        }),
        Err(Fault::NotAuthorized)
    );
    state.acknowledge("1").unwrap();
    state
        .accept(&RenderFrame::SnapshotEnd {
            sequence: "2".into(),
        })
        .unwrap();
    state.acknowledge("2").unwrap();
    assert!(
        state
            .accept(&RenderFrame::SnapshotEnd {
                sequence: "3".into()
            })
            .is_err()
    );
    assert_eq!(state.phase(), Phase::Live);
}
fn create(sequence: &str, lease: &str) -> RenderFrame {
    RenderFrame::Message {
        sequence: sequence.into(),
        message: json!({"version":"v0.9","createSurface":{"surfaceId":"node~app/main"}}),
        lease: Some(lease.into()),
    }
}
fn state() -> SessionState {
    SessionState::new(
        Mode::Owner,
        Permissions {
            interact: true,
            edit_layout: true,
        },
    )
    .unwrap()
}
#[test]
fn acknowledgement_requires_successful_application_and_canonical_monotonic_sequence() {
    let mut s = state();
    for sequence in ["0", "01", "+1", "18446744073709551616"] {
        assert_eq!(
            s.accept(&create(sequence, "one")).unwrap_err(),
            Fault::ProtocolError
        );
    }
    s.accept(&create("1", "one")).unwrap();
    assert!(s.accept(&create("2", "two")).is_err());
    assert!(s.acknowledge("2").is_err());
    assert_eq!(s.pending_ack(), Some("1"));
    s.acknowledge("1").unwrap();
    assert!(s.accept(&create("1", "one")).is_err());
    s.accept(&RenderFrame::SnapshotEnd {
        sequence: "18446744073709551615".into(),
    })
    .unwrap();
    assert_eq!(s.phase(), Phase::Live);
}
#[test]
fn recreation_replacement_deletion_and_close_invalidate_leases() {
    let mut s = state();
    s.accept(&create("1", "one")).unwrap();
    s.acknowledge("1").unwrap();
    assert!(s.accept(&create("2", "one")).is_err());
    let update = |sequence: &str, lease: &str| RenderFrame::Message {
        sequence: sequence.into(),
        message: json!({"version":"v0.9","updateComponents":{"surfaceId":"node~app/main","components":[]}}),
        lease: Some(lease.into()),
    };
    assert!(
        s.accept(&update("2", "one"))
            .unwrap()
            .invalidated_leases
            .is_empty()
    );
    s.acknowledge("2").unwrap();
    assert_eq!(
        s.accept(&update("3", "two")).unwrap().invalidated_leases,
        ["one"]
    );
    s.acknowledge("3").unwrap();
    let image = |lease: &str| Rpc::Image {
        lease: lease.into(),
        uri: "uri".into(),
    };
    assert_eq!(s.authorize(&image("one")), Err(Fault::ScopeExpired));
    assert!(s.authorize(&image("two")).is_ok());
    let deletion = RenderFrame::Message {
        sequence: "4".into(),
        message: json!({"version":"v0.9","deleteSurface":{"surfaceId":"node~app/main"}}),
        lease: None,
    };
    assert_eq!(s.accept(&deletion).unwrap().invalidated_leases, ["two"]);
    assert_eq!(s.authorize(&image("two")), Err(Fault::ScopeExpired));
    s.close();
    assert_eq!(
        s.accept(&create("5", "new")).unwrap_err(),
        Fault::ScopeExpired
    );
}
#[test]
fn rejected_frame_does_not_replace_valid_lease_or_consume_sequence() {
    let mut s = state();
    s.accept(&create("1", "old")).unwrap();
    s.acknowledge("1").unwrap();
    assert!(s.accept(&create("2", "")).is_err());
    assert!(
        s.authorize(&Rpc::Image {
            lease: "old".into(),
            uri: "uri".into()
        })
        .is_ok()
    );
    s.accept(&RenderFrame::Message { sequence: "2".into(), message: json!({"version":"v0.9","updateComponents":{"surfaceId":"node~app/main","components":[]}}), lease: Some("new".into()) }).unwrap();
}
#[test]
fn typed_contracts_preserve_wire_names_and_reject_unknown_operations() {
    for value in [
        json!({"method":"next","ack":"18446744073709551615"}),
        json!({"method":"register","invitation":"link","save":false}),
        json!({"method":"rpc","value":{"op":"control","command":{"method":"shareCreate","board_id":"b","public":false,"permissions":{"interact":true,"editLayout":false}}}}),
    ] {
        let command: Command = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(command).unwrap(), value);
    }
    assert!(serde_json::from_value::<Command>(json!({"method":"unknown"})).is_err());
    assert!(
        serde_json::from_value::<Command>(
            json!({"method":"rpc","value":{"op":"ping","extra":true}})
        )
        .is_err()
    );
    assert!(
        SessionState::new(
            Mode::Public,
            Permissions {
                interact: true,
                edit_layout: false
            }
        )
        .is_err()
    );
}
