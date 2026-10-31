#![allow(dead_code)]
use omochi_ui::{ActionInput, App, Input};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize, Deserialize, ActionInput)]
struct Nested {
    count: i64,
}
#[derive(Serialize, Deserialize, ActionInput)]
struct Form {
    title: String,
    #[serde(flatten)]
    nested: Nested,
}
#[derive(Serialize, Deserialize, ActionInput)]
enum External {
    Unit,
    Tuple(i64, String),
    Record { count: i64 },
}
#[derive(Serialize, Deserialize, ActionInput)]
#[serde(tag = "kind")]
enum Internal {
    Unit,
    Record { count: i64 },
}
#[derive(Serialize, Deserialize, ActionInput)]
#[serde(tag = "kind", content = "data")]
enum Adjacent {
    Unit,
    Tuple(i64, String),
}
#[derive(Serialize, Deserialize, ActionInput)]
#[serde(untagged)]
enum Untagged {
    Number(i64),
    Record { count: i64 },
}
#[derive(Serialize, Deserialize, ActionInput)]
struct Tuple(i64, String);
#[derive(Serialize, Deserialize, ActionInput)]
struct Newtype(String);
#[derive(Serialize, Deserialize, ActionInput)]
struct Unit;
#[derive(Serialize, Deserialize, ActionInput)]
struct Generic<T> {
    value: T,
}
#[derive(Serialize, Deserialize, ActionInput)]
struct Optional {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    value: Option<String>,
}

#[test]
fn generated_forms_preserve_serde_shapes() {
    assert_eq!(ExternalInput::Unit.encode().unwrap(), json!("Unit"));
    assert_eq!(
        ExternalInput::Tuple(1.into(), "x".to_owned().into())
            .encode()
            .unwrap(),
        json!({"Tuple": [1, "x"]})
    );
    assert_eq!(
        InternalInput::Record { count: 3.into() }.encode().unwrap(),
        json!({"kind":"Record", "count":3})
    );
    assert_eq!(
        AdjacentInput::Tuple(1.into(), "x".to_owned().into())
            .encode()
            .unwrap(),
        json!({"kind":"Tuple", "data":[1,"x"]})
    );
    assert_eq!(UntaggedInput::Number(1.into()).encode().unwrap(), json!(1));
    assert_eq!(
        TupleInput(1.into(), "x".to_owned().into())
            .encode()
            .unwrap(),
        json!([1, "x"])
    );
    assert_eq!(
        NewtypeInput("x".to_owned().into()).encode().unwrap(),
        json!("x")
    );
    assert_eq!(UnitInput.encode().unwrap(), json!(null));
    assert_eq!(
        GenericInput {
            value: Input::Literal(3i64)
        }
        .encode()
        .unwrap(),
        json!({"value":3})
    );
    assert_eq!(
        OptionalInput { value: None.into() }.encode().unwrap(),
        json!({})
    );
}

#[test]
fn flattened_fields_and_nested_expressions() {
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let form = ui
        .local(
            "form",
            Form {
                title: "x".into(),
                nested: Nested { count: 1 },
            },
        )
        .unwrap();
    let count = form
        .field(Form::field_nested())
        .field(Nested::field_count());
    let input = FormInput {
        title: "x".to_owned().into(),
        nested: Input::fields(NestedInput {
            count: count.value().into(),
        })
        .unwrap(),
    };
    assert_eq!(
        input.encode().unwrap(),
        json!({"title":"x", "count":{"path":"/$local/form/count"}})
    );
}

#[tokio::test]
async fn handler_does_not_run_on_invalid_wire_input() {
    let mut app = App::new(0);
    app.action::<Nested, _, _>("run", |_, ctx| async move { ctx.update(|s| *s += 1).await })
        .unwrap();
    let mut runtime = app.build();
    assert!(runtime.invoke("run", json!({"count":"bad"})).await.is_err());
    assert_eq!(runtime.read(|s| *s).await, 0);
}

#[derive(Serialize, Deserialize, ActionInput)]
#[serde(rename_all = "camelCase")]
struct Renamed {
    long_name: String,
}
#[derive(Serialize, Deserialize, ActionInput)]
struct Collision {
    count: i64,
    #[serde(flatten)]
    nested: Nested,
}

#[test]
fn rejects_duplicate_keys_and_nonfinite_numbers() {
    assert!(
        CollisionInput {
            count: 1.into(),
            nested: Nested { count: 2 }.into()
        }
        .encode()
        .is_err()
    );
    assert!(omochi_ui::checked_json(f64::NAN).is_err());
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let local = ui
        .local(
            "x",
            Renamed {
                long_name: "x".into(),
            },
        )
        .unwrap();
    assert_eq!(
        omochi_ui::checked_json(local.field(Renamed::field_long_name()).value()).unwrap(),
        json!({"path":"/$local/x/longName"})
    );
}

