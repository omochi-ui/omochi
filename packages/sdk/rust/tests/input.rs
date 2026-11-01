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
    Item(Nested),
    Form(Form),
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
async fn merge_snapshots_preserve_capabilities_and_keep_processing_actions() {
    use std::sync::{Arc, Mutex};
    let mut app = App::new(0);
    let run = app
        .action::<Form, _, _>("run", |_, ctx| async move {
            ctx.update(|state| *state += 1).await
        })
        .unwrap();
    app.surface("main", move |ui, state| {
        let nested = ui.local("nested", Nested { count: 3 })?;
        ui.text(state.to_string())?;
        ui.button(
            "Run",
            run.call(FormInput {
                title: "x".into(),
                nested: nested.value().into(),
            })?,
            Default::default(),
        )
    })
    .unwrap();
    let mut runtime = app.build();
    assert!(matches!(
        runtime.surface("main").await,
        Err(omochi_ui::Error::Capability(_))
    ));
    let sent = Arc::new(Mutex::new(Vec::new()));
    let output = sent.clone();
    let mut incoming = std::collections::VecDeque::from([
        json!({"type":"getSurface", "surface":"main"}),
        json!({"type":"invoke", "action":"run", "input":{"title":"x", "count":3}}),
    ]);
    let transport = omochi_ui::CallbackTransport::new(
        move |message| {
            output.lock().unwrap().push(message);
            async { Ok(()) }
        },
        move || {
            let message = incoming.pop_front();
            async move { Ok(message) }
        },
    )
    .with_capabilities([omochi_ui::MERGE_OBJECTS_CAPABILITY]);
    runtime.start_with(transport).await.unwrap();
    assert_eq!(runtime.read(|state| *state).await, 1);
    assert_eq!(runtime.app_revision(), 1);
    let messages = sent.lock().unwrap();
    assert_eq!(messages.len(), 8); // hello, initial snapshot, getSurface snapshot, action update
    assert_eq!(&messages[1..4], &messages[4..7]);
    assert!(messages[1].get("createSurface").is_some());
    assert_eq!(
        messages[7]["updateComponents"]["components"][0]["text"],
        "1"
    );
}

#[derive(Serialize, Deserialize, ActionInput)]
enum MixedExternal {
    Tagged {
        label: String,
    },
    #[serde(untagged)]
    Raw {
        count: i64,
    },
}
#[derive(Serialize, Deserialize, ActionInput)]
#[serde(tag = "kind", content = "data")]
enum MixedAdjacent {
    Tagged {
        label: String,
    },
    #[serde(untagged)]
    Raw {
        count: i64,
    },
}
#[derive(Serialize, Deserialize, ActionInput)]
#[serde(tag = "kind")]
enum MixedInternal {
    Tagged {
        label: String,
    },
    #[serde(untagged)]
    Raw(Nested),
}

#[test]
fn per_variant_untagged_projects_serialized_fields_and_bindings() {
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let raw = MixedExternal::Raw { count: 3 };
    assert_eq!(serde_json::to_value(&raw).unwrap(), json!({"count":3}));
    let local = ui.local("external", raw).unwrap();
    assert!(local.variant(MixedExternal::variant_tagged()).is_none());
    let count = local
        .variant(MixedExternal::variant_raw())
        .unwrap()
        .field(MixedExternalRawFields::field_count());
    assert_eq!(
        omochi_ui::checked_json(count.value()).unwrap(),
        json!({"path":"/$local/external/count"})
    );
    assert_eq!(
        serde_json::Value::from(count.bind().unwrap()),
        json!({"path":"/$local/external/count"})
    );
    let local = ui
        .local("adjacent", MixedAdjacent::Raw { count: 3 })
        .unwrap();
    let count = local
        .variant(MixedAdjacent::variant_raw())
        .unwrap()
        .field(MixedAdjacentRawFields::field_count());
    assert_eq!(
        omochi_ui::checked_json(count.value()).unwrap(),
        json!({"path":"/$local/adjacent/count"})
    );
    let local = ui
        .local("internal", MixedInternal::Raw(Nested { count: 3 }))
        .unwrap();
    let payload = local
        .variant(MixedInternal::variant_raw())
        .unwrap()
        .field(MixedInternalRawFields::field_0());
    assert_eq!(
        omochi_ui::checked_json(payload.value()).unwrap(),
        json!({"path":"/$local/internal"})
    );
}

