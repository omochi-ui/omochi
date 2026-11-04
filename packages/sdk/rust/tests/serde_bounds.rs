#![allow(dead_code)]
use omochi_ui::{ActionInput, STANDARD_CATALOG, Ui, checked_json};
use serde::Serialize;
use serde_json::json;

trait DomainInput {
    type Value;
}

macro_rules! bound_case {
    ($module:ident, $container:meta, $field:meta) => {
        mod $module {
            use super::*;

            #[derive(Serialize, ActionInput)]
            #[$container]
            struct Request {
                #[$field]
                count: i64,
            }

            // Only the original request belongs to the application's domain.
            impl DomainInput for Request {
                type Value = i64;
            }

            #[test]
            fn bounds_preserve_concrete_serialization_and_expressions() {
                let concrete = Request { count: 7 };
                let serialized = serde_json::to_value(&concrete).unwrap();
                assert_eq!(serialized, json!({"count": 7}));
                assert_eq!(
                    RequestInput { count: 7.into() }.encode().unwrap(),
                    serialized
                );
                let mut ui = Ui::new("main", STANDARD_CATALOG);
                let count = ui.local("count", 9i64).unwrap();
                assert_eq!(
                    RequestInput {
                        count: count.value().into()
                    }
                    .encode()
                    .unwrap(),
                    json!({"count": {"path": "/$local/count"}})
                );
                let request = ui.local("request", concrete).unwrap();
                assert_eq!(
                    checked_json(request.field(Request::field_count()).value()).unwrap(),
                    json!({"path": "/$local/request/count"})
                );
            }
        }
    };
}

bound_case!(
    container_combined,
    serde(bound = "Self: DomainInput"),
    serde(bound = "")
);
bound_case!(
    container_split,
    serde(bound(
        serialize = "Self: DomainInput",
        deserialize = "Self: DomainInput"
    )),
    serde(bound(serialize = "", deserialize = ""))
);
bound_case!(
    field_combined,
    serde(bound = "i64: Serialize"),
    serde(bound = "Self: DomainInput")
);
bound_case!(
    field_split,
    serde(bound(
        serialize = "i64: Serialize",
        deserialize = "i64: serde::Deserialize<'de>"
    )),
    serde(bound(
        serialize = "Self: DomainInput",
        deserialize = "Self: DomainInput"
    ))
);

// Forwarded expr fragments introduce invisible groups around the bound literal.
macro_rules! forwarded_bound {
    ($bound:expr) => {
        bound_case!(
            forwarded,
            serde(bound = $bound),
            serde(bound(serialize = $bound, deserialize = $bound))
        );
    };
}
forwarded_bound!("Self: DomainInput");

#[derive(Serialize, ActionInput)]
#[serde(bound(
    serialize = "Self: DomainInput<Value = T>, <Self as DomainInput>::Value: Serialize,"
))]
struct GenericRequest<T> {
    #[serde(bound = "T: Serialize, <Self as DomainInput>::Value: Serialize")]
    value: T,
}

impl<T> DomainInput for GenericRequest<T> {
    type Value = T;
}

#[test]
fn generic_self_and_associated_types_refer_to_the_original_request() {
    let concrete = GenericRequest { value: 7u32 };
    assert_eq!(
        GenericRequestInput::<u32> { value: 7.into() }
            .encode()
            .unwrap(),
        serde_json::to_value(&concrete).unwrap()
    );
    let mut ui = Ui::new("main", STANDARD_CATALOG);
    let value = ui.local("value", 9u32).unwrap();
    assert_eq!(
        GenericRequestInput::<u32> {
            value: value.value().into()
        }
        .encode()
        .unwrap(),
        json!({"value": {"path": "/$local/value"}})
    );
}

#[derive(Serialize, ActionInput)]
enum BoundedEnum {
    #[serde(bound(serialize = "Self: DomainInput"))]
    Count {
        #[serde(bound = "Self: DomainInput")]
        count: i64,
    },
}

impl DomainInput for BoundedEnum {
    type Value = i64;
}

#[test]
fn variant_and_variant_field_bounds_preserve_serialization_and_expressions() {
    assert_eq!(
        BoundedEnumInput::Count { count: 7.into() }
            .encode()
            .unwrap(),
        serde_json::to_value(BoundedEnum::Count { count: 7 }).unwrap()
    );
    let mut ui = Ui::new("main", STANDARD_CATALOG);
    let count = ui.local("count", 9i64).unwrap();
    assert_eq!(
        BoundedEnumInput::Count {
            count: count.value().into()
        }
        .encode()
        .unwrap(),
        json!({"Count": {"count": {"path": "/$local/count"}}})
    );
}
