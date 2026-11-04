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
    #[arg(long)]
    pub catalog: PathBuf,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub typescript: Option<PathBuf>,
    /// Write Python bindings preserving JSON keys; approximate allOf without intersections.
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
    base: &Path,
    stack: &BTreeSet<String>,
    depth: usize,
) -> Result<Value> {
    if depth > 128 {
        bail!("schema reference nesting exceeds 128");
    }
    if let Some(array) = value.as_array() {
        return array
            .iter()
            .map(|v| resolve(v, document, base, stack, depth + 1))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array);
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
                base.join(filename)
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
        let key = format!("{}:{reference}", base.display());
        let mut stack = stack.clone();
        if !stack.insert(key) {
            bail!("circular schema reference: {reference}");
        }
        if !fragment.is_empty() && !fragment.starts_with('/') {
            bail!("unsupported JSON Schema fragment: #{fragment}");
        }
        let resolved = doc
            .pointer(fragment)
            .with_context(|| format!("unresolved JSON Schema fragment: #{fragment}"))?;
        let mut merged = resolved
            .as_object()
            .context("schema reference must resolve to an object")?
            .clone();
        for (key, value) in object {
            if key != "$ref" {
                merged.insert(key.clone(), value.clone());
            }
        }
        return resolve(
            &Value::Object(merged),
            doc,
            path.as_deref().and_then(Path::parent).unwrap_or(base),
            &stack,
            depth + 1,
        );
    }
    object
        .iter()
        .map(|(k, v)| Ok((k.clone(), resolve(v, document, base, stack, depth + 1)?)))
        .collect::<Result<Map<_, _>>>()
        .map(Value::Object)
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
    use Language::*;
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
        } else {
            values.join(" & ")
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
        let item = ty(&schema["items"], lang)?;
        return Ok(match lang {
            // Array suffixes bind more tightly than unions and intersections,
            // including compound types expanded from symbolic references.
            Ts if item.contains(['|', '&']) => format!("({item})[]"),
            Ts => format!("{item}[]"),
            Py => format!("list[{item}]"),
            Rs => format!("Vec<{item}>"),
        });
    }
    if schema["type"] == "object"
        && let Some(props) = schema["properties"].as_object()
    {
        return Ok(match lang {
            Ts => format!(
                "{{ {} }}",
                props
                    .iter()
                    .map(|(k, v)| {
                        let required = schema["required"]
                            .as_array()
                            .is_some_and(|keys| keys.iter().any(|key| key.as_str() == Some(k)));
                        Ok(format!(
                            "{}{}: {}",
                            quote(k),
                            if required { "" } else { "?" },
                            ty(v, lang)?
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
        let layers = schema["allOf"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![schema.clone()]);
        let mut properties = Map::new();
        let mut required = BTreeSet::new();
        for layer in layers {
            if let Some(reference) = layer["$ref"].as_str() {
                if reference.ends_with("/ComponentCommon") {
                    properties.insert("accessibility".into(), json!({"$ref":"https://a2ui.org/specification/v0_9/common_types.json#/$defs/AccessibilityAttributes"}));
                } else if reference.ends_with("/Checkable") {
                    properties.insert("checks".into(), json!({"type":"array","items":{"$ref":"https://a2ui.org/specification/v0_9/common_types.json#/$defs/CheckRule"}}));
                    properties.insert("isValid".into(), json!({"type":"boolean"}));
                    properties.insert(
                        "validationErrors".into(),
                        json!({"type":"array","items":{"type":"string"}}),
                    );
                } else {
                    bail!("unsupported component mixin: {reference}");
                }
            }
            if let Some(props) = layer["properties"].as_object() {
                for (key, value) in props {
                    if key != "component" && key != "id" {
                        properties.insert(key.clone(), value.clone());
                    }
                }
            }
            if let Some(keys) = layer["required"].as_array() {
                required.extend(keys.iter().filter_map(Value::as_str).map(str::to_owned));
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
fn render(m: &Model, lang: Language) -> Result<String> {
    use Language::*;
    let comment = if lang == Py { "#" } else { "//" };
    let header = format!(
        "{comment} Generated by omochi-ui from catalog {}\n{comment} Source SHA-256: {}\n",
        m.id, m.hash
    );
    let mut bodies = Vec::new();
    let mut enums = Vec::new();
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
                kind = format!(
                    "{}{}",
                    identifier(&c.name),
                    identifier(&upper_first(&p.name))
                );
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
            let overloads = m.components.iter().map(|c| format!("    @overload\n    def component(self, component: Literal[{}], *, key: str, **props: Unpack[{}Props]) -> None: ...", quote(&c.name), c.name)).collect::<Vec<_>>().join("\n");
            format!(
                "{header}from typing import Any, Literal, Required, TypedDict, Unpack, overload\nfrom omochi_ui import Catalog, UI\nfrom omochi_ui.catalog import DataBinding, DynamicBoolean, DynamicNumber, DynamicString, DynamicStringList, FunctionCall\n\n{body}\n\nclass {name}UI(UI):\n{overloads}\n    def component(self, component: str, *, key: str, **props: Any) -> None:\n        super().component(component, key=key, **props)\n\n{constant} = Catalog(id={id}, name={}, ui_type={name}UI)\n",
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
    let resolved = resolve(
        &parsed,
        &parsed,
        file.parent().unwrap(),
        &BTreeSet::new(),
        0,
    )?;
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
    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/catalog")
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
        // Unconstrained layers remain outside the supported schema subset.
        for schema in [json!({"allOf": [{}]}), json!({"allOf": [true]})] {
            assert!(ty(&schema, Language::Py).is_err());
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