#[derive(Serialize, Deserialize, ActionInput)]
#[serde(transparent)]
struct WrappedNested(Nested);
#[derive(Serialize, Deserialize, ActionInput)]
struct WrappedForm {
    title: String,
    #[serde(flatten)]
    nested: WrappedNested,
}

#[test]
fn flatten_chains_preserve_fields_and_complete_payload_projections() {
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let local = ui
        .local(
            "wrapped",
            WrappedForm {
                title: "x".into(),
                nested: WrappedNested(Nested { count: 3 }),
            },
        )
        .unwrap();
    let count = local
        .field(WrappedForm::field_nested())
        .field(WrappedNested::field_0())
        .field(Nested::field_count());
    assert_eq!(
        omochi_ui::checked_json(count.value()).unwrap(),
        json!({"path":"/$local/wrapped/count"})
    );
    assert_eq!(
        serde_json::Value::from(count.bind().unwrap()),
        json!({"path":"/$local/wrapped/count"})
    );
    let local = ui
        .local("whole", WrappedNested(Nested { count: 3 }))
        .unwrap();
    assert_eq!(
        omochi_ui::checked_json(local.field(WrappedNested::field_0()).value()).unwrap(),
        json!({"path":"/$local/whole"})
    );
    let local = ui
        .local("tagged", Internal::Item(Nested { count: 3 }))
        .unwrap();
    let count = local
        .variant(Internal::variant_item())
        .unwrap()
        .field(InternalItemFields::field_0())
        .field(Nested::field_count());
    assert_eq!(
        omochi_ui::checked_json(count.value()).unwrap(),
        json!({"path":"/$local/tagged/count"})
    );
    let local = ui.local("newtype", Newtype("x".into())).unwrap();
    assert_eq!(
        omochi_ui::checked_json(local.field(Newtype::field_0()).value()).unwrap(),
        json!({"path":"/$local/newtype"})
    );
}

fn empty_slice(values: &[u32]) -> bool {
    values.is_empty()
}
fn empty_str(value: &str) -> bool {
    value.is_empty()
}
#[derive(Serialize, ActionInput)]
struct CoercingSkips {
    #[serde(skip_serializing_if = "empty_slice")]
    values: Vec<u32>,
    #[serde(skip_serializing_if = "empty_str")]
    label: String,
}
#[derive(Serialize, ActionInput)]
struct BorrowedSkip<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a str>,
}
#[derive(Serialize, Deserialize, ActionInput)]
struct GenericBorrowedSkip<'a, T: Serialize, const N: usize> {
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<std::borrow::Cow<'a, str>>,
    other: T,
    #[serde(skip)]
    marker: std::marker::PhantomData<[(); N]>,
}

#[test]
fn skip_helpers_preserve_coercions_and_infer_lifetimes() {
    assert_eq!(
        CoercingSkipsInput {
            values: Vec::new().into(),
            label: "".into()
        }
        .encode()
        .unwrap(),
        json!({})
    );
    assert_eq!(
        CoercingSkipsInput {
            values: vec![1, 2].into(),
            label: "x".into()
        }
        .encode()
        .unwrap(),
        serde_json::to_value(CoercingSkips {
            values: vec![1, 2],
            label: "x".into()
        })
        .unwrap()
    );
    assert_eq!(
        BorrowedSkipInput { value: None.into() }.encode().unwrap(),
        json!({})
    );
    let text = String::from("x");
    assert_eq!(
        BorrowedSkipInput {
            value: Some(text.as_str()).into()
        }
        .encode()
        .unwrap(),
        json!({"value":"x"})
    );
    let input = GenericBorrowedSkipInput::<i64, 3> {
        value: Some(std::borrow::Cow::Borrowed(text.as_str())).into(),
        other: 4.into(),
        marker: Default::default(),
    };
    assert_eq!(input.encode().unwrap(), json!({"value":"x", "other":4}));
    let mut app = App::new(());
    app.action::<GenericBorrowedSkip<'static, i64, 3>, _, _>("run", |_, _| async { Ok(()) })
        .unwrap();
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

#[test]
fn internally_tagged_newtype_preserves_tag_with_referenced_payload() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../../spec/fixtures/action-input.json")).unwrap();
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let nested = ui.local("nested", Nested { count: 3 }).unwrap();
    let encoded = InternalInput::Item(nested.value().into()).encode().unwrap();
    assert_eq!(encoded, fixture["taggedNewtypeMerge"]["expression"]);
    assert!(matches!(
        serde_json::from_value::<Internal>(fixture["taggedNewtypeMerge"]["resolved"].clone())
            .unwrap(),
        Internal::Item(Nested { count: 3 })
    ));
    assert_eq!(
        InternalInput::Item(Nested { count: 3 }.into())
            .encode()
            .unwrap(),
        json!({"kind":"Item", "count":3})
    );
    assert_eq!(
        InternalInput::Item(
            Input::fields(NestedInput {
                count: nested.field(Nested::field_count()).value().into(),
            })
            .unwrap()
        )
        .encode()
        .unwrap(),
        json!({"kind":"Item", "count":{"path":"/$local/nested/count"}})
    );
    let form = Input::fields(FormInput {
        title: "x".into(),
        nested: nested.value().into(),
    })
    .unwrap();
    assert_eq!(
        InternalInput::Form(form).encode().unwrap(),
        json!({"call":"omochiMergeObjects","args":{"objects":[{"kind":"Form"},fixture["merge"]["expression"]]},"returnType":"object"})
    );
}

