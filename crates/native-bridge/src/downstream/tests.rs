use super::*;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use omochi_protocol::{read_frame, write_frame};
use omochi_transport::SecretKey;

#[test]
fn typed_owner_commands_match_the_existing_server_contract() {
    use omochi_downstream_core::ControlCommand;
    for value in [
        json!({"method":"gatewayPairStart","invitation":"link","name":"Gateway"}),
        json!({"method":"gatewayPairStatus","id":"job"}),
        json!({"method":"gatewayPairRetrySave","id":"job"}),
        json!({"method":"boardsList"}),
        json!({"method":"boardGet","id":"board"}),
        json!({"method":"boardCreate","value":{"name":"Board","panels":[]}}),
        json!({"method":"boardUpdate","id":"board","value":{"revision":1}}),
        json!({"method":"boardDelete","id":"board","value":{"revision":1}}),
        json!({"method":"presets"}),
        json!({"method":"connections"}),
        json!({"method":"sharing"}),
        json!({"method":"sharesList"}),
        json!({"method":"shareCreate","board_id":"board","public":false,"permissions":{"interact":true,"editLayout":false}}),
        json!({"method":"shareRevoke","id":"share"}),
    ] {
        let typed: ControlCommand = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), value);
    }
}

#[cfg(test)]
mod invitation_tests {
    use super::*;
    #[test]
    fn shared_invitation_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../spec/fixtures/hub-invitations.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            assert_eq!(
                Link::parse(case["input"].as_str().unwrap()).is_ok(),
                case["valid"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
    #[test]
    fn invitation_modes_and_secrets_are_checked() {
        let value = json!({"version":1,"hubId":SecretKey::generate().public().to_string(),"grantId":uuid::Uuid::nil(),"relayUrls":["https://relay.example/"],"mode":"owner","secret":"cd".repeat(32)});
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).unwrap());
        assert!(Link::parse(&format!("omochi://owner/{payload}")).is_ok());
        assert!(Link::parse(&format!("https://app.omochi.fyi/#owner:{payload}")).is_ok());
        assert!(Link::parse(&format!("omochi://share/{payload}")).is_err());
        assert!(Link::parse("omochi://owner/invalid").is_err());
    }
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    #[tokio::test]
    async fn malformed_peer_json_is_a_terminal_protocol_fault() {
        let server = omochi_transport::bind(SecretKey::generate(), true, None)
            .await
            .unwrap();
        server.set_alpns(vec![b"omochi/owner/1".to_vec()]);
        let address = server.addr();
        let endpoint = omochi_transport::bind(SecretKey::generate(), true, None)
            .await
            .unwrap();
        let link = Link {
            version: 1,
            hub_id: server.id().to_string(),
            relay_urls: vec!["https://relay.example/".into()],
            grant_id: uuid::Uuid::nil().to_string(),
            mode: Mode::Owner,
            secret: "0".repeat(64),
        };
        let serving = tokio::spawn(async move {
            let connection = server.accept().await.unwrap().await.unwrap();
            let (mut tx, mut rx) = connection.accept_bi().await.unwrap();
            let _: Value = read_frame(&mut rx).await.unwrap().unwrap();
            tx.write_all(&[0, 0, 0, 1, b'{']).await.unwrap();
            tx.finish().unwrap();
            connection.closed().await;
            server.close().await;
        });
        let error = Client::open_at(&endpoint, &link, address)
            .await
            .err()
            .unwrap();
        assert_eq!(error.downcast_ref::<Fault>(), Some(&Fault::ProtocolError));
        assert!(!error.downcast_ref::<Fault>().unwrap().retryable());
        endpoint.close().await;
        serving.await.unwrap();
    }

