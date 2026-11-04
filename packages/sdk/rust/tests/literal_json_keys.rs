use omochi_ui::{Declaration, Reconciler, TextFieldOptions, Ui};
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../spec/fixtures/literal-json-keys.json"
    ))
    .unwrap()
}

fn declaration(fixture: &Value, root: &Value, state: &Value) -> Declaration {
    serde_json::from_value(json!({
        "irVersion": "2", "surfaceId": "main", "catalogId": fixture["catalogId"],
        "root": root, "data": state["data"], "locals": state["locals"],
        "requiredCapabilities": [],
    }))
    .unwrap()
}

#[test]
fn reconciles_shared_literal_json_keys() {
    let fixture = fixture();
    let mut reconciler = Reconciler::default();
    for step in fixture["steps"].as_array().unwrap() {
        let value = declaration(&fixture, &fixture["root"], &step["state"]);
        assert_eq!(
            Value::Array(reconciler.render(value.clone()).unwrap()),
            step["expectedMessages"],
            "{}",
            step["name"]
        );
        assert!(reconciler.render(value).unwrap().is_empty());
    }
}

#[test]
fn public_builder_preserves_literal_json_keys() {
    let fixture = fixture();
    let mut ui = Ui::new("main", fixture["catalogId"].as_str().unwrap());
    for key in fixture["keys"].as_array().unwrap() {
        let key = key.as_str().unwrap();
        ui.local(key, fixture["builder"]["initials"][key].clone())
            .unwrap();
        assert!(ui.local(key, "duplicate").is_err());
        ui.text_field(
            key,
            TextFieldOptions {
                label: key.into(),
                default: Some(fixture["builder"]["defaults"][key].as_str().unwrap().into()),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let declaration = ui.build();
    let serialized = serde_json::to_value(&declaration).unwrap();
    assert_eq!(
        serialized["data"],
        json!({"inputs": fixture["builder"]["defaults"]})
    );
    let locals: serde_json::Map<String, Value> = fixture["builder"]["initials"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), json!({"initial": value})))
        .collect();
    assert_eq!(serialized["locals"], Value::Object(locals));
    let messages = Reconciler::default().render(declaration).unwrap();
    assert_eq!(
        messages.last().unwrap()["updateDataModel"],
        json!({
            "surfaceId": "main", "path": "/", "value": {
                "inputs": fixture["builder"]["defaults"], "$local": fixture["builder"]["initials"],
            },
        })
    );
}

#[test]
fn flattens_shared_literal_slot_pointers() {
    let fixture = fixture();
    let value = declaration(
        &fixture,
        &fixture["flatten"]["root"],
        &json!({"data": {}, "locals": {}}),
    );
    let messages = Reconciler::default().render(value).unwrap();
    assert_eq!(
        messages[1]["updateComponents"],
        json!({
            "surfaceId": "main", "components": fixture["flatten"]["expectedComponents"],
        })
    );
}