#[derive(Serialize, Deserialize, ActionInput)]
enum BoundedEnum<T = String>
where
    T: Clone,
{
    Value(T),
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    value == &T::default()
}

#[derive(Serialize, ActionInput)]
struct BoundedOptional<T = String>
where
    T: Default + PartialEq,
{
    #[serde(skip_serializing_if = "is_default")]
    value: T,
}

#[test]
fn generated_helpers_preserve_where_clauses() {
    assert_eq!(
        BoundedEnumInput::Value(3i64.into()).encode().unwrap(),
        json!({"Value":3})
    );
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let local = ui.local("bounded", BoundedEnum::Value(3i64)).unwrap();
    let variant = local.variant(BoundedEnum::<i64>::variant_value()).unwrap();
    assert_eq!(
        omochi_ui::checked_json(
            variant
                .field(BoundedEnumValueFields::<i64>::field_0())
                .value()
        )
        .unwrap(),
        json!({"path":"/$local/bounded/Value"})
    );
    assert_eq!(
        BoundedOptionalInput::<String> { value: "".into() }
            .encode()
            .unwrap(),
        json!({})
    );
    assert_eq!(
        BoundedOptionalInput::<String> { value: "x".into() }
            .encode()
            .unwrap(),
        json!({"value":"x"})
    );
}

#[derive(Serialize, ActionInput)]
struct ConditionalTuple(
    #[serde(skip)] (),
    String,
    #[serde(skip_serializing_if = "Option::is_none")] Option<String>,
    String,
);

#[derive(Serialize, Deserialize, ActionInput)]
enum ConditionalVariant {
    Item(
        #[serde(skip)] (),
        String,
        #[serde(skip_serializing_if = "Option::is_none")] Option<String>,
        String,
    ),
}

#[test]
fn conditional_tuple_inputs_keep_encoding_and_stable_projections() {
    for value in [None, Some("optional".to_owned())] {
        let expected = omochi_ui::checked_json(ConditionalTuple(
            (),
            "prefix".into(),
            value.clone(),
            "actual".into(),
        ))
        .unwrap();
        assert_eq!(
            ConditionalTupleInput(
                ().into(),
                "prefix".into(),
                value.clone().into(),
                "actual".into()
            )
            .encode()
            .unwrap(),
            expected
        );
        assert_eq!(
            ConditionalVariantInput::Item(
                ().into(),
                "prefix".into(),
                value.clone().into(),
                "actual".into()
            )
            .encode()
            .unwrap(),
            json!({"Item":expected})
        );
    }
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let tuple = ui
        .local(
            "tuple",
            ConditionalTuple((), "prefix".into(), None, "actual".into()),
        )
        .unwrap();
    assert_eq!(
        omochi_ui::checked_json(tuple.field(ConditionalTuple::field_1()).value()).unwrap(),
        json!({"path":"/$local/tuple/0"})
    );
    let local = ui
        .local(
            "variant",
            ConditionalVariant::Item(
                (),
                "prefix".into(),
                Some("optional".into()),
                "actual".into(),
            ),
        )
        .unwrap();
    let variant = local.variant(ConditionalVariant::variant_item()).unwrap();
    assert_eq!(
        omochi_ui::checked_json(
            variant
                .field(ConditionalVariantItemFields::field_1())
                .value()
        )
        .unwrap(),
        json!({"path":"/$local/variant/Item/0"})
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
