//! Native catalog bindings. Remote common-type references are symbolic, never fetched.
use anyhow::{Context, Result, bail};
use clap::Args;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Args)]
pub struct Options {
    /// Local catalog JSON; component names must be supported ASCII identifiers.
    /// Component-level anyOf and oneOf are unsupported.
    #[arg(long)]
    pub catalog: PathBuf,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub typescript: Option<PathBuf>,
    /// Write Python bindings preserving JSON keys; approximate allOf without intersections.
    /// Component property `key` is reserved for the Python builder's node key.
    #[arg(long)]
    pub python: Option<PathBuf>,
    /// Write Rust bindings that preserve the original JSON property names.
    #[arg(long)]
    pub rust: Option<PathBuf>,
    #[arg(long)]
    pub check: bool,
}
#[derive(Clone, Copy, PartialEq)]
enum Language {
    Ts,
    Py,
    Rs,
}
struct Property {
    name: String,
    schema: Value,
    required: bool,
}
struct Component {
    name: String,
    properties: Vec<Property>,
}
struct Model {
    id: String,
    name: String,
    hash: String,
    components: Vec<Component>,
}
fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}
fn validate_component_name(name: &str, lang: Language) -> Result<()> {
    let language = match lang {
        Language::Ts => "TypeScript",
        Language::Py => "Python",
        Language::Rs => "Rust",
    };
    let identifier_start =
        |c: char| c.is_ascii_alphabetic() || c == '_' || (lang == Language::Ts && c == '$');
    if !name.starts_with(identifier_start)
        || !name
            .chars()
            .all(|c| identifier_start(c) || c.is_ascii_digit())
    {
        bail!(
            "component {name:?}: unsupported {language} component name; use an ASCII letter or underscore followed by ASCII letters, digits or underscores{}",
            if lang == Language::Ts {
                " (TypeScript also allows $)"
            } else {
                ""
            }
        );
    }
    // Props is appended to every declaration, so keyword component names are
    // safe. Python still mangles leading __ references inside the UI class.
    if lang == Language::Py && name.starts_with("__") {
        bail!("component {name:?}: Python component names must not start with __");
    }
    // The generated struct would shadow the imported SDK trait.
    if lang == Language::Rs && name == "Component" {
        bail!(
            "component {name:?}: generated Rust type ComponentProps conflicts with the imported SDK trait"
        );
    }
    Ok(())
}
fn python_class_field(name: &str) -> bool {
    // Keep ordinary fields readable, but avoid keywords, name mangling and
    // Unicode identifier normalization changing the original JSON keys.
    name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with("__")
        && !matches!(
            name,
            "False"
                | "None"
                | "True"
                | "and"
                | "as"
                | "assert"
                | "async"
                | "await"
                | "break"
                | "class"
                | "continue"
                | "def"
                | "del"
                | "elif"
                | "else"
                | "except"
                | "finally"
                | "for"
                | "from"
                | "global"
                | "if"
                | "import"
                | "in"
                | "is"
                | "lambda"
                | "nonlocal"
                | "not"
                | "or"
                | "pass"
                | "raise"
                | "return"
                | "try"
                | "while"
                | "with"
                | "yield"
        )
}
fn identifier(s: &str) -> String {
    let re = regex::Regex::new(r"[^A-Za-z0-9]+(.)").unwrap();
    let s = re.replace_all(s, |captures: &regex::Captures<'_>| {
        captures[1].to_uppercase()
    });
    s.trim_start_matches(|c: char| !c.is_ascii_alphabetic() && c != '_')
        .to_owned()
}
fn snake(s: &str) -> String {
    let re = regex::Regex::new(r"([a-z0-9])([A-Z])").unwrap();
    let s = re.replace_all(s, "${1}_${2}");
    regex::Regex::new(r"[^A-Za-z0-9]+")
        .unwrap()
        .replace_all(&s, "_")
        .to_lowercase()
}
fn rust_field(name: &str) -> Result<String> {
    let field = snake(name);
    if field.is_empty() || field == "_" || field.starts_with(|c: char| c.is_ascii_digit()) {
        bail!("property {name:?} cannot be represented as a Rust field");
    }
    Ok(match field.as_str() {
        // These keywords cannot be raw identifiers.
        "self" | "super" | "crate" => format!("{field}_"),
        "as" | "async" | "await" | "break" | "const" | "continue" | "dyn" | "else" | "enum"
        | "extern" | "false" | "fn" | "for" | "if" | "impl" | "in" | "let" | "loop" | "match"
        | "mod" | "move" | "mut" | "pub" | "ref" | "return" | "static" | "struct" | "trait"
        | "true" | "type" | "unsafe" | "use" | "where" | "while" | "abstract" | "become"
        | "box" | "do" | "final" | "gen" | "macro" | "override" | "priv" | "try" | "typeof"
        | "unsized" | "virtual" | "yield" => format!("r#{field}"),
        _ => field,
    })
}
fn upper_first(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}
fn resolve(
    value: &Value,
    document: &Value,
    document_path: &Path,
    stack: &BTreeSet<(PathBuf, String)>,
    depth: usize,
) -> Result<Value> {
    if depth > 128 {
        bail!("schema reference nesting exceeds 128");
    }
    let Some(object) = value.as_object() else {
        return Ok(value.clone());
    };
    if let Some(reference) = value["$ref"]
        .as_str()
        .filter(|r| !r.starts_with("http:") && !r.starts_with("https:"))
    {
        let (filename, fragment) = reference.split_once('#').unwrap_or((reference, ""));
        let path = if filename.is_empty() {
            None
        } else {
            Some(
                document_path
                    .parent()
                    .context("schema document has no parent directory")?
                    .join(filename)
                    .canonicalize()
                    .with_context(|| format!("unresolved schema reference {reference}"))?,
            )
        };
        let external;
        let doc = if let Some(path) = &path {
            external = serde_json::from_slice::<Value>(&std::fs::read(path)?)?;
            &external
        } else {
            document
        };
        let target_path = path.as_deref().unwrap_or(document_path);
        let key = (target_path.to_path_buf(), fragment.to_owned());
        let mut target_stack = stack.clone();
        if !target_stack.insert(key) {
            bail!("circular schema reference: {reference}");
        }
        if !fragment.is_empty() && !fragment.starts_with('/') {
            bail!("unsupported JSON Schema fragment: #{fragment}");
        }
        let resolved = doc
            .pointer(fragment)
            .with_context(|| format!("unresolved JSON Schema fragment: #{fragment}"))?;
        resolved
            .as_object()
            .context("schema reference must resolve to an object")?;
        let target = resolve(resolved, doc, target_path, &target_stack, depth + 1)?;
        // Siblings belong to the referring document, including relative refs.
        let siblings = Value::Object(
            object
                .iter()
                .filter(|(key, _)| *key != "$ref")
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        );
        let siblings = resolve(&siblings, document, document_path, stack, depth + 1)?;
        return Ok(compose_reference(target, siblings));
    }
    object
        .iter()
        .map(|(k, v)| {
            // Schema maps contain arbitrary names, which may themselves be
            // keywords such as `default`, `examples`, or even `$ref`.
            let child = match k.as_str() {
                "components" | "$defs" | "definitions" | "properties" | "patternProperties"
                | "dependentSchemas" | "dependencies"
                    if v.is_object() =>
                {
                    Value::Object(
                        v.as_object()
                            .unwrap()
                            .iter()
                            .map(|(name, schema)| {
                                Ok((
                                    name.clone(),
                                    resolve(schema, document, document_path, stack, depth + 2)?,
                                ))
                            })
                            .collect::<Result<Map<_, _>>>()?,
                    )
                }
                "allOf" | "anyOf" | "oneOf" | "prefixItems" | "items" if v.is_array() => {
                    Value::Array(
                        v.as_array()
                            .unwrap()
                            .iter()
                            .map(|schema| {
                                resolve(schema, document, document_path, stack, depth + 2)
                            })
                            .collect::<Result<Vec<_>>>()?,
                    )
                }
                "items"
                | "additionalItems"
                | "additionalProperties"
                | "unevaluatedItems"
                | "unevaluatedProperties"
                | "contains"
                | "propertyNames"
                | "not"
                | "if"
                | "then"
                | "else"
                | "contentSchema" => resolve(v, document, document_path, stack, depth + 1)?,
                // Annotation and validation data (default/examples/enum/const),
                // and unknown extension values, are not embedded schemas.
                _ => v.clone(),
            };
            Ok((k.clone(), child))
        })
        .collect::<Result<Map<_, _>>>()
        .map(Value::Object)
}
fn annotations_only(schema: &Value) -> bool {
    schema.as_object().is_some_and(|object| {
        object.keys().all(|key| {
            matches!(
                key.as_str(),
                "title"
                    | "description"
                    | "default"
                    | "examples"
                    | "deprecated"
                    | "readOnly"
                    | "writeOnly"
                    | "$comment"
                    | "$defs"
                    | "$schema"
                    | "$id"
            )
        })
    })
}
fn known_type(schema: &Value) -> Option<&str> {
    if schema.get("if").is_some() || schema.get("then").is_some() {
        return None;
    }
    if let Some(kind) = schema["type"].as_str() {
        return Some(kind);
    }
    if let Some(layers) = schema["allOf"].as_array() {
        return layers.iter().find_map(known_type);
    }
    for keyword in ["anyOf", "oneOf"] {
        if let Some(layers) = schema[keyword].as_array() {
            let first = layers.first().and_then(known_type)?;
            return layers
                .iter()
                .all(|layer| known_type(layer) == Some(first))
                .then_some(first);
        }
    }
    None
}
fn normalize_conjunction(mut schema: Value, inherited_type: Option<&str>) -> Value {
    if schema.get("if").is_some() || schema.get("then").is_some() {
        return schema;
    }
    let kind = known_type(&schema).or(inherited_type).map(str::to_owned);
    let compact_enum = enum_matches_type(&schema);
    let Some(object) = schema.as_object_mut() else {
        return schema;
    };
    let mut constraints = Vec::new();
    // Keep adjacent applicators and scalar assertions independent: each
    // keyword constrains the same instance, including without a local $ref.
    for keyword in ["allOf", "anyOf", "oneOf", "enum"] {
        if keyword == "enum" && compact_enum {
            continue;
        }
        if let Some(value) = object.remove(keyword) {
            let value = if keyword != "enum" {
                match value.as_array() {
                    Some(layers) => Value::Array(
                        layers
                            .iter()
                            .cloned()
                            .map(|layer| normalize_conjunction(layer, kind.as_deref()))
                            .collect(),
                    ),
                    None => value,
                }
            } else {
                value
            };
            // Retain the applicator even with no unconditional layers. Its
            // existing language-specific approximation belongs to ty_schema.
            constraints.push(json!({keyword: value}));
        }
    }
    if !annotations_only(&schema) {
        // Partial keywords only receive a type already guaranteed by this
        // conjunction. Required-only objects retain unknown field types.
        if schema.get("type").is_none()
            && let Some(kind) = kind
        {
            schema["type"] = json!(kind);
        }
        constraints.push(schema.clone());
    }
    let mut layers = constraints.into_iter();
    let Some(mut result) = layers.next() else {
        return schema;
    };
    for layer in layers {
        intersect_schemas(&mut result, layer);
    }
    result
}
// An explicit scalar type adds no constraint when every enum value already
// has that type. Keep these common enums compact in all generated languages.
fn enum_matches_type(schema: &Value) -> bool {
    schema["enum"].as_array().is_some_and(|values| {
        !values.is_empty()
            && values.iter().all(|value| match schema["type"].as_str() {
                Some("string") => value.is_string(),
                Some("number") => value.is_number(),
                Some("integer") => value.is_i64() || value.is_u64(),
                Some("boolean") => value.is_boolean(),
                Some("null") => value.is_null(),
                _ => false,
            })
    })
}
// A local $ref and its siblings are conjunctive in JSON Schema 2020-12.
// Use the same normalization as ordinary property and array-item schemas.
fn compose_reference(mut target: Value, mut siblings: Value) -> Value {
    if annotations_only(&siblings) {
        return target;
    }
    if annotations_only(&target) {
        return siblings;
    }
    let kind = known_type(&target)
        .or_else(|| known_type(&siblings))
        .map(str::to_owned);
    target = normalize_conjunction(target, kind.as_deref());
    siblings = normalize_conjunction(siblings, kind.as_deref());
    intersect_schemas(&mut target, siblings);
    target
}
fn validate_enums(schema: &Value) -> Result<()> {
    if let Some(values) = schema.get("enum") {
        let values = values
            .as_array()
            .context("unsupported enum: expected a non-empty array of JSON scalar values")?;
        if values.is_empty() || values.iter().any(|v| v.is_array() || v.is_object()) {
            bail!("unsupported enum: expected a non-empty array of JSON scalar values");
        }
    }
    for key in ["items", "additionalProperties"] {
        if let Some(child) = schema.get(key) {
            validate_enums(child)?;
        }
    }
    if let Some(properties) = schema["properties"].as_object() {
        for child in properties.values() {
            validate_enums(child)?;
        }
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(children) = schema[key].as_array() {
            for child in children {
                validate_enums(child)?;
            }
        }
    }
    Ok(())
}
fn ty(schema: &Value, lang: Language) -> Result<String> {
    ty_schema(&normalize_conjunction(schema.clone(), None), lang)
}
fn ty_schema(schema: &Value, lang: Language) -> Result<String> {
    use Language::*;
    if schema.as_object().is_some_and(Map::is_empty) {
        return Ok(["unknown", "Any", "Value"][lang as usize].into());
    }
    if let Some(reference) = schema["$ref"].as_str() {
        let name = reference.rsplit('/').next().unwrap();
        let mapped = match name {
            "ComponentId" => ["string", "str", "String"],
            "ChildList" => [
                "string[] | { componentId: string; path: string }",
                "list[str] | dict[str, str]",
                "Value",
            ],
            "DataBinding" => ["DataBinding", "DataBinding", "Value"],
            "FunctionCall" => ["FunctionCall", "FunctionCall", "Value"],
            "DynamicValue" => ["DynamicValue", "Any", "Value"],
            "DynamicString" => ["DynamicString", "DynamicString", "Value"],
            "DynamicNumber" => ["DynamicNumber", "DynamicNumber", "Value"],
            "DynamicBoolean" => ["DynamicBoolean", "DynamicBoolean", "Value"],
            "DynamicStringList" => ["DynamicStringList", "DynamicStringList", "Value"],
            "AccessibilityAttributes" => ["Accessibility", "dict[str, DynamicString]", "Value"],
            "CheckRule" => ["CheckRule", "dict[str, Any]", "Value"],
            "Action" => ["WireAction", "dict[str, Any]", "Value"],
            _ => bail!("unsupported schema reference: {reference}"),
        };
        return Ok(mapped[lang as usize].into());
    }
    if let Some(layers) = schema["allOf"].as_array() {
        let values = layers
            .iter()
            .filter(|s| s.get("if").is_none() && s.get("then").is_none())
            .map(|s| ty(s, lang))
            .collect::<Result<Vec<_>>>()?;
        if lang == Py {
            // Python has no intersection operator for types. Reuse a shared
            // approximation (e.g. dict[str, Any] for object layers); otherwise
            // widen to Any without changing or coercing the JSON values.
            return Ok(match values.first() {
                Some(first) if values.iter().all(|value| value == first) => first.clone(),
                _ => "Any".into(),
            });
        }
        return Ok(if values.len() == 1 {
            values[0].clone()
        } else if lang == Rs {
            "Value".into()
        } else if values.is_empty() {
            // Ignored conditional layers leave no static constraint.
            "unknown".into()
        } else {
            // Intersections bind more tightly than unions, including unions
            // expanded from enum schemas and symbolic references.
            values
                .iter()
                .map(|value| {
                    if value.contains('|') {
                        format!("({value})")
                    } else {
                        value.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(" & ")
        });
    }
    if let Some(union) = schema
        .get("anyOf")
        .or_else(|| schema.get("oneOf"))
        .and_then(Value::as_array)
    {
        let values = union
            .iter()
            .map(|s| ty(s, lang))
            .collect::<Result<Vec<_>>>()?;
        return Ok(if lang == Rs {
            "Value".into()
        } else {
            values.join(" | ")
        });
    }
    if let Some(values) = schema["enum"].as_array() {
        return Ok(match lang {
            Ts => values
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join(" | "),
            Py => {
                // Python Literal supports integers, but not floating-point values.
                let literals = values
                    .iter()
                    .filter(|v| !v.is_f64())
                    .map(|v| match v {
                        Value::Bool(true) => "True".into(),
                        Value::Bool(false) => "False".into(),
                        Value::Null => "None".into(),
                        _ => v.to_string(),
                    })
                    .collect::<Vec<_>>();
                let mut kinds = Vec::new();
                if !literals.is_empty() {
                    kinds.push(format!("Literal[{}]", literals.join(", ")));
                }
                if values.iter().any(Value::is_f64) {
                    kinds.push("float".into());
                }
                kinds.join(" | ")
            }
            // Nested enums and non-string property enums use types that preserve
            // JSON values; serde's renamed unit variants only serialize strings.
            Rs if values.iter().all(Value::is_string) => "String".into(),
            Rs if values.iter().all(Value::is_boolean) => "bool".into(),
            Rs if values.iter().all(Value::is_number) => "serde_json::Number".into(),
            Rs if values.iter().all(Value::is_null) => "()".into(),
            Rs => "Value".into(),
        });
    }
    let primitives = match schema["type"].as_str() {
        Some("string") => Some(["string", "str", "String"]),
        Some("number" | "integer") => Some(["number", "float", "f64"]),
        Some("boolean") => Some(["boolean", "bool", "bool"]),
        _ => None,
    };
    if let Some(types) = primitives {
        return Ok(types[lang as usize].into());
    }
    if schema["type"] == "array" {
        let item = match schema.get("items") {
            Some(items) => ty(items, lang)?,
            None => ["unknown", "Any", "Value"][lang as usize].into(),
        };
        return Ok(match lang {
            // Array suffixes bind more tightly than unions and intersections,
            // including compound types expanded from symbolic references.
            Ts if item.contains(['|', '&']) => format!("({item})[]"),
            Ts => format!("{item}[]"),
            Py => format!("list[{item}]"),
            Rs => format!("Vec<{item}>"),
        });
    }
    if schema["type"] == "object" {
        let mut props = schema["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let required: BTreeSet<_> = schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        // A required-only sibling still requires the key, while its referenced
        // schema supplies the property's type through the intersection.
        for key in &required {
            props.entry((*key).to_owned()).or_insert(Value::Null);
        }
        return Ok(match lang {
            Ts if props.is_empty() => "Record<string, unknown>".into(),
            Ts => format!(
                "{{ {} }}",
                props
                    .iter()
                    .map(|(key, value)| {
                        Ok(format!(
                            "{}{}: {}",
                            quote(key),
                            if required.contains(key.as_str()) {
                                ""
                            } else {
                                "?"
                            },
                            if schema["properties"].get(key).is_none() {
                                "unknown".into()
                            } else {
                                ty(value, lang)?
                            }
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?
                    .join("; ")
            ),
            Py => "dict[str, Any]".into(),
            Rs => "Value".into(),
        });
    }
    bail!("unsupported schema fragment: {schema}")
}
fn add_component_property(properties: &mut Map<String, Value>, name: &str, schema: Value) {
    let Some(previous) = properties.get_mut(name) else {
        properties.insert(name.into(), schema);
        return;
    };
    intersect_schemas(previous, schema);
}
fn intersect_schemas(previous: &mut Value, schema: Value) {
    if *previous == schema {
        return;
    }
    // Repeated properties constrain the same value. Keep every schema rather
    // than letting the traversal order select one layer's constraints.
    if previous.as_object().is_some_and(|object| object.len() == 1)
        && let Some(layers) = previous.get_mut("allOf").and_then(Value::as_array_mut)
    {
        if !layers.contains(&schema) {
            layers.push(schema);
        }
    } else {
        *previous = json!({"allOf": [std::mem::take(previous), schema]});
    }
}
fn collect_component_properties(
    schema: &Value,
    properties: &mut Map<String, Value>,
    required: &mut BTreeSet<String>,
) -> Result<()> {
    for keyword in ["anyOf", "oneOf"] {
        if schema.get(keyword).is_some() {
            bail!(
                "unsupported component-level {keyword}; unions are supported only in property schemas"
            );
        }
    }
    if let Some(reference) = schema["$ref"].as_str() {
        if reference.ends_with("/ComponentCommon") {
            add_component_property(
                properties,
                "accessibility",
                json!({"$ref":"https://a2ui.org/specification/v0_9/common_types.json#/$defs/AccessibilityAttributes"}),
            );
        } else if reference.ends_with("/Checkable") {
            add_component_property(
                properties,
                "checks",
                json!({"type":"array","items":{"$ref":"https://a2ui.org/specification/v0_9/common_types.json#/$defs/CheckRule"}}),
            );
            add_component_property(properties, "isValid", json!({"type":"boolean"}));
            add_component_property(
                properties,
                "validationErrors",
                json!({"type":"array","items":{"type":"string"}}),
            );
        } else {
            bail!("unsupported component mixin: {reference}");
        }
    }
    if let Some(props) = schema["properties"].as_object() {
        for (key, value) in props {
            if key != "component" && key != "id" {
                add_component_property(properties, key, value.clone());
            }
        }
    }
    if let Some(keys) = schema["required"].as_array() {
        required.extend(keys.iter().filter_map(Value::as_str).map(str::to_owned));
    }
    if let Some(layers) = schema["allOf"].as_array() {
        for (index, layer) in layers.iter().enumerate() {
            collect_component_properties(layer, properties, required)
                .with_context(|| format!("allOf[{index}]"))?;
        }
    }
    Ok(())
}
fn model(catalog: Value, name: Option<&str>, hash: String) -> Result<Model> {
    let name = identifier(
        name.or_else(|| catalog["title"].as_str())
            .unwrap_or("GeneratedCatalog"),
    );
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        bail!("invalid catalog name");
    }
    let mut components = Vec::new();
    for (component, schema) in catalog["components"]
        .as_object()
        .context("catalog must contain a components object")?
    {
        let mut properties = Map::new();
        let mut required = BTreeSet::new();
        collect_component_properties(schema, &mut properties, &mut required)
            .with_context(|| format!("component {component:?}"))?;
        // Required keys need not have a property schema. Fill them only after
        // every allOf layer has had a chance to supply their value constraints.
        for key in &required {
            if key != "component" && key != "id" {
                properties.entry(key.clone()).or_insert_with(|| json!({}));
            }
        }
        components.push(Component {
            name: component.clone(),
            properties: properties
                .into_iter()
                .map(|(name, schema)| Property {
                    required: required.contains(&name),
                    name,
                    schema,
                })
                .collect(),
        });
    }
    let id = catalog
        .get("catalogId")
        .or_else(|| catalog.get("$id"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .into();
    Ok(Model {
        id,
        name,
        hash,
        components,
    })
}
fn rust_enum_name(component: &Component, property: &Property) -> String {
    let mut suffix = identifier(&upper_first(&property.name));
    suffix.retain(|c| c.is_ascii_alphanumeric() || c == '_');
    if suffix.is_empty() || suffix == "_" {
        suffix = "Value".into();
    }
    // Normalize before reserving preferred names so punctuation and numeric
    // prefixes cannot bypass collision allocation or produce invalid types.
    let name = format!("{}{}", identifier(&component.name), upper_first(&suffix));
    if name == "Self" {
        "SelfValue".into()
    } else {
        name
    }
}
fn render(m: &Model, lang: Language) -> Result<String> {
    use Language::*;
    for component in &m.components {
        validate_component_name(&component.name, lang)?;
        if lang == Py && component.properties.iter().any(|p| p.name == "key") {
            bail!(
                "component {:?}, property \"key\": reserved for the Python builder's node key; rename this property to generate Python bindings",
                component.name
            );
        }
    }
    let comment = if lang == Py { "#" } else { "//" };
    let header = format!(
        "{comment} Generated by omochi-ui from catalog {}\n{comment} Source SHA-256: {}\n",
        m.id, m.hash
    );
    let mut bodies = Vec::new();
    let mut enums = Vec::new();
    let mut rust_types: BTreeSet<String> = [
        "ComponentProps",
        "JsonObject",
        "Serialize",
        "Value",
        "Option",
        "String",
        "Vec",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let mut preferred_enum_names = BTreeSet::new();
    if lang == Rs {
        for c in &m.components {
            rust_types.insert(format!("{}Props", c.name));
            for p in &c.properties {
                if p.schema["enum"]
                    .as_array()
                    .is_some_and(|values| values.iter().all(Value::is_string))
                {
                    preferred_enum_names.insert(rust_enum_name(c, p));
                }
            }
        }
    }
    for c in &m.components {
        let functional_typed_dict =
            lang == Py && c.properties.iter().any(|p| !python_class_field(&p.name));
        let mut props = Vec::new();
        let mut rust_fields = BTreeSet::new();
        for p in &c.properties {
            validate_enums(&p.schema)?;
            let mut kind = ty(&p.schema, lang)?;
            if lang == Rs
                && let Some(values) = p.schema["enum"].as_array()
                && values.iter().all(Value::is_string)
            {
                let base = rust_enum_name(c, p);
                kind = base.clone();
                let mut suffix = 2;
                if !rust_types.insert(kind.clone()) {
                    loop {
                        kind = format!("{base}{suffix}");
                        suffix += 1;
                        // A suffix must not steal another property's ordinary name.
                        if !preferred_enum_names.contains(&kind) && rust_types.insert(kind.clone())
                        {
                            break;
                        }
                    }
                }
                let mut used = BTreeSet::new();
                let variants = values
                    .iter()
                    .map(|v| {
                        let raw = v.as_str().unwrap();
                        let mut base = identifier(&upper_first(raw));
                        base.retain(|c| c.is_ascii_alphanumeric() || c == '_');
                        if base.is_empty() || base == "_" {
                            base = "Value".into();
                        } else if base == "Self" {
                            base = "SelfValue".into();
                        }
                        let mut variant = base.clone();
                        let mut suffix = 2;
                        while !used.insert(variant.clone()) {
                            variant = format!("{base}{suffix}");
                            suffix += 1;
                        }
                        format!("    #[serde(rename = {raw:?})]\n    {variant},")
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                enums.push(format!(
                    "#[derive(Clone, Debug, Serialize)]\npub enum {kind} {{\n{variants}\n}}"
                ));
            }
            props.push(match lang {
                Ts => format!(
                    "  {}{}: {kind}",
                    quote(&p.name),
                    if p.required { "" } else { "?" }
                ),
                Py => format!(
                    "    {}: {}{}",
                    if functional_typed_dict {
                        quote(&p.name)
                    } else {
                        p.name.clone()
                    },
                    if p.required {
                        format!("Required[{kind}]")
                    } else {
                        kind
                    },
                    if functional_typed_dict { "," } else { "" }
                ),
                Rs => {
                    let field =
                        rust_field(&p.name).with_context(|| format!("component {}", c.name))?;
                    if !rust_fields.insert(field.clone()) {
                        bail!(
                            "component {}: property {:?} collides with another Rust field {field}",
                            c.name,
                            p.name
                        );
                    }
                    format!(
                        "    #[serde(rename = {:?})]\n{}    pub {}: {},",
                        p.name,
                        if p.required {
                            ""
                        } else {
                            "    #[serde(skip_serializing_if = \"Option::is_none\")]\n"
                        },
                        field,
                        if p.required {
                            kind
                        } else {
                            format!("Option<{kind}>")
                        }
                    )
                }
            });
        }
        let props = props.join("\n");
        bodies.push(match lang {
            Ts => format!("export interface {}Props {{\n{props}\n}}", c.name),
            Py if functional_typed_dict => format!("{}Props = TypedDict({}, {{\n{props}\n}}, total=False)", c.name, quote(&format!("{}Props", c.name))),
            Py => format!("class {}Props(TypedDict, total=False):\n{}", c.name, if props.is_empty() { "    pass" } else { &props }),
            Rs => format!("#[derive(Clone, Debug, Serialize)]\npub struct {}Props {{\n{props}\n}}\nimpl ComponentProps for {}Props {{\n    const NAME: &'static str = {};\n    fn into_props(self) -> JsonObject {{ serde_json::to_value(self).unwrap().as_object().unwrap().clone() }}\n}}", c.name, c.name, quote(&c.name)),
        });
    }
    let body = bodies.join("\n\n");
    let name = &m.name;
    let constant = name.to_uppercase();
    let id = quote(&m.id);
    Ok(match lang {
        Ts => {
            let mut imports = vec!["CatalogDefinition"];
            imports.extend(
                [
                    "Accessibility",
                    "CheckRule",
                    "DataBinding",
                    "DynamicBoolean",
                    "DynamicNumber",
                    "DynamicString",
                    "DynamicStringList",
                    "DynamicValue",
                    "FunctionCall",
                    "WireAction",
                ]
                .into_iter()
                .filter(|s| body.contains(s)),
            );
            let map = m
                .components
                .iter()
                .map(|c| format!("  {}: {}Props", quote(&c.name), c.name))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "{header}import type {{ {} }} from '@omochi-ui/sdk'\n\n{body}\n\nexport interface {name}ComponentMap {{\n{map}\n}}\n\nexport const {constant}: CatalogDefinition<{name}ComponentMap> = {{ id: {id}, name: {} }}\n",
                imports.join(", "),
                quote(name)
            )
        }
        Py => {
            // Catalog builders deliberately replace the built-in component set.
            // Keep that narrowing explicit without weakening call-site checks.
            let override_comment = "    # This catalog replaces UI.component's built-in signatures.\n    # Only the intentional override mismatch is suppressed; calls stay typed.\n";
            let component = match m.components.as_slice() {
                [] => "    def component(self, /, component: Never, *, key: str, **props: Any) -> None:  # type: ignore[override]  # pyright: ignore[reportIncompatibleMethodOverride]\n        self._append(component, props, key)".to_owned(),
                [c] => format!("    def component(self, /, component: Literal[{}], *, key: str, **props: Unpack[{}Props]) -> None:  # type: ignore[override]  # pyright: ignore[reportIncompatibleMethodOverride]\n        self._append(component, dict(props), key)", quote(&c.name), c.name),
                components => {
                    let overloads = components.iter().enumerate().map(|(i, c)| format!("    @overload{}\n    def component(self, /, component: Literal[{}], *, key: str, **props: Unpack[{}Props]) -> None: ...", if i == 0 { "  # type: ignore[override]" } else { "" }, quote(&c.name), c.name)).collect::<Vec<_>>().join("\n");
                    format!("{overloads}\n    def component(self, /, component: str, *, key: str, **props: Any) -> None:  # pyright: ignore[reportIncompatibleMethodOverride]\n        self._append(component, props, key)")
                }
            };
            format!(
                "{header}from typing import Any, Literal, Never, Required, TypedDict, Unpack, overload\nfrom omochi_ui import Catalog, UI\nfrom omochi_ui.catalog import DataBinding, DynamicBoolean, DynamicNumber, DynamicString, DynamicStringList, FunctionCall\n\n{body}\n\nclass {name}UI(UI):\n{override_comment}{component}\n\n{constant} = Catalog(id={id}, name={}, ui_type={name}UI)\n",
                quote(name)
            )
        }
        Rs => {
            let enums = enums.join("\n\n");
            let separator = if enums.is_empty() { "" } else { "\n\n" };
            format!(
                "{header}use omochi_ui::{{ComponentProps, JsonObject}};\nuse serde::Serialize;\nuse serde_json::Value;\n\npub const CATALOG_ID: &str = {id};\n\n{enums}{separator}{body}\n"
            )
        }
    })
}
pub fn generate(options: Options) -> Result<()> {
    if options.typescript.is_none() && options.python.is_none() && options.rust.is_none() {
        bail!("at least one output flag is required");
    }
    let file = options.catalog.canonicalize()?;
    let raw = std::fs::read(&file)?;
    let parsed: Value = serde_json::from_slice(&raw)?;
    let resolved = resolve(&parsed, &parsed, &file, &BTreeSet::new(), 0)?;
    let model = model(
        resolved,
        options.name.as_deref(),
        hex::encode(Sha256::digest(&raw)),
    )?;
    // Render and check every output before modifying any file.
    let mut outputs = Vec::new();
    for (path, lang) in [
        (options.typescript, Language::Ts),
        (options.python, Language::Py),
        (options.rust, Language::Rs),
    ] {
        if let Some(path) = path {
            outputs.push((path, render(&model, lang)?));
        }
    }
    for (path, content) in &outputs {
        if options.check && std::fs::read_to_string(path).ok().as_ref() != Some(content) {
            bail!("generated catalog is stale: {}", path.display());
        }
    }
    if !options.check {
        for (path, content) in outputs {
            if std::fs::read_to_string(&path).ok().as_ref() != Some(&content) {
                std::fs::write(path, content)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn python_builder_key_is_rejected_before_writing_any_output() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("input.json");
        let outputs = ["ts", "py", "rs"].map(|ext| temp.path().join(format!("catalog.{ext}")));
        for definition in [
            json!({"properties": {"key": {"type": "string"}}, "required": ["key"]}),
            json!({"properties": {"key": {"type": "string"}}}),
            json!({"required": ["key"]}),
        ] {
            std::fs::write(temp.path().join("definition.json"), definition.to_string()).unwrap();
            for schema in [
                definition.clone(),
                json!({"allOf": [{"allOf": [definition.clone()]}]}),
                json!({"$ref": "#/$defs/shortcut"}),
                json!({"allOf": [{"$ref": "definition.json"}]}),
            ] {
                std::fs::write(
                    &input,
                    json!({
                        "$defs": {"shortcut": definition},
                        "components": {"Shortcut": schema}
                    })
                    .to_string(),
                )
                .unwrap();
                for check in [false, true] {
                    for output in &outputs {
                        std::fs::write(output, "existing").unwrap();
                    }
                    let error = generate(Options {
                        catalog: input.clone(),
                        name: None,
                        typescript: Some(outputs[0].clone()),
                        python: Some(outputs[1].clone()),
                        rust: Some(outputs[2].clone()),
                        check,
                    })
                    .unwrap_err()
                    .to_string();
                    assert!(
                        error.contains("component \"Shortcut\", property \"key\""),
                        "{error}"
                    );
                    assert!(
                        error.contains("reserved for the Python builder's node key"),
                        "{error}"
                    );
                    for output in &outputs {
                        assert_eq!(std::fs::read_to_string(output).unwrap(), "existing");
                    }
                }
                generate(Options {
                    catalog: input.clone(),
                    name: None,
                    typescript: Some(outputs[0].clone()),
                    python: None,
                    rust: Some(outputs[2].clone()),
                    check: false,
                })
                .unwrap();
            }
        }
    }

    #[test]
    fn unsupported_component_names_preserve_all_outputs() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("input.json");
        let outputs = ["ts", "py", "rs"].map(|ext| temp.path().join(format!("catalog.{ext}")));
        for name in [
            "custom-card",
            "",
            "1Card",
            "Card name",
            "Card\nName",
            "Card.Name",
            "カード",
            "__Private",
            "Component",
            "$Card",
        ] {
            std::fs::write(
                &input,
                json!({"components": {name: {"properties": {"label": {"type": "string"}}}}})
                    .to_string(),
            )
            .unwrap();
            for output in &outputs {
                std::fs::write(output, "existing").unwrap();
            }
            let error = generate(Options {
                catalog: input.clone(),
                name: None,
                typescript: Some(outputs[0].clone()),
                python: Some(outputs[1].clone()),
                rust: Some(outputs[2].clone()),
                check: false,
            })
            .unwrap_err();
            let message = error.to_string();
            assert!(
                message.contains(&format!("component {name:?}")),
                "{message}"
            );
            for output in &outputs {
                assert_eq!(std::fs::read_to_string(output).unwrap(), "existing");
            }
        }
    }

    #[test]
    fn component_name_restrictions_follow_the_selected_language() {
        for (name, allowed) in [
            ("class", [true, true, true]),
            ("Self", [true, true, true]),
            ("_Badge", [true, true, true]),
            ("Card2", [true, true, true]),
            ("__Private", [true, false, true]),
            ("Component", [true, true, false]),
            ("$Card", [true, false, false]),
            ("custom-card", [false, false, false]),
        ] {
            let model = model(
                json!({"components": {name: {"properties": {"label": {"type": "string"}}}}}),
                None,
                "".into(),
            )
            .unwrap();
            for (lang, allowed) in [Language::Ts, Language::Py, Language::Rs]
                .into_iter()
                .zip(allowed)
            {
                assert_eq!(render(&model, lang).is_ok(), allowed, "{name}");
            }
        }
    }

    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/catalog")
    }
    #[test]
    fn reference_resolution_preserves_literal_json_data() {
        let literal = json!({
            "$ref": "missing.json",
            "properties": {"nested": {"$ref": "#/missing"}},
            "allOf": [{"$ref": "also-missing.json"}]
        });
        let document = json!({
            "$defs": {"text": {"type": "string"}},
            "default": literal,
            "examples": [literal],
            "enum": [literal],
            "const": literal,
            "x-extension": literal,
            "properties": {
                "default": {"$ref": "#/$defs/text", "default": literal},
                "examples": {"$ref": "#/$defs/text", "examples": [literal]},
                "$ref": {"$ref": "#/$defs/text"},
                "value": {"type": "object", "default": literal, "examples": [literal]}
            }
        });
        let resolved = resolve(
            &document,
            &document,
            Path::new("input.json"),
            &BTreeSet::new(),
            0,
        )
        .unwrap();
        for keyword in ["default", "examples", "enum", "const", "x-extension"] {
            assert_eq!(resolved[keyword], document[keyword], "{keyword}");
        }
        for name in ["default", "examples", "$ref"] {
            assert_eq!(resolved["properties"][name]["type"], "string", "{name}");
        }
        assert_eq!(resolved["properties"]["value"]["default"], literal);
        assert_eq!(
            resolved["properties"]["value"]["examples"],
            json!([literal])
        );
    }

    #[test]
    fn reference_resolution_visits_schema_positions() {
        let reference = json!({"$ref": "#/$defs/text"});
        let mut document = json!({"$defs": {"text": {"type": "string"}}});
        let maps = [
            "components",
            "definitions",
            "properties",
            "patternProperties",
            "dependentSchemas",
            "dependencies",
        ];
        let arrays = ["allOf", "anyOf", "oneOf", "prefixItems"];
        let singles = [
            "items",
            "additionalItems",
            "additionalProperties",
            "unevaluatedItems",
            "unevaluatedProperties",
            "contains",
            "propertyNames",
            "not",
            "if",
            "then",
            "else",
            "contentSchema",
        ];
        for keyword in maps {
            document[keyword] = json!({"default": reference});
        }
        for keyword in arrays {
            document[keyword] = json!([reference]);
        }
        for keyword in singles {
            document[keyword] = reference.clone();
        }
        document["dependencies"]["names"] = json!(["default", "examples"]);
        let resolved = resolve(
            &document,
            &document,
            Path::new("input.json"),
            &BTreeSet::new(),
            0,
        )
        .unwrap();
        for keyword in maps {
            assert_eq!(resolved[keyword]["default"]["type"], "string", "{keyword}");
        }
        for keyword in arrays {
            assert_eq!(resolved[keyword][0]["type"], "string", "{keyword}");
        }
        for keyword in singles {
            assert_eq!(resolved[keyword]["type"], "string", "{keyword}");
        }
        assert_eq!(
            resolved["dependencies"]["names"],
            json!(["default", "examples"])
        );
        document["items"] = json!([reference, false]);
        document["additionalProperties"] = json!(false);
        let resolved = resolve(
            &document,
            &document,
            Path::new("input.json"),
            &BTreeSet::new(),
            0,
        )
        .unwrap();
        assert_eq!(resolved["items"], json!([{"type": "string"}, false]));
        assert_eq!(resolved["additionalProperties"], false);
    }

    #[test]
    fn ordinary_conjunction_normalization_is_idempotent_and_keeps_enum_types_compact() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/catalog-adjacent-constraints.json"
        ))
        .unwrap();
        for schema in fixture["components"]["Adjacent"]["properties"]
            .as_object()
            .unwrap()
            .values()
        {
            let normalized = normalize_conjunction(schema.clone(), None);
            assert_eq!(normalize_conjunction(normalized.clone(), None), normalized);
            for lang in [Language::Ts, Language::Py, Language::Rs] {
                assert_eq!(ty(schema, lang).unwrap(), ty(&normalized, lang).unwrap());
            }
        }
        assert_eq!(
            ty(
                &fixture["components"]["Adjacent"]["properties"]["choice"],
                Language::Ts
            )
            .unwrap(),
            r#"(string | number) & "allowed""#
        );
        assert_eq!(
            ty(
                &fixture["components"]["Adjacent"]["properties"]["details"],
                Language::Ts
            )
            .unwrap(),
            r#"{ "count": number } & { "label": string }"#
        );
        for schema in [
            json!({"type": "string", "enum": ["allowed"]}),
            json!({"type": "number", "enum": [1, 2.5]}),
            json!({"type": "integer", "enum": [1, 2]}),
            json!({"type": "boolean", "enum": [true, false]}),
            json!({"type": "null", "enum": [null]}),
        ] {
            assert_eq!(normalize_conjunction(schema.clone(), None), schema);
            for lang in [Language::Ts, Language::Py, Language::Rs] {
                assert_eq!(
                    ty(&schema, lang).unwrap(),
                    ty(&json!({"enum": schema["enum"]}), lang).unwrap()
                );
            }
        }
    }

    #[test]
    fn reference_siblings_keep_constraints_and_their_own_document_scope() {
        let temp = tempfile::tempdir().unwrap();
        let external = temp.path().join("external");
        std::fs::create_dir(&external).unwrap();
        std::fs::write(temp.path().join("sibling.json"), r#"{"type":"boolean"}"#).unwrap();
        std::fs::write(external.join("target.json"), r#"{"type":"number"}"#).unwrap();
        std::fs::write(
            external.join("base.json"),
            json!({
                "$defs": {"choice": {"enum": ["a", "b"]}},
                "type": "object", "properties": {"value": {"$ref": "#/$defs/choice"}, "fromFile": {"$ref": "target.json"}},
                "required": ["value"]
            })
            .to_string(),
        )
        .unwrap();
        let document = json!({
            "$defs": {"choice": {"enum": ["b", "c"]}},
            "$ref": "external/base.json",
            "properties": {"value": {"$ref": "#/$defs/choice"}, "relative": {"$ref": "sibling.json"}},
            "required": ["other"]
        });
        let resolved = resolve(
            &document,
            &document,
            &temp.path().join("input.json"),
            &BTreeSet::new(),
            0,
        )
        .unwrap();
        assert_eq!(
            resolved["allOf"][0]["properties"]["value"]["enum"],
            json!(["a", "b"])
        );
        assert_eq!(
            resolved["allOf"][1]["properties"]["value"]["enum"],
            json!(["b", "c"])
        );
        assert_eq!(
            resolved["allOf"][0]["properties"]["fromFile"]["type"],
            "number"
        );
        assert_eq!(
            resolved["allOf"][1]["properties"]["relative"]["type"],
            "boolean"
        );
        let mut properties = Map::new();
        let mut required = BTreeSet::new();
        collect_component_properties(&resolved, &mut properties, &mut required).unwrap();
        assert_eq!(required, BTreeSet::from(["value".into(), "other".into()]));
        assert_eq!(
            ty(&properties["value"], Language::Ts).unwrap(),
            r#"("a" | "b") & ("b" | "c")"#
        );
    }

    #[test]
    fn reference_cycles_use_canonical_target_documents_and_fragments() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("input.json");
        let external = temp.path().join("external.json");
        std::fs::create_dir(temp.path().join("nested")).unwrap();
        for reference in [
            "#/$defs/value",
            "./input.json#/$defs/value",
            "nested/../input.json#/$defs/value",
            "external.json#/$defs/value",
        ] {
            let document = json!({"$defs": {"value": {"$ref": reference}}});
            std::fs::write(&input, document.to_string()).unwrap();
            std::fs::write(
                &external,
                r##"{"$defs":{"value":{"$ref":"./input.json#/$defs/value"}}}"##,
            )
            .unwrap();
            let error = resolve(
                &document,
                &document,
                &input.canonicalize().unwrap(),
                &BTreeSet::new(),
                0,
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("circular schema reference"),
                "{reference}: {error}"
            );
        }
    }

    #[test]
    fn acyclic_reference_chains_still_enforce_the_depth_limit() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..130 {
            let document = if index == 129 {
                json!({"type": "string"})
            } else {
                json!({"$ref": format!("{}.json", index + 1)})
            };
            std::fs::write(
                temp.path().join(format!("{index}.json")),
                document.to_string(),
            )
            .unwrap();
        }
        let input = temp.path().join("0.json").canonicalize().unwrap();
        let document = serde_json::from_slice(&std::fs::read(&input).unwrap()).unwrap();
        let error = resolve(&document, &document, &input, &BTreeSet::new(), 0).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("schema reference nesting exceeds 128"),
            "{error}"
        );
    }

    #[test]
    fn rejects_component_unions_with_context_before_writing_any_output() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("input.json");
        let ts = temp.path().join("catalog.ts");
        let py = temp.path().join("catalog.py");
        let rs = temp.path().join("catalog.rs");
        for keyword in ["anyOf", "oneOf"] {
            let union = json!({keyword: [
                {"type": "object", "properties": {"label": {"type": "string"}}, "required": ["label"]},
                {"type": "object", "properties": {"count": {"type": "number"}}, "required": ["count"]}
            ]});
            std::fs::write(temp.path().join("union.json"), union.to_string()).unwrap();
            for (schema, nested) in [
                (union.clone(), false),
                (
                    json!({"properties": {"shared": {"type": "boolean"}}, keyword: union[keyword]}),
                    false,
                ),
                (json!({"allOf": [{"allOf": [union]}]}), true),
                (json!({"$ref": "#/$defs/choice"}), false),
                (json!({"allOf": [{"$ref": "union.json"}]}), true),
                (
                    json!({"$ref": "#/$defs/base", keyword: union[keyword]}),
                    true,
                ),
            ] {
                let catalog = json!({
                    "$defs": {"choice": union, "base": {"type": "object"}},
                    "components": {"Choice": schema}
                });
                std::fs::write(&input, catalog.to_string()).unwrap();
                for output in [&ts, &py, &rs] {
                    std::fs::write(output, "existing").unwrap();
                }
                let error = generate(Options {
                    catalog: input.clone(),
                    name: None,
                    typescript: Some(ts.clone()),
                    python: Some(py.clone()),
                    rust: Some(rs.clone()),
                    check: false,
                })
                .unwrap_err();
                let message = format!("{error:#}");
                assert!(message.contains("component \"Choice\""), "{message}");
                assert!(
                    message.contains(&format!("unsupported component-level {keyword}")),
                    "{message}"
                );
                if nested {
                    assert!(message.contains("allOf["), "{message}");
                }
                for output in [&ts, &py, &rs] {
                    assert_eq!(std::fs::read_to_string(output).unwrap(), "existing");
                }
            }
        }
    }

    #[test]
    fn component_properties_still_support_unions_in_all_languages() {
        for keyword in ["anyOf", "oneOf"] {
            let union = json!({keyword: [{"type": "string"}, {"type": "number"}]});
            let catalog = json!({"components": {"Choice": {"allOf": [{"properties": {
                "value": union,
                "items": {"type": "array", "items": union},
                "details": {"type": "object", "properties": {"value": union}}
            }}]}}});
            let model = model(catalog, None, "".into()).unwrap();
            for (lang, expected) in [
                (Language::Ts, "string | number"),
                (Language::Py, "str | float"),
                (Language::Rs, "Value"),
            ] {
                assert_eq!(ty(&union, lang).unwrap(), expected);
                assert!(render(&model, lang).unwrap().contains(expected));
            }
        }
    }

    #[test]
    fn unsupported_reference_siblings_fail_before_writing() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("input.json");
        std::fs::write(
            &input,
            json!({
                "$defs": {"value": {"type": "string"}},
                "components": {"Bad": {"properties": {"value": {
                    "$ref": "#/$defs/value", "enum": [{"unsupported": true}]
                }}}}
            })
            .to_string(),
        )
        .unwrap();
        let ts = temp.path().join("catalog.ts");
        let py = temp.path().join("catalog.py");
        let rs = temp.path().join("catalog.rs");
        for output in [&ts, &py, &rs] {
            std::fs::write(output, "existing").unwrap();
        }
        let error = generate(Options {
            catalog: input,
            name: None,
            typescript: Some(ts.clone()),
            python: Some(py.clone()),
            rust: Some(rs.clone()),
            check: false,
        })
        .unwrap_err();
        assert!(error.to_string().contains("unsupported enum"));
        for output in [ts, py, rs] {
            assert_eq!(std::fs::read_to_string(output).unwrap(), "existing");
        }
    }

    #[test]
    fn reference_annotations_and_partial_constraints_remain_typable() {
        let string = json!({"type": "string"});
        assert_eq!(
            compose_reference(
                string.clone(),
                json!({"description": "label", "title": "Title", "default": "ok", "examples": ["ok"]})
            ),
            string
        );
        for (target, sibling, expected) in [
            (
                json!({"type": "object", "properties": {"value": {"type": "string"}}}),
                json!({"required": ["value"]}),
                r#"{ "value"?: string } & { "value": unknown }"#,
            ),
            (
                json!({"type": "array", "items": {"type": "string"}}),
                json!({"items": {"enum": ["b"]}}),
                r#"string[] & "b"[]"#,
            ),
            (
                json!({"type": "string"}),
                json!({"type": "number"}),
                "string & number",
            ),
            (
                json!({"allOf": [{"if": {}, "type": "string"}, {"type": "object", "properties": {"value": {"type": "string"}}}]}),
                json!({"required": ["value"]}),
                r#"{ "value"?: string } & { "value": unknown }"#,
            ),
        ] {
            assert_eq!(
                ty(&compose_reference(target, sibling), Language::Ts).unwrap(),
                expected
            );
        }
        // Null is not an unconstrained property schema; only absent definitions
        // in required-only constraints receive an unknown type.
        assert!(
            ty(
                &json!({"type": "object", "properties": {"bad": null}}),
                Language::Ts
            )
            .is_err()
        );
    }

    #[test]
    fn overlapping_properties_preserve_every_constraint_and_deduplicate_identical_schemas() {
        let first = json!({"enum": ["a", "b"]});
        let second = json!({"enum": ["b", "c"]});
        for layers in [vec![&first, &second], vec![&second, &first]] {
            let schema = json!({"properties": {"value": layers[0]}, "allOf": [
                {"properties": {"value": layers[1]}, "required": ["value"]},
                {"properties": {"value": layers[0]}}
            ]});
            let m = model(json!({"components": {"Overlap": schema}}), None, "".into()).unwrap();
            let prop = &m.components[0].properties[0];
            assert_eq!(prop.schema, json!({"allOf": layers}));
            assert!(prop.required);
            assert_eq!(ty(&prop.schema, Language::Py).unwrap(), "Any");
            assert_eq!(ty(&prop.schema, Language::Rs).unwrap(), "Value");
        }
        let common = json!({"$ref": "https://a2ui.org/specification/v0_9/common_types.json#/$defs/Checkable"});
        let narrow = json!({"properties": {"isValid": {"enum": [true]}}});
        for layers in [json!([common, narrow]), json!([narrow, common])] {
            let mut properties = Map::new();
            collect_component_properties(
                &json!({"allOf": layers}),
                &mut properties,
                &mut BTreeSet::new(),
            )
            .unwrap();
            let constraints = properties["isValid"]["allOf"].as_array().unwrap();
            assert!(constraints.contains(&json!({"type": "boolean"})));
            assert!(constraints.contains(&json!({"enum": [true]})));
        }
        let mut properties = Map::new();
        for _ in 0..3 {
            add_component_property(&mut properties, "value", first.clone());
        }
        assert_eq!(properties["value"], first);
    }
    #[test]
    fn python_allof_uses_evaluable_approximations_without_changing_other_languages() {
        let objects = json!({"allOf": [
            {"type": "object", "properties": {"label": {"type": "string"}}, "required": ["label"]},
            {"type": "object", "properties": {"enabled": {"type": "boolean"}}, "required": ["enabled"]}
        ]});
        assert_eq!(ty(&objects, Language::Py).unwrap(), "dict[str, Any]");
        assert_eq!(
            ty(&objects, Language::Ts).unwrap(),
            "{ \"label\": string } & { \"enabled\": boolean }"
        );
        assert_eq!(ty(&objects, Language::Rs).unwrap(), "Value");
        for (schema, expected) in [
            (json!({"allOf": [{"type": "string"}]}), "str"),
            (
                json!({"allOf": [{"type": "string"}, {"type": "string"}]}),
                "str",
            ),
            (
                json!({"allOf": [{"type": "string"}, {"enum": ["ready"]}]}),
                "Any",
            ),
            (
                json!({"allOf": [{"type": "string"}, {"type": "boolean"}]}),
                "Any",
            ),
            (json!({"allOf": []}), "Any"),
            (json!({"allOf": [{"if": {}, "then": {}}]}), "Any"),
            (
                json!({"allOf": [{"type": "string"}, {"if": {}, "then": {}}]}),
                "str",
            ),
            (
                json!({"anyOf": [{"allOf": []}, {"type": "string"}]}),
                "Any | str",
            ),
            (
                json!({"type": "array", "items": {"allOf": []}}),
                "list[Any]",
            ),
        ] {
            assert_eq!(ty(&schema, Language::Py).unwrap(), expected, "{schema}");
        }
        assert_eq!(ty(&json!({"allOf": [{}]}), Language::Py).unwrap(), "Any");
        // Boolean schemas and unsupported constraints still fail explicitly.
        for schema in [json!({"allOf": [true]}), json!({"minimum": 1})] {
            assert!(ty(&schema, Language::Py).is_err());
        }
    }
    #[test]
    fn typescript_allof_without_unconditional_layers_uses_unknown() {
        let conditional = json!({"allOf": [{"if": {"type": "string"}, "then": {"minLength": 1}}]});
        for schema in [json!({"allOf": []}), conditional.clone()] {
            assert_eq!(ty(&schema, Language::Ts).unwrap(), "unknown");
            assert_eq!(ty(&schema, Language::Py).unwrap(), "Any");
            assert_eq!(ty(&schema, Language::Rs).unwrap(), "Value");
        }
        for (schema, expected) in [
            (json!({"type": "array", "items": conditional}), "unknown[]"),
            (
                json!({"anyOf": [conditional, {"type": "string"}]}),
                "unknown | string",
            ),
            (
                json!({"allOf": [conditional, {"type": "string"}]}),
                "unknown & string",
            ),
        ] {
            assert_eq!(ty(&schema, Language::Ts).unwrap(), expected);
        }
    }
    #[test]
    fn matches_legacy_outputs_and_check_never_overwrites_stale_files() {
        let temp = tempfile::tempdir().unwrap();
        let options = |check| Options {
            catalog: fixture().join("catalog.json"),
            name: None,
            typescript: Some(temp.path().join("catalog.ts")),
            python: Some(temp.path().join("catalog.py")),
            rust: Some(temp.path().join("catalog.rs")),
            check,
        };
        generate(options(false)).unwrap();
        for extension in ["ts", "py", "rs"] {
            assert_eq!(
                std::fs::read_to_string(temp.path().join(format!("catalog.{extension}"))).unwrap(),
                std::fs::read_to_string(fixture().join(format!("expected.{extension}"))).unwrap()
            );
        }
        generate(options(true)).unwrap();
        std::fs::write(temp.path().join("catalog.py"), "user edit").unwrap();
        assert!(
            generate(options(true))
                .unwrap_err()
                .to_string()
                .contains("stale")
        );
        assert_eq!(
            std::fs::read_to_string(temp.path().join("catalog.py")).unwrap(),
            "user edit"
        );
    }
    #[test]
    fn rejects_cycles_missing_fragments_and_unsupported_types_without_partial_writes() {
        let temp = tempfile::tempdir().unwrap();
        for source in [
            json!({"components":{"Bad":{"$ref":"#/components/Bad"}}}),
            json!({"components":{"Bad":{"$ref":"#/missing"}}}),
            json!({"components":{"Bad":{"properties":{"x":{"type":"null"}}}}}),
        ] {
            let path = temp.path().join("input.json");
            std::fs::write(&path, source.to_string()).unwrap();
            let output = temp.path().join("output.ts");
            std::fs::write(&output, "existing").unwrap();
            assert!(
                generate(Options {
                    catalog: path,
                    name: None,
                    typescript: Some(output.clone()),
                    python: None,
                    rust: None,
                    check: false
                })
                .is_err()
            );
            assert_eq!(std::fs::read_to_string(output).unwrap(), "existing");
        }
    }

    #[test]
    fn rejects_non_scalar_and_empty_enums_before_writing_any_language() {
        let temp = tempfile::tempdir().unwrap();
        for invalid in [json!([]), json!([{}]), json!([[1]]), json!(true)] {
            for schema in [
                json!({"enum": invalid}),
                json!({"type": "array", "items": {"enum": invalid}}),
                json!({"anyOf": [{"enum": invalid}, {"type": "string"}]}),
                json!({"type": "object", "properties": {"nested": {"enum": invalid}}}),
            ] {
                let input = temp.path().join("input.json");
                std::fs::write(
                    &input,
                    json!({"components": {"Bad": {"properties": {"value": schema}}}}).to_string(),
                )
                .unwrap();
                for lang in [Language::Ts, Language::Py, Language::Rs] {
                    let output = temp.path().join("output");
                    std::fs::write(&output, "existing").unwrap();
                    let error = generate(Options {
                        catalog: input.clone(),
                        name: None,
                        check: false,
                        typescript: (lang == Language::Ts).then(|| output.clone()),
                        python: (lang == Language::Py).then(|| output.clone()),
                        rust: (lang == Language::Rs).then(|| output.clone()),
                    })
                    .unwrap_err();
                    assert!(error.to_string().contains("unsupported enum"), "{error}");
                    assert_eq!(std::fs::read_to_string(output).unwrap(), "existing");
                }
            }
        }
    }

    #[test]
    fn rejects_unrepresentable_rust_enum_property_names() {
        for name in ["123", "日本語"] {
            let catalog = model(
                json!({"components": {"Panel": {"properties": {
                    name: {"enum": ["a", "b"]}
                }}}}),
                None,
                String::new(),
            )
            .unwrap();
            let error = render(&catalog, Language::Rs).unwrap_err();
            assert!(format!("{error:#}").contains("cannot be represented"));
        }
    }

    #[test]
    fn rejects_invalid_or_colliding_rust_fields_before_writing_any_language() {
        let temp = tempfile::tempdir().unwrap();
        for (names, message) in [
            (vec!["dataKey", "data_key"], "collides"),
            (vec!["self", "self_"], "collides"),
            (vec!["123key"], "cannot be represented"),
            (vec![""], "cannot be represented"),
            (vec!["_"], "cannot be represented"),
        ] {
            let properties: Map<_, _> = names
                .iter()
                .map(|name| ((*name).to_owned(), json!({"type": "string"})))
                .collect();
            let input = temp.path().join("input.json");
            std::fs::write(
                &input,
                json!({"components": {"Bad": {"properties": properties}}}).to_string(),
            )
            .unwrap();
            let ts = temp.path().join("catalog.ts");
            let rs = temp.path().join("catalog.rs");
            for output in [&ts, &rs] {
                std::fs::write(output, "existing").unwrap();
            }
            let error = generate(Options {
                catalog: input,
                name: None,
                typescript: Some(ts.clone()),
                python: None,
                rust: Some(rs.clone()),
                check: false,
            })
            .unwrap_err();
            assert!(format!("{error:#}").contains(message), "{error:#}");
            for output in [ts, rs] {
                assert_eq!(std::fs::read_to_string(output).unwrap(), "existing");
            }
        }
    }
}
