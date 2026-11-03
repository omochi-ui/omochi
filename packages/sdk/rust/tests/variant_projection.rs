#![allow(dead_code)]
use omochi_ui::{ActionInput, CheckBox, STANDARD_CATALOG, Ui};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Deserialize, ActionInput)]
enum External {
    Unit,
    Record {
        #[serde(skip_serializing)]
        required: String,
        enabled: bool,
    },
    Tuple(#[serde(skip_serializing)] String, bool),
}

#[derive(Serialize, Deserialize, ActionInput)]
#[serde(tag = "kind")]
enum Internal {
    Unit,
    Record {
        #[serde(skip_serializing)]
        required: String,
        enabled: bool,
    },
}

#[derive(Serialize, Deserialize, ActionInput)]
#[serde(tag = "kind", content = "payload")]
enum Adjacent {
    Unit,
    Record {
        #[serde(skip_serializing)]
        required: String,
        enabled: bool,
    },
}

#[test]
fn skipped_serializing_fields_do_not_prevent_variant_projection() {
    let mut ui = Ui::new("main", STANDARD_CATALOG);
    let initial = External::Record {
        required: "secret".into(),
        enabled: false,
    };
    assert!(serde_json::from_value::<External>(serde_json::to_value(&initial).unwrap()).is_err());
    let local = ui.local("external", initial).unwrap();
    assert!(local.variant(External::variant_unit()).is_none());
    assert!(local.variant(External::variant_tuple()).is_none());
    let enabled = local
        .variant(External::variant_record())
        .unwrap()
        .field(ExternalRecordFields::field_enabled());
    assert_eq!(
        Value::from(enabled.bind().unwrap()),
        json!({"path":"/$local/external/Record/enabled"})
    );
    ui.add(CheckBox::new("enabled", "Enabled").bind(enabled.bind().unwrap()))
        .unwrap();

    let local = ui
        .local("tuple", External::Tuple("secret".into(), true))
        .unwrap();
    assert!(local.variant(External::variant_record()).is_none());
    let enabled = local
        .variant(External::variant_tuple())
        .unwrap()
        .field(ExternalTupleFields::field_1());
    assert_eq!(
        Value::from(enabled.bind().unwrap()),
        json!({"path":"/$local/tuple/Tuple/0"})
    );

    let initial = Internal::Record {
        required: "secret".into(),
        enabled: false,
    };
    assert!(serde_json::from_value::<Internal>(serde_json::to_value(&initial).unwrap()).is_err());
    let local = ui.local("internal", initial).unwrap();
    assert!(local.variant(Internal::variant_unit()).is_none());
    let enabled = local
        .variant(Internal::variant_record())
        .unwrap()
        .field(InternalRecordFields::field_enabled());
    assert_eq!(
        Value::from(enabled.bind().unwrap()),
        json!({"path":"/$local/internal/enabled"})
    );

    let initial = Adjacent::Record {
        required: "secret".into(),
        enabled: false,
    };
    assert!(serde_json::from_value::<Adjacent>(serde_json::to_value(&initial).unwrap()).is_err());
    let local = ui.local("adjacent", initial).unwrap();
    assert!(local.variant(Adjacent::variant_unit()).is_none());
    let enabled = local
        .variant(Adjacent::variant_record())
        .unwrap()
        .field(AdjacentRecordFields::field_enabled());
    assert_eq!(
        Value::from(enabled.bind().unwrap()),
        json!({"path":"/$local/adjacent/payload/enabled"})
    );
}

#[derive(Serialize, Deserialize, ActionInput)]
#[serde(rename_all(serialize = "snake_case", deserialize = "SCREAMING_SNAKE_CASE"))]
enum Renamed {
    UnitValue,
    #[serde(rename(serialize = "out/~", deserialize = "incoming"))]
    Record {
        #[serde(rename(serialize = "is/enabled", deserialize = "enabled"))]
        enabled: bool,
    },
}

#[test]
fn projection_uses_serialization_names() {
    let mut ui = Ui::new("main", STANDARD_CATALOG);
    let local = ui.local("unit", Renamed::UnitValue).unwrap();
    assert!(local.variant(Renamed::variant_unit_value()).is_some());
    assert!(local.variant(Renamed::variant_record()).is_none());
    let local = ui
        .local("record", Renamed::Record { enabled: true })
        .unwrap();
    assert!(local.variant(Renamed::variant_unit_value()).is_none());
    let enabled = local
        .variant(Renamed::variant_record())
        .unwrap()
        .field(RenamedRecordFields::field_enabled());
    assert_eq!(
        Value::from(enabled.bind().unwrap()),
        json!({"path":"/$local/record/out~1~0/is~1enabled"})
    );
}

#[derive(Serialize, ActionInput)]
struct EncodeOnlyPayload {
    enabled: bool,
}

#[derive(Serialize, ActionInput)]
enum EncodeOnlyExternal<T> {
    Unit,
    Value(T),
}

#[derive(Serialize, ActionInput)]
#[serde(
    tag = "kind",
    rename_all(serialize = "snake_case", deserialize = "SCREAMING_SNAKE_CASE")
)]
enum EncodeOnlyInternal<'a> {
    UnitValue,
    Record { label: &'a str },
}

