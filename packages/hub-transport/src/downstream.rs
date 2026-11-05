//! Browser source admission policy. Hub owner and share boards may span any
//! number of Gateways/services within the shared lease and snapshot byte budgets.
use omochi_downstream_core::{Fault, Mode, Permissions, SessionState, SourceLimits};

pub fn new_state(mode: Mode, permissions: Permissions) -> Result<SessionState, Fault> {
    SessionState::with_source_limits(
        mode,
        permissions,
        SourceLimits {
            gateways: None,
            services: None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use omochi_downstream_core::{Phase, RenderFrame, Rpc};
    use serde_json::json;

    fn create(index: usize) -> RenderFrame {
        RenderFrame::Message {
            sequence: (index + 1).to_string(),
            message: json!({"version":"v0.9", "createSurface": {
                "surfaceId": format!("node{}~app{index}/main", index % 3)
            }}),
            lease: Some(format!("lease{index}")),
        }
    }

    #[test]
    fn owner_and_share_snapshots_accept_three_gateways_and_nine_services() {
        for mode in [Mode::Owner, Mode::Share] {
            let mut state = new_state(mode, Permissions::default()).unwrap();
            for index in 0..9 {
                let effect = state.accept(&create(index)).unwrap();
                assert_eq!(effect.lease, Some(format!("lease{index}")));
                state.acknowledge(&effect.sequence).unwrap();
            }
            state
                .accept(&RenderFrame::SnapshotEnd {
                    sequence: "10".into(),
                })
                .unwrap();
            state.acknowledge("10").unwrap();
            assert_eq!(state.phase(), Phase::Live);
            for index in 0..9 {
                state
                    .authorize(&Rpc::Image {
                        lease: format!("lease{index}"),
                        uri: "image.png".into(),
                    })
                    .unwrap();
            }
            assert_eq!(state.close().len(), 9);
        }
    }

    #[test]
    fn browser_policy_preserves_the_shared_lease_limit() {
        let mut state = new_state(Mode::Owner, Permissions::default()).unwrap();
        for index in 0..1024 {
            let effect = state.accept(&create(index)).unwrap();
            state.acknowledge(&effect.sequence).unwrap();
        }
        assert_eq!(
            state.accept(&create(1024)).unwrap_err(),
            Fault::CapacityExceeded
        );
        assert_eq!(state.pending_ack(), None);
        assert_eq!(state.close().len(), 1024);
    }

    #[test]
    fn browser_policy_preserves_the_shared_snapshot_byte_limit() {
        let mut state = new_state(Mode::Owner, Permissions::default()).unwrap();
        state.accept(&create(0)).unwrap();
        state.acknowledge("1").unwrap();
        // Individually bounded frames can still exceed the aggregate snapshot budget.
        let data = "x".repeat(1024 * 1024);
        for index in 0..32 {
            let sequence = (index + 2).to_string();
            let frame = RenderFrame::Message {
                sequence: sequence.clone(),
                message: json!({"version":"v0.9", "updateDataModel": {
                    "surfaceId": "node0~app0/main", "path": format!("/key{index}"), "value": data
                }}),
                lease: Some("lease0".into()),
            };
            if index == 31 {
                assert_eq!(state.accept(&frame).unwrap_err(), Fault::CapacityExceeded);
            } else {
                state.accept(&frame).unwrap();
                state.acknowledge(&sequence).unwrap();
            }
        }
        assert_eq!(state.pending_ack(), None);
        assert_eq!(state.close(), ["lease0"]);
    }
}
