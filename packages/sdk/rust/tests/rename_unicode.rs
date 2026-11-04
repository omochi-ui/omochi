use omochi_ui::{ActionInput, STANDARD_CATALOG, Ui, checked_json};
use serde::Serialize;
use serde_json::json;

macro_rules! unicode_rename_case {
    ($module:ident, $rule:tt, $field:literal, $variant:literal) => {
        mod $module {
            use super::*;

            #[derive(Serialize, ActionInput)]
            #[serde(rename_all = $rule)]
            struct Form {
                café_éclair: String,
            }

            #[derive(Serialize, ActionInput)]
            #[serde(rename_all = $rule, rename_all_fields = $rule)]
            enum Choice {
                CaféÉclair { café_éclair: String },
                UnitéÉclair,
            }

            #[test]
            fn unicode_field_paths_match_serialized_names() {
                let concrete = Form {
                    café_éclair: "text".into(),
                };
                let serialized = serde_json::to_value(&concrete).unwrap();
                assert_eq!(serialized, json!({($field): "text"}));
                assert_eq!(
                    FormInput {
                        café_éclair: "text".into(),
                    }
                    .encode()
                    .unwrap(),
                    serialized
                );

                let mut ui = Ui::new("main", STANDARD_CATALOG);
                let local = ui.local("form", concrete).unwrap();
                let path = format!("/$local/form/{}", $field);
                assert_eq!(
                    checked_json(local.field(Form::field_café_éclair()).value()).unwrap(),
                    json!({"path": path})
                );
                assert_eq!(serialized.pointer(&format!("/{}", $field)), Some(&json!("text")));
            }

            #[test]
            fn unicode_variant_matching_and_payload_paths_match_serialized_names() {
                let concrete = Choice::CaféÉclair {
                    café_éclair: "text".into(),
                };
                let serialized = serde_json::to_value(&concrete).unwrap();
                assert_eq!(serialized, json!({($variant): {($field): "text"}}));
                assert_eq!(
                    ChoiceInput::CaféÉclair {
                        café_éclair: "text".into(),
                    }
                    .encode()
                    .unwrap(),
                    serialized
                );

                let mut ui = Ui::new("main", STANDARD_CATALOG);
                let local = ui.local("choice", concrete).unwrap();
                assert!(local.variant(Choice::variant_unité_éclair()).is_none());
                let payload = local.variant(Choice::variant_café_éclair()).unwrap();
                let path = format!("/$local/choice/{}/{}", $variant, $field);
                assert_eq!(
                    checked_json(payload.field(ChoiceCaféÉclairFields::field_café_éclair()).value())
                        .unwrap(),
                    json!({"path": path})
                );
                assert_eq!(
                    serialized.pointer(&format!("/{}/{}", $variant, $field)),
                    Some(&json!("text"))
                );

                let unit = Choice::UnitéÉclair;
                assert_eq!(
                    ChoiceInput::UnitéÉclair.encode().unwrap(),
                    serde_json::to_value(&unit).unwrap()
                );
                let local = ui.local("unit", unit).unwrap();
                assert!(local.variant(Choice::variant_unité_éclair()).is_some());
                assert!(local.variant(Choice::variant_café_éclair()).is_none());
            }
        }
    };
}

unicode_rename_case!(lowercase, "lowercase", "café_éclair", "caféÉclair");
unicode_rename_case!(uppercase, "UPPERCASE", "CAFé_éCLAIR", "CAFéÉCLAIR");
unicode_rename_case!(pascal, "PascalCase", "Cafééclair", "CaféÉclair");
unicode_rename_case!(camel, "camelCase", "cafééclair", "caféÉclair");
unicode_rename_case!(snake, "snake_case", "café_éclair", "café_Éclair");
unicode_rename_case!(
    screaming_snake,
    "SCREAMING_SNAKE_CASE",
    "CAFé_éCLAIR",
    "CAFé_ÉCLAIR"
);
unicode_rename_case!(kebab, "kebab-case", "café-éclair", "café-Éclair");
unicode_rename_case!(
    screaming_kebab,
    "SCREAMING-KEBAB-CASE",
    "CAFé-éCLAIR",
    "CAFé-ÉCLAIR"
);