struct StringAdapter;
impl omochi_ui::ExpressionAdapter<i64> for StringAdapter {
    type Wire = String;
    fn encode_literal(value: &i64) -> omochi_ui::Result<serde_json::Value> {
        Ok(json!(value.to_string()))
    }
    fn project(path: &str) -> String {
        path.into()
    }
}
fn number_string<S: serde::Serializer>(n: &i64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&n.to_string())
}
#[derive(Serialize, ActionInput)]
struct Adapted {
    #[serde(serialize_with = "number_string")]
    #[action_input(adapter = StringAdapter)]
    value: i64,
}
#[test]
fn explicit_adapter_preserves_custom_wire_shape() {
    assert_eq!(
        AdaptedInput { value: 42.into() }.encode().unwrap(),
        json!({"value":"42"})
    );
}

#[test]
fn shared_validation_fixture() {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Validated {
        temperature: f64,
        note: Option<String>,
    }
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../../spec/fixtures/action-input.json")).unwrap();
    for v in fixture["valid"].as_array().unwrap() {
        assert!(serde_json::from_value::<Validated>(v.clone()).is_ok());
    }
    for v in fixture["invalid"].as_array().unwrap() {
        assert!(serde_json::from_value::<Validated>(v.clone()).is_err());
    }
}

#[test]
fn enum_projection_requires_matching_variant() {
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let local = ui.local("form", Internal::Record { count: 3 }).unwrap();
    assert!(local.variant(Internal::variant_unit()).is_none());
    let record = local.variant(Internal::variant_record()).unwrap();
    let count = record.field(InternalRecordFields::field_count()).value();
    assert_eq!(
        omochi_ui::checked_json(count).unwrap(),
        json!({"path":"/$local/form/count"})
    );
}

#[derive(Serialize, Deserialize, ActionInput)]
struct GenericOptional<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    value: Option<T>,
}
#[derive(Serialize, Deserialize, ActionInput)]
#[serde(transparent)]
struct Transparent {
    value: String,
}
#[derive(Serialize, ActionInput)]
struct Skipped {
    #[serde(skip)]
    resource: std::sync::Mutex<()>,
    count: i64,
}
#[test]
fn generic_skip_and_transparent_input() {
    assert_eq!(
        GenericOptionalInput::<String> { value: None.into() }
            .encode()
            .unwrap(),
        json!({})
    );
    assert_eq!(
        TransparentInput { value: "x".into() }.encode().unwrap(),
        json!("x")
    );
    assert_eq!(
        SkippedInput {
            resource: std::sync::Mutex::new(()).into(),
            count: 1.into()
        }
        .encode()
        .unwrap(),
        json!({"count":1})
    );
}

#[derive(Serialize, Deserialize, ActionInput)]
enum GenericEnum<T> {
    Record { value: T },
}
#[test]
fn nested_arrays_and_generic_variant_references() {
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let local = ui
        .local("form", GenericEnum::Record { value: 3i64 })
        .unwrap();
    let variant = local.variant(GenericEnum::<i64>::variant_record()).unwrap();
    let reference = variant
        .field(GenericEnumRecordFields::<i64>::field_value())
        .value();
    let values: Input<Vec<i64>> = Input::fields(vec![Input::Literal(1), reference.into()]).unwrap();
    assert_eq!(
        omochi_ui::checked_json(values).unwrap(),
        json!([1,{"path":"/$local/form/Record/value"}])
    );
}

#[derive(Serialize, ActionInput)]
struct GenericTwo<T, U> {
    #[serde(skip_serializing_if = "Option::is_none")]
    first: Option<T>,
    second: U,
}
#[test]
fn skip_predicate_with_multiple_type_parameters() {
    assert_eq!(
        GenericTwoInput::<String, i64> {
            first: None.into(),
            second: 1.into()
        }
        .encode()
        .unwrap(),
        json!({"second":1})
    );
}

#[test]
fn flattened_whole_expression_is_deferred() {
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let nested = ui.local("nested", Nested { count: 1 }).unwrap();
    assert_eq!(
        FormInput {
            title: "x".into(),
            nested: nested.value().into()
        }
        .encode()
        .unwrap(),
        json!({"call":"omochiMergeObjects","args":{"objects":[{"title":"x"},{"path":"/$local/nested"}]},"returnType":"object"})
    );
}

