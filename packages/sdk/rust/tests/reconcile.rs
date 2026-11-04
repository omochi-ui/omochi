use omochi_ui::{Declaration, Reconciler};
use serde_json::{Value, json};

fn declaration(value: &Value) -> Declaration {
    serde_json::from_value(json!({
        "irVersion": "2",
        "surfaceId": "main",
        "catalogId": "https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json",
        "root": {
            "key": "root", "component": "Button",
            "props": { "action": { "event": { "name": "set", "context": { "input": value } } } },
            "slots": [{ "path": "/child", "kind": "single", "nodes": [
                { "key": "label", "component": "Text", "props": { "text": "Set" } }
            ] }]
        },
        "data": { "value": value },
        "locals": { "form": { "initial": { "draft": value }, "resetKey": value } },
        "requiredCapabilities": []
    }))
    .unwrap()
}

#[test]
fn reconciles_shared_json_type_changes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../spec/fixtures/reconciliation.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut reconciler = Reconciler::default();
        reconciler.render(declaration(&case["before"])).unwrap();
        assert_eq!(
            Value::Array(reconciler.render(declaration(&case["after"])).unwrap()),
            case["expectedMessages"],
            "{}",
            case["name"]
        );
        assert!(
            reconciler
                .render(declaration(&case["after"]))
                .unwrap()
                .is_empty()
        );
    }
}
