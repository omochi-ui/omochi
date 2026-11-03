#![allow(dead_code)]
use omochi_ui::{ActionInput, Field, Input};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize, ActionInput)]
struct Node {
    next: Option<Box<Self>>,
}

#[derive(Serialize, ActionInput)]
struct GenericNode<T> {
    value: T,
    #[serde(skip_serializing_if = "Self::has_no_next")]
    next: Option<Box<Self>>,
}

impl<T> GenericNode<T> {
    fn has_no_next(next: &Option<Box<Self>>) -> bool {
        next.is_none()
    }
}

#[derive(Serialize, ActionInput)]
struct SelfPredicate {
    #[serde(skip_serializing_if = "Self::is_empty")]
    value: Option<u32>,
}

impl SelfPredicate {
    fn is_empty(value: &Option<u32>) -> bool {
        value.is_none()
    }
}

#[test]
fn self_skip_predicates_preserve_literal_omission() {
    for value in [None, Some(7)] {
        let encoded = SelfPredicateInput {
            value: value.into(),
        }
        .encode()
        .unwrap();
        assert_eq!(
            encoded,
            serde_json::to_value(SelfPredicate { value }).unwrap()
        );
    }
}

#[derive(Serialize, ActionInput)]
#[serde(rename = "not_transparent")]
struct RenamedRecord {
    value: String,
}

#[derive(Serialize, ActionInput)]
#[serde(transparent)]
struct TransparentRecord {
    value: String,
}

#[test]
fn only_the_transparent_attribute_removes_the_field_projection_segment() {
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let renamed = ui
        .local("renamed", RenamedRecord { value: "x".into() })
        .unwrap();
    let transparent = ui
        .local("transparent", TransparentRecord { value: "x".into() })
        .unwrap();
    assert_eq!(
        omochi_ui::checked_json(renamed.field(RenamedRecord::field_value()).value()).unwrap(),
        json!({"path":"/$local/renamed/value"})
    );
    assert_eq!(
        omochi_ui::checked_json(transparent.field(TransparentRecord::field_value()).value())
            .unwrap(),
        json!({"path":"/$local/transparent"})
    );
}

#[derive(Serialize, Deserialize, ActionInput)]
enum RecursiveEnum {
    Leaf(u32),
    Next(Box<Self>),
}

#[derive(Serialize, ActionInput)]
struct SelfArray {
    values: [u8; Self::LEN],
}
impl SelfArray {
    const LEN: usize = 3;
}

#[derive(Serialize, ActionInput)]
struct ScopedSelfArray {
    values: [u8; {
        struct Size;
        impl Size {
            const ONE: usize = 1;
            const LEN: usize = Self::ONE + 1;
        }
        Size::LEN
    }],
}

#[test]
fn self_types_keep_the_original_type_in_inputs_helpers_and_projections() {
    let value = NodeInput {
        next: Some(Box::new(Node { next: None })).into(),
    }
    .encode()
    .unwrap();
    assert_eq!(value, json!({"next":{"next":null}}));
    let _: Field<Node, Option<Box<Node>>> = Node::field_next();

    let concrete = GenericNode {
        value: 1u32,
        next: None,
    };
    let encoded = GenericNodeInput::<u32> {
        value: 1.into(),
        next: None.into(),
    }
    .encode()
    .unwrap();
    assert_eq!(encoded, serde_json::to_value(&concrete).unwrap());
    let encoded = GenericNodeInput {
        value: 2u32.into(),
        next: Some(Box::new(concrete)).into(),
    }
    .encode()
    .unwrap();
    assert_eq!(encoded, json!({"value":2,"next":{"value":1}}));
    let _: Field<GenericNode<u32>, Option<Box<GenericNode<u32>>>> = GenericNode::field_next();

    let encoded = RecursiveEnumInput::Next(Box::new(RecursiveEnum::Leaf(7)).into())
        .encode()
        .unwrap();
    assert_eq!(encoded, json!({"Next":{"Leaf":7}}));
    let _: Field<RecursiveEnumNextFields, Box<RecursiveEnum>, omochi_ui::SameValue> =
        RecursiveEnumNextFields::field_0();
    assert_eq!(
        SelfArrayInput {
            values: [1, 2, 3].into()
        }
        .encode()
        .unwrap(),
        json!({"values":[1,2,3]})
    );
    assert_eq!(
        ScopedSelfArrayInput {
            values: [1, 2].into()
        }
        .encode()
        .unwrap(),
        json!({"values":[1,2]})
    );
}

