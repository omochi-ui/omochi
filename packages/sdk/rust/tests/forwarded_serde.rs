#![allow(dead_code)]
use omochi_ui::{ActionInput, STANDARD_CATALOG, Ui, checked_json};
use serde::Serialize;
use serde_json::json;

// `literal` fragments reach derive macros inside invisible expression groups.
macro_rules! forwarded_types {
    ($rule:literal, $rename:literal, $tag:literal, $content:literal) => {
        #[derive(Serialize, ActionInput)]
        #[serde(rename_all = $rule)]
        struct Form {
            value: String,
            #[serde(rename = $rename)]
            direct: String,
            #[serde(rename(serialize = $content, deserialize = "incoming"))]
            nested: String,
        }

        #[derive(Serialize, ActionInput)]
        #[serde(rename_all(serialize = $rule), rename_all_fields(serialize = $rule))]
        enum External {
            Record {
                value: String,
            },
            #[serde(rename(serialize = $rename))]
            Renamed {
                #[serde(rename(serialize = $rename))]
                value: String,
            },
            Unit,
        }

        #[derive(Serialize, ActionInput)]
        #[serde(tag = $tag)]
        enum Internal {
            Record { value: String },
            Unit,
        }

        #[derive(Serialize, ActionInput)]
        #[serde(tag = $tag, content = $content)]
        enum Adjacent {
            Record { value: String },
            Unit,
        }
    };
}
forwarded_types!("UPPERCASE", "wire/~", "kind/~", "payload/~");

macro_rules! forwarded_predicate {
    ($predicate:literal) => {
        #[derive(Serialize, ActionInput)]
        struct Conditional {
            #[serde(skip_serializing_if = $predicate)]
            value: Option<String>,
        }
    };
}
forwarded_predicate!("Option::is_none");

#[test]
fn forwarded_skip_predicates_preserve_literal_and_reference_behavior() {
    for value in [None, Some("text".to_owned())] {
        assert_eq!(
            ConditionalInput {
                value: value.clone().into()
            }
            .encode()
            .unwrap(),
            serde_json::to_value(Conditional { value }).unwrap()
        );
    }
    let mut ui = Ui::new("main", STANDARD_CATALOG);
    let local = ui.local("value", None::<String>).unwrap();
    assert!(
        ConditionalInput {
            value: local.value().into()
        }
        .encode()
        .unwrap_err()
        .to_string()
        .contains("skip_serializing_if cannot evaluate an unresolved input")
    );
}

#[test]
fn forwarded_field_names_match_serde() {
    let form = Form {
        value: "rule".into(),
        direct: "direct".into(),
        nested: "nested".into(),
    };
    let serialized = serde_json::to_value(&form).unwrap();
    assert_eq!(
        serialized,
        json!({"VALUE":"rule", "wire/~":"direct", "payload/~":"nested"})
    );
    let mut ui = Ui::new("main", STANDARD_CATALOG);
    let local = ui.local("form", form).unwrap();
    assert_eq!(
        checked_json(local.field(Form::field_value()).value()).unwrap(),
        json!({"path":"/$local/form/VALUE"})
    );
    for (field, path) in [
        (Form::field_direct(), "/$local/form/wire~1~0"),
        (Form::field_nested(), "/$local/form/payload~1~0"),
    ] {
        assert_eq!(
            checked_json(local.field(field).value()).unwrap(),
            json!({"path":path})
        );
    }
}

#[test]
fn forwarded_variant_names_and_nested_serialize_rules_match_serde() {
    let mut ui = Ui::new("main", STANDARD_CATALOG);
    let record = External::Record { value: "x".into() };
    assert_eq!(
        serde_json::to_value(&record).unwrap(),
        json!({"RECORD":{"VALUE":"x"}})
    );
    let local = ui.local("record", record).unwrap();
    assert!(local.variant(External::variant_unit()).is_none());
    let payload = local.variant(External::variant_record()).unwrap();
    assert_eq!(
        checked_json(payload.field(ExternalRecordFields::field_value()).value()).unwrap(),
        json!({"path":"/$local/record/RECORD/VALUE"})
    );

    let renamed = External::Renamed { value: "x".into() };
    assert_eq!(
        serde_json::to_value(&renamed).unwrap(),
        json!({"wire/~":{"wire/~":"x"}})
    );
    let local = ui.local("renamed", renamed).unwrap();
    let payload = local.variant(External::variant_renamed()).unwrap();
    assert_eq!(
        checked_json(payload.field(ExternalRenamedFields::field_value()).value()).unwrap(),
        json!({"path":"/$local/renamed/wire~1~0/wire~1~0"})
    );
    let local = ui.local("unit", External::Unit).unwrap();
    assert!(local.variant(External::variant_unit()).is_some());
    assert!(local.variant(External::variant_record()).is_none());
}

#[test]
fn forwarded_tag_and_content_preserve_variant_paths() {
    let mut ui = Ui::new("main", STANDARD_CATALOG);
    let record = Internal::Record { value: "x".into() };
    assert_eq!(
        serde_json::to_value(&record).unwrap(),
        json!({"kind/~":"Record", "value":"x"})
    );
    let local = ui.local("internal", record).unwrap();
    assert!(local.variant(Internal::variant_unit()).is_none());
    let payload = local.variant(Internal::variant_record()).unwrap();
    assert_eq!(
        checked_json(payload.field(InternalRecordFields::field_value()).value()).unwrap(),
        json!({"path":"/$local/internal/value"})
    );

    let record = Adjacent::Record { value: "x".into() };
    assert_eq!(
        serde_json::to_value(&record).unwrap(),
        json!({"kind/~":"Record", "payload/~":{"value":"x"}})
    );
    let local = ui.local("adjacent", record).unwrap();
    assert!(local.variant(Adjacent::variant_unit()).is_none());
    let payload = local.variant(Adjacent::variant_record()).unwrap();
    assert_eq!(
        checked_json(payload.field(AdjacentRecordFields::field_value()).value()).unwrap(),
        json!({"path":"/$local/adjacent/payload~1~0/value"})
    );
}
