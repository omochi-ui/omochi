use omochi_ui::{App, Declaration, LocalDeclaration, Reconciler, Text};
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

fn local_declaration(local: &Value) -> LocalDeclaration {
    let declaration: LocalDeclaration = serde_json::from_value(local.clone()).unwrap();
    assert_eq!(serde_json::to_value(&declaration).unwrap(), *local);
    declaration
}

#[tokio::test]
async fn reconciles_shared_local_reset_key_presence() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../spec/fixtures/reconciliation.json"
    ))
    .unwrap();
    for case in fixture["localResetCases"].as_array().unwrap() {
        let mut reconciler = Reconciler::default();
        let mut before = declaration(&json!("unchanged"));
        before
            .locals
            .insert("form".into(), local_declaration(&case["before"]));
        let mut after = declaration(&json!("unchanged"));
        after
            .locals
            .insert("form".into(), local_declaration(&case["after"]));
        reconciler.render(before).unwrap();
        assert_eq!(
            Value::Array(reconciler.render(after.clone()).unwrap()),
            case["expectedMessages"],
            "{}",
            case["name"]
        );
        assert!(reconciler.render(after).unwrap().is_empty());

        let mut app = App::new(case["before"].clone());
        app.surface("main", |ui, local| {
            if let Some(reset_key) = local.get("resetKey") {
                ui.local_with("form", local["initial"].clone(), Some(reset_key.clone()))?;
            } else {
                ui.local("form", local["initial"].clone())?;
            }
            ui.add(Text::new("Unchanged").key("label"))
        })
        .unwrap();
        let mut runtime = app.build();
        runtime.render("main").await.unwrap();
        let next = case["after"].clone();
        assert_eq!(
            Value::Array(runtime.update(move |local| *local = next).await.unwrap()),
            case["expectedMessages"],
            "{}",
            case["name"]
        );
        assert!(runtime.render("main").await.unwrap().is_empty());
    }
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
