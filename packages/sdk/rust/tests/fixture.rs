use std::path::PathBuf;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use omochi_ui::{ActionInput, Button, Input, Slider, Text};
use omochi_ui::{App, CallbackTransport, Error, Result, TextOptions};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Default)]
struct State {
    temperature: f64,
    result: Option<String>,
    revision: u64,
}
#[derive(Deserialize, Serialize, ActionInput)]
struct RunInput {
    temperature: f64,
}

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../spec/fixtures/basic.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}
fn client_errors() -> Value {
    serde_json::from_str(include_str!("../../../../spec/fixtures/client-errors.json")).unwrap()
}
fn messages_transport(
    messages: Vec<Value>,
    sent: Arc<Mutex<Vec<Value>>>,
) -> impl omochi_ui::Transport {
    let messages = Arc::new(Mutex::new(VecDeque::from(messages)));
    CallbackTransport::new(
        move |message| {
            sent.lock().unwrap().push(message);
            async { Ok(()) }
        },
        move || {
            let message = messages.lock().unwrap().pop_front();
            async move { Ok(message) }
        },
    )
}

#[tokio::test]
async fn client_reports_allow_subsequent_actions() {
    let fixture = fixture();
    for report in client_errors()["reports"].as_array().unwrap() {
        let mut runtime = runtime().unwrap();
        assert!(runtime.handle(report.clone()).await.unwrap().is_empty());
        let sent = Arc::new(Mutex::new(Vec::new()));
        runtime
            .start_with(messages_transport(
                vec![report.clone(), fixture["action"].clone()],
                sent.clone(),
            ))
            .await
            .unwrap();
        assert_eq!(runtime.read(|state| state.temperature).await, 75.0);
        let expected: Vec<_> = fixture["initialMessages"]
            .as_array()
            .unwrap()
            .iter()
            .chain(fixture["afterActionMessages"].as_array().unwrap())
            .cloned()
            .collect();
        assert_eq!(&sent.lock().unwrap()[1..], expected);
    }
}

#[tokio::test]
async fn malformed_reports_and_unknown_messages_stop_receive_loop() {
    for message in client_errors()["invalid"].as_array().unwrap() {
        let mut runtime = runtime().unwrap();
        assert!(matches!(
            runtime.handle(message.clone()).await,
            Err(Error::Protocol(_))
        ));
        let transport = messages_transport(
            vec![message.clone(), fixture()["action"].clone()],
            Arc::default(),
        );
        assert!(matches!(
            runtime.start_with(transport).await,
            Err(Error::Protocol(_))
        ));
        assert_eq!(runtime.read(|state| state.temperature).await, 50.0);
    }
}

#[tokio::test]
async fn client_reports_do_not_mask_handler_failures() {
    let mut app = App::new(0);
    app.action::<(), _, _>("fail", |_, context| async move {
        context.update(|state| *state += 1).await?;
        Err(Error::Protocol("handler failed".into()))
    })
    .unwrap();
    let mut messages = client_errors()["reports"].as_array().unwrap().clone();
    let action = json!({"type": "invoke", "action": "fail"});
    messages.extend([action.clone(), action]);
    let mut runtime = app.build();
    let result = runtime
        .start_with(messages_transport(messages, Arc::default()))
        .await;
    assert!(matches!(result, Err(Error::Protocol(message)) if message == "handler failed"));
    assert_eq!(runtime.read(|state| *state).await, 1);
}
fn runtime() -> Result<omochi_ui::Runtime<State>> {
    let mut app = App::new(State {
        temperature: 50.0,
        ..Default::default()
    });
    let run = app.action::<RunInput, _, _>("run", |input, context| async move {
        context
            .update(|state| {
                state.temperature = input.temperature;
                state.result = Some(format!("Result: {}", input.temperature));
            })
            .await?;
        Ok(())
    })?;
    app.surface("main", move |ui, state| {
        let form = ui.local(
            "form",
            RunInput {
                temperature: state.temperature,
            },
        )?;
        ui.add(Text::new("Hello").key("hello"))?;
        ui.add(
            Slider::new("temperature", 0.0, 100.0)
                .bind(form.field(RunInput::field_temperature()).bind()?),
        )?;
        ui.add(Button::new("Run").on_press(run.call(RunInputInput {
            temperature: Input::expression(form.field(RunInput::field_temperature()).value()),
        })?))?;
        if let Some(result) = &state.result {
            ui.add(Text::new(result.clone()).key("result"))?;
        }
        Ok(())
    })?;
    Ok(app.build())
}