    /// A real QUIC connection, using the same frames as the server implementation.
    #[tokio::test]
    async fn downstream_owner_welcome_and_control_over_quic() {
        let server = omochi_transport::bind(SecretKey::generate(), true, None)
            .await
            .unwrap();
        server.set_alpns(vec![b"omochi/owner/1".to_vec()]);
        let client = omochi_transport::bind(SecretKey::generate(), true, None)
            .await
            .unwrap();
        let address = server.addr();
        let server_task = tokio::spawn(async move {
            let connection = server.accept().await.unwrap().await.unwrap();
            let (mut tx, mut rx) = connection.accept_bi().await.unwrap();
            let hello: Value = read_frame(&mut rx).await.unwrap().unwrap();
            assert_eq!(
                hello,
                json!({"version":1,"grantId":uuid::Uuid::nil().to_string()})
            );
            write_frame(&mut tx, &json!({"kind":"ownerWelcome","services":[]}))
                .await
                .unwrap();
            let (mut reply, mut request) = connection.accept_bi().await.unwrap();
            let command: Value = read_frame(&mut request).await.unwrap().unwrap();
            assert_eq!(command["command"]["method"], "boardsList");
            write_frame(&mut reply, &json!({"kind":"result","value":[]}))
                .await
                .unwrap();
            reply.finish().unwrap();
            let _ = reply.stopped().await;
            connection.close(0u32.into(), b"done");
            server.close().await;
        });
        // Direct loopback avoids depending on external relay availability.
        let connection = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            client.connect(address, b"omochi/owner/1"),
        )
        .await
        .unwrap()
        .unwrap();
        let (mut tx, mut rx) = connection.open_bi().await.unwrap();
        write_frame(
            &mut tx,
            &json!({"version":1,"grantId":uuid::Uuid::nil().to_string()}),
        )
        .await
        .unwrap();
        let welcome: Value = read_frame(&mut rx).await.unwrap().unwrap();
        assert_eq!(welcome["kind"], "ownerWelcome");
        let active = Active {
            endpoint: client.clone(),
            connection,
            stream: AsyncMutex::new((tx, rx)),
            state: AsyncMutex::new(
                SessionState::new(
                    Mode::Owner,
                    omochi_downstream_core::Permissions {
                        interact: true,
                        edit_layout: true,
                    },
                )
                .unwrap(),
            ),
            tokens: Mutex::default(),
            stop: CancellationToken::new(),
        };
        let response = active
            .rpc(Rpc::Control {
                command: omochi_downstream_core::ControlCommand::BoardsList {},
            })
            .await
            .unwrap();
        assert_eq!(response, json!({"kind":"result","value":[]}));
        server_task.await.unwrap();
        client.close().await;
    }
    #[tokio::test]
    async fn leases_expire_on_deletion_and_ack_follows_application() {
        let server = omochi_transport::bind(SecretKey::generate(), true, None)
            .await
            .unwrap();
        server.set_alpns(vec![b"omochi/dashboard/1".to_vec()]);
        let endpoint = omochi_transport::bind(SecretKey::generate(), true, None)
            .await
            .unwrap();
        let address = server.addr();
        let link = Link {
            version: 1,
            hub_id: server.id().to_string(),
            relay_urls: vec!["https://relay.example/".into()],
            grant_id: uuid::Uuid::nil().to_string(),
            mode: Mode::Public,
            secret: "cd".repeat(32),
        };
        let serving = tokio::spawn(async move {
            let connection = server.accept().await.unwrap().await.unwrap();
            let (mut tx, mut rx) = connection.accept_bi().await.unwrap();
            let hello: Value = read_frame(&mut rx).await.unwrap().unwrap();
            assert_eq!(hello["secret"], "cd".repeat(32));
            write_frame(&mut tx,&json!({"kind":"welcome","board":{"id":"board","panels":[]},"permissions":{"interact":false,"editLayout":false}})).await.unwrap();
            for (sequence, message, lease) in [
                (
                    "1",
                    json!({"version":"v0.9","createSurface":{"surfaceId":"node~counter/main"}}),
                    json!("lease-1"),
                ),
                (
                    "2",
                    json!({"version":"v0.9","deleteSurface":{"surfaceId":"node~counter/main"}}),
                    Value::Null,
                ),
            ] {
                write_frame(
                    &mut tx,
                    &json!({"kind":"message","message":message,"lease":lease,"sequence":sequence}),
                )
                .await
                .unwrap();
                let ack: Value = read_frame(&mut rx).await.unwrap().unwrap();
                assert_eq!(ack, json!({"ack":sequence}));
            }
            write_frame(&mut tx, &json!({"kind":"snapshotEnd","sequence":"3"}))
                .await
                .unwrap();
            connection.closed().await;
            server.close().await;
        });
        let (connection, tx, rx, welcome) =
            Client::open_at(&endpoint, &link, address).await.unwrap();
        let client = Client::default();
        *client.active.write().await = Some(Arc::new(Active {
            endpoint,
            connection,
            stream: AsyncMutex::new((tx, rx)),
            state: AsyncMutex::new(
                SessionState::new(
                    Mode::Public,
                    serde_json::from_value(welcome["permissions"].clone()).unwrap(),
                )
                .unwrap(),
            ),
            tokens: Mutex::default(),
            stop: CancellationToken::new(),
        }));
        let store = Arc::new(CallbackStore::new());
        let frame = client
            .execute_inner(json!({"method":"next"}), store.clone())
            .await
            .unwrap();
        assert_eq!(frame["sequence"], "1");
        let token = client.lease_token("lease-1").await.unwrap();
        assert!(!token.is_cancelled());
        let denied = client
            .execute_inner(
                json!({"method":"rpc","value":{"op":"action","lease":"lease-1","message":{}}}),
                store.clone(),
            )
            .await
            .unwrap_err();
        assert_eq!(denied.to_string(), "NotAuthorized");
        client
            .execute_inner(json!({"method":"next","ack":"1"}), store.clone())
            .await
            .unwrap();
        assert!(token.is_cancelled());
        assert!(client.lease_token("lease-1").await.is_err());
        let frame = client
            .execute_inner(json!({"method":"next","ack":"2"}), store)
            .await
            .unwrap();
        assert_eq!(frame["kind"], "snapshotEnd");
        client.close().await;
        serving.await.unwrap();
    }
    #[test]
    fn stored_profile_never_recovers_from_corrupt_key() {
        let profile = Profile {
            version: 1,
            key: "corrupt".into(),
            records: vec![],
        };
        assert!(profile.key().is_err());
    }
}