#[derive(Serialize, ActionInput)]
#[serde(tag = "kind", content = "data/~")]
enum EncodeOnlyAdjacent {
    Unit,
    #[serde(rename(serialize = "outgoing", deserialize = "incoming"))]
    Value(EncodeOnlyPayload),
}

#[test]
fn tagged_projection_supports_serialize_only_and_borrowed_payloads() {
    let mut ui = Ui::new("main", STANDARD_CATALOG);
    let local = ui
        .local(
            "external",
            EncodeOnlyExternal::Value(EncodeOnlyPayload { enabled: true }),
        )
        .unwrap();
    assert!(local.variant(EncodeOnlyExternal::variant_unit()).is_none());
    let enabled = local
        .variant(EncodeOnlyExternal::variant_value())
        .unwrap()
        .field(EncodeOnlyExternalValueFields::field_0())
        .field(EncodeOnlyPayload::field_enabled());
    assert_eq!(
        Value::from(enabled.bind().unwrap()),
        json!({"path":"/$local/external/Value/enabled"})
    );
    let local = ui
        .local(
            "externalUnit",
            EncodeOnlyExternal::<EncodeOnlyPayload>::Unit,
        )
        .unwrap();
    assert!(local.variant(EncodeOnlyExternal::variant_unit()).is_some());
    assert!(local.variant(EncodeOnlyExternal::variant_value()).is_none());

    let label = "borrowed".to_owned();
    let local = ui
        .local("internal", EncodeOnlyInternal::Record { label: &label })
        .unwrap();
    assert!(
        local
            .variant(EncodeOnlyInternal::variant_record())
            .is_some()
    );
    assert!(
        local
            .variant(EncodeOnlyInternal::variant_unit_value())
            .is_none()
    );
    let local = ui
        .local("internalUnit", EncodeOnlyInternal::UnitValue)
        .unwrap();
    assert!(
        local
            .variant(EncodeOnlyInternal::variant_unit_value())
            .is_some()
    );
    assert!(
        local
            .variant(EncodeOnlyInternal::variant_record())
            .is_none()
    );

    let local = ui
        .local(
            "adjacent",
            EncodeOnlyAdjacent::Value(EncodeOnlyPayload { enabled: true }),
        )
        .unwrap();
    assert!(local.variant(EncodeOnlyAdjacent::variant_unit()).is_none());
    let enabled = local
        .variant(EncodeOnlyAdjacent::variant_value())
        .unwrap()
        .field(EncodeOnlyAdjacentValueFields::field_0())
        .field(EncodeOnlyPayload::field_enabled());
    assert_eq!(
        Value::from(enabled.bind().unwrap()),
        json!({"path":"/$local/adjacent/data~1~0/enabled"})
    );
    let local = ui.local("adjacentUnit", EncodeOnlyAdjacent::Unit).unwrap();
    assert!(local.variant(EncodeOnlyAdjacent::variant_unit()).is_some());
    assert!(local.variant(EncodeOnlyAdjacent::variant_value()).is_none());
}