#[tokio::test]
async fn matches_shared_fixture() {
    let fixture = fixture();
    let mut runtime = runtime().unwrap();
    assert_eq!(
        Value::Array(runtime.render("main").await.unwrap()),
        fixture["initialMessages"]
    );
    assert_eq!(
        Value::Array(runtime.handle(fixture["action"].clone()).await.unwrap()),
        fixture["afterActionMessages"]
    );
}

#[tokio::test]
async fn typed_action_is_independent_of_surface() {
    #[derive(Deserialize)]
    struct Input {
        value: i32,
    }
    let mut app = App::new(0i32);
    app.action::<Input, _, _>("set", |input, context| async move {
        context
            .update(|state| {
                *state = input.value;
            })
            .await?;
        Ok(())
    })
    .unwrap();
    let mut runtime = app.build();
    assert!(
        runtime
            .invoke("set", json!({ "value": 7 }))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(runtime.app_revision(), 1);
    assert_eq!(runtime.read(|state| *state).await, 7);
}

#[tokio::test]
async fn external_update_renders_all_surfaces() {
    let mut app = App::new(0i32);
    app.surface("one", |ui, state| {
        ui.text_with(
            state.to_string(),
            TextOptions {
                key: Some("value".into()),
                ..Default::default()
            },
        )
    })
    .unwrap();
    app.surface("two", |ui, state| {
        ui.text_with(
            state.to_string(),
            TextOptions {
                key: Some("value".into()),
                ..Default::default()
            },
        )
    })
    .unwrap();
    let mut runtime = app.build();
    runtime.render("one").await.unwrap();
    runtime.render("two").await.unwrap();
    let messages = runtime
        .update(|state| {
            *state = 1;
        })
        .await
        .unwrap();
    let surfaces: std::collections::BTreeSet<_> = messages
        .iter()
        .filter_map(|message| message.get("updateComponents")?.get("surfaceId")?.as_str())
        .collect();
    assert_eq!(surfaces, ["one", "two"].into_iter().collect());
}

#[tokio::test]
async fn local_state_resets_only_when_reset_key_changes() {
    let mut app = App::new(State {
        temperature: 50.0,
        ..Default::default()
    });
    app.surface("main", |ui, state| {
        let form = ui.local_with(
            "form",
            RunInput {
                temperature: state.temperature,
            },
            Some(json!(state.revision)),
        )?;
        ui.add(
            Slider::new("temperature", 0.0, 100.0)
                .bind(form.field(RunInput::field_temperature()).bind()?),
        )
    })
    .unwrap();
    let mut runtime = app.build();
    runtime.render("main").await.unwrap();
    assert!(
        runtime
            .update(|state| {
                state.temperature = 75.0;
            })
            .await
            .unwrap()
            .is_empty()
    );
    let messages = runtime
        .update(|state| {
            state.revision += 1;
        })
        .await
        .unwrap();
    assert_eq!(
        messages,
        vec![
            json!({ "version": "v0.9", "updateDataModel": { "surfaceId": "main", "path": "/$local/form", "value": { "temperature": 75 } } })
        ]
    );
}

#[tokio::test]
async fn get_surface_returns_full_snapshot_after_render() {
    let mut runtime = runtime().unwrap();
    runtime.render("main").await.unwrap();
    assert!(
        runtime
            .handle(json!({ "type": "getSurface", "surface": "main" }))
            .await
            .unwrap()[0]
            .get("createSurface")
            .is_some()
    );
}