#[derive(Serialize, Deserialize, ActionInput)]
struct SkippedTuple(#[serde(skip)] (), String);
#[derive(Serialize, Deserialize, ActionInput)]
enum SkippedVariant {
    Item(#[serde(skip)] (), String),
    Raw { r#type: String },
}
#[derive(Serialize, Deserialize, ActionInput)]
struct Raw {
    r#type: String,
}

#[test]
fn serialized_tuple_positions_and_raw_names() {
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let value = ui.local("tuple", SkippedTuple((), "x".into())).unwrap();
    assert_eq!(
        omochi_ui::checked_json(value.field(SkippedTuple::field_1()).value()).unwrap(),
        json!({"path":"/$local/tuple/0"})
    );
    let raw = ui.local("raw", Raw { r#type: "x".into() }).unwrap();
    assert_eq!(
        omochi_ui::checked_json(raw.field(Raw::field_type()).value()).unwrap(),
        json!({"path":"/$local/raw/type"})
    );
    let variant = ui.local("v", SkippedVariant::Item((), "x".into())).unwrap();
    let v = variant.variant(SkippedVariant::variant_item()).unwrap();
    assert_eq!(
        omochi_ui::checked_json(v.field(SkippedVariantItemFields::field_1()).value()).unwrap(),
        json!({"path":"/$local/v/Item/0"})
    );
    let _ = SkippedVariantRawFields::field_type();
}

#[derive(Serialize, Deserialize, ActionInput)]
struct DynamicForm {
    title: String,
    #[serde(flatten)]
    values: std::collections::BTreeMap<String, i64>,
}
#[derive(Serialize, Deserialize, ActionInput)]
#[serde(tag = "kind")]
enum FlatEnum {
    Item {
        title: String,
        #[serde(flatten)]
        nested: Nested,
    },
}

#[test]
fn dynamic_map_and_tagged_enum_flatten() {
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let values = ui
        .local(
            "values",
            std::collections::BTreeMap::from([("count".to_owned(), 3i64)]),
        )
        .unwrap();
    let encoded = DynamicFormInput {
        title: "x".into(),
        values: values.value().into(),
    }
    .encode()
    .unwrap();
    assert_eq!(
        encoded["args"]["objects"][1],
        json!({"path":"/$local/values"})
    );
    let nested = ui.local("nested", Nested { count: 2 }).unwrap();
    let encoded = FlatEnumInput::Item {
        title: "x".into(),
        nested: nested.value().into(),
    }
    .encode()
    .unwrap();
    assert_eq!(
        encoded["args"]["objects"][0],
        json!({"kind":"Item", "title":"x"})
    );
}

#[test]
fn conditional_omission_rejects_unresolved_values() {
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let value = ui.local("value", Some("x".to_owned())).unwrap();
    assert!(
        OptionalInput {
            value: value.value().into()
        }
        .encode()
        .unwrap_err()
        .to_string()
        .contains("skip_serializing_if")
    );
}

#[test]
fn merge_matches_shared_fixture_and_requires_capability() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../../spec/fixtures/action-input.json")).unwrap();
    let mut app = App::new(());
    let run = app
        .action::<Form, _, _>("run", |_, _| async { Ok(()) })
        .unwrap();
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let nested = ui.local("nested", Nested { count: 3 }).unwrap();
    let call = run
        .call(FormInput {
            title: "x".into(),
            nested: nested.value().into(),
        })
        .unwrap();

    ui.button("Run", call, Default::default()).unwrap();
    let declaration = ui.build();
    assert_eq!(
        declaration.root.slots[0].nodes[0].props["action"]["event"]["context"]["input"],
        fixture["merge"]["expression"]
    );
    assert!(
        declaration
            .required_capabilities
            .contains(omochi_ui::MERGE_OBJECTS_CAPABILITY)
    );
    assert!(
        omochi_ui::Reconciler::default()
            .render(declaration)
            .is_err()
    );
}

#[derive(Serialize, Deserialize, ActionInput)]
struct RecursiveFlat {
    label: String,
    #[serde(flatten)]
    form: Form,
}
#[derive(Serialize, Deserialize, ActionInput)]
enum ExternalFlat {
    Item {
        title: String,
        #[serde(flatten)]
        nested: Nested,
    },
}
#[derive(Serialize, Deserialize, ActionInput)]
#[serde(tag = "kind", content = "data")]
enum AdjacentFlat {
    Item {
        title: String,
        #[serde(flatten)]
        nested: Nested,
    },
}
#[derive(Serialize, Deserialize, ActionInput)]
#[serde(untagged)]
enum UntaggedFlat {
    Item {
        title: String,
        #[serde(flatten)]
        nested: Nested,
    },
}

#[test]
fn recursive_and_enum_flatten_preserve_their_outer_shape() {
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let nested = ui.local("nested", Nested { count: 3 }).unwrap();
    let nested_form = Input::fields(FormInput {
        title: "x".into(),
        nested: nested.value().into(),
    })
    .unwrap();
    let result = RecursiveFlatInput {
        label: "outer".into(),
        form: nested_form,
    }
    .encode()
    .unwrap();
    assert_eq!(result["args"]["objects"][0], json!({"label":"outer"}));
    assert_eq!(result["args"]["objects"][1]["call"], "omochiMergeObjects");
    let external = ExternalFlatInput::Item {
        title: "x".into(),
        nested: nested.value().into(),
    }
    .encode()
    .unwrap();
    assert_eq!(external["Item"]["call"], "omochiMergeObjects");
    let adjacent = AdjacentFlatInput::Item {
        title: "x".into(),
        nested: nested.value().into(),
    }
    .encode()
    .unwrap();
    assert_eq!(adjacent["kind"], "Item");
    assert_eq!(adjacent["data"]["call"], "omochiMergeObjects");
    let untagged = UntaggedFlatInput::Item {
        title: "x".into(),
        nested: nested.value().into(),
    }
    .encode()
    .unwrap();
    assert_eq!(untagged["call"], "omochiMergeObjects");
}