#[derive(Deserialize)]
struct DeserializeOnly(u32);

#[derive(Serialize, Deserialize, ActionInput)]
enum SkippedVariants<T> {
    Public {
        value: u32,
    },
    #[serde(skip)]
    Internal(std::thread::JoinHandle<()>),
    #[serde(skip)]
    Generic(T),
    #[serde(skip_serializing)]
    DecodeOnly(DeserializeOnly),
    #[serde(skip_serializing)]
    Record {
        value: DeserializeOnly,
    },
}

#[test]
fn skipped_variants_do_not_require_serializable_payloads() {
    type Payload = std::thread::JoinHandle<()>;
    let encoded = SkippedVariantsInput::<Payload>::Public { value: 7.into() }
        .encode()
        .unwrap();
    assert_eq!(
        encoded,
        serde_json::to_value(SkippedVariants::<Payload>::Public { value: 7 }).unwrap()
    );
    assert!(
        SkippedVariantsInput::Generic(DeserializeOnly(7).into())
            .encode()
            .is_err()
    );
    assert!(
        SkippedVariantsInput::<Payload>::DecodeOnly(DeserializeOnly(7).into())
            .encode()
            .is_err()
    );
    assert!(
        SkippedVariantsInput::<Payload>::Record {
            value: DeserializeOnly(7).into()
        }
        .encode()
        .is_err()
    );
}

#[derive(Serialize, ActionInput)]
#[serde(tag = "kind")]
struct Tagged {
    value: u32,
}

#[derive(Serialize, ActionInput)]
#[serde(tag = "kind", rename = "WireName")]
struct Renamed {
    value: u32,
}

#[derive(Serialize, ActionInput)]
#[serde(
    tag = "kind",
    rename(serialize = "SerializedName", deserialize = "DeserializedName")
)]
struct SplitRenamed {
    value: u32,
}

#[derive(Serialize, ActionInput)]
#[serde(tag = "kind", rename(deserialize = "DeserializedName"))]
struct DeserializeRenamed {
    value: u32,
}

#[derive(Serialize, ActionInput)]
#[serde(tag = "kind")]
struct TaggedFlat {
    #[serde(flatten)]
    fields: std::collections::BTreeMap<String, u32>,
}

#[test]
fn tagged_structs_preserve_effective_serialization_names() {
    for (concrete, generated) in [
        (
            serde_json::to_value(Tagged { value: 7 }).unwrap(),
            TaggedInput { value: 7.into() }.encode().unwrap(),
        ),
        (
            serde_json::to_value(Renamed { value: 7 }).unwrap(),
            RenamedInput { value: 7.into() }.encode().unwrap(),
        ),
        (
            serde_json::to_value(SplitRenamed { value: 7 }).unwrap(),
            SplitRenamedInput { value: 7.into() }.encode().unwrap(),
        ),
        (
            serde_json::to_value(DeserializeRenamed { value: 7 }).unwrap(),
            DeserializeRenamedInput { value: 7.into() }
                .encode()
                .unwrap(),
        ),
    ] {
        assert_eq!(generated, concrete);
    }
    let mut ui = omochi_ui::Ui::new("main", omochi_ui::STANDARD_CATALOG);
    let value = ui.local("value", 7u32).unwrap();
    assert_eq!(
        TaggedInput {
            value: value.value().into()
        }
        .encode()
        .unwrap(),
        json!({"kind":"Tagged","value":{"path":"/$local/value"}})
    );
    let fields = ui
        .local(
            "fields",
            std::collections::BTreeMap::from([("value".to_owned(), 7u32)]),
        )
        .unwrap();
    assert_eq!(
        TaggedFlatInput {
            fields: Input::from(fields.value())
        }
        .encode()
        .unwrap(),
        json!({"call":"omochiMergeObjects","args":{"objects":[{"kind":"TaggedFlat"},{"path":"/$local/fields"}]},"returnType":"object"})
    );
}
