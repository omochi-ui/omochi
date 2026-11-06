---
title: "Catalog generation"
description: "Catalog generation · omochi"
sidebar:
  order: 8
---

Generate typed bindings from a local A2UI v0.9 catalog for any subset of SDKs.
Create output directories before running the command.

```sh
mkdir -p generated
omochi-ui generate catalog \
  --catalog ./catalog.json \
  --name ProductCatalog \
  --typescript ./generated/product-catalog.ts \
  --python ./generated/product_catalog.py \
  --rust ./generated/product_catalog.rs
```

| Option | Meaning |
| --- | --- |
| `--catalog FILE` | Required catalog JSON input |
| `--name NAME` | Type name; defaults to catalog title or `GeneratedCatalog` |
| `--typescript FILE` | TypeScript output |
| `--python FILE` | Python output |
| `--rust FILE` | Rust output |
| `--check` | Check outputs without writing; fail if missing or stale |

At least one output flag is required. Generation resolves local schemas only;
it does not fetch schemas over the network. Reuse the same invocation with
`--check` in CI.

## Names and properties

Component names are used unchanged in generated type names and on the wire. They
must match `[A-Za-z_][A-Za-z0-9_]*`; TypeScript also permits `$` at any position.
Python component names must not start with `__`, because class references would
be name-mangled. Rust rejects `Component`, whose generated `ComponentProps` would
conflict with the SDK trait. Keyword names such as `class` are supported because
generated property types append `Props`. Restrictions apply only to the selected
output languages, and an unsupported name fails before any output is modified.

Python preserves original property names, using functional `TypedDict` declarations
for keywords such as `class` or keys such as `data-label` that cannot be class fields.
Use dictionaries with these keys; required and optional fields retain their meaning.
TypeScript and Python reserve the component property `key` for the builder's node key. If a
component declares `key` in `properties` or `required`, including through `allOf`
or local references, requesting TypeScript or Python output fails before any output is changed.
Rename that property when targeting either language. Rust supports it because its
builder takes the node key separately from props. Keys inside nested object
properties are supported in all three languages.

Rust fields use snake case and explicitly preserve the catalog's original JSON
property names, including acronyms and underscores. Rust keywords use raw
identifiers (for example, `r#type`); `self`, `super` and `crate` receive a trailing
underscore. If a property cannot form a Rust identifier or multiple properties
map to the same Rust field, generation fails before writing any output.

## Enums

Enums support JSON scalar values (strings, booleans, numbers and null), including
inside arrays and unions. Empty enums and object or array enum members are rejected
before any output is written. TypeScript uses literal unions; Python uses
`Literal` with `True`, `False` and `None`, widening floating-point members to `float`.
Rust keeps named enums for string properties, and uses `bool`, `serde_json::Number`
or `()` for homogeneous non-string enums and `Value` for mixed enums. Nested string
enums use `String`. These wider types preserve JSON values but do not restrict
values to the declared enum members. Rust string variant names that collide after
sanitization receive numeric suffixes.

## Schema composition

Local `$ref` siblings follow the [JSON Schema 2020-12 reference rules](https://json-schema.org/draft/2020-12/json-schema-core#section-8.2.3.1):
both the referenced schema and adjacent constraints apply. For example, a reference
to `enum: ["a", "b"]` with a sibling `enum: ["b", "c"]` becomes an intersection,
so TypeScript accepts only `"b"`. Annotation-only siblings such as `description`
do not change the type. Partial `required`, `properties` and `items` constraints
use an object or array type guaranteed by the reference; required fields without
a local property definition use `unknown` in TypeScript and retain their referenced
type through the intersection. Each side resolves relative references from its own
source file. Python and Rust use the `allOf` approximations described below.

Property and array-item schemas also combine adjacent constraints without a `$ref`.
For example, `anyOf` accepting strings or numbers alongside `enum: ["allowed"]`
accepts only `"allowed"` in TypeScript. Object properties and requirements next to
`allOf` remain part of the generated intersection. Python and Rust use the same
approximations as `allOf`; these bindings do not implement full JSON Schema validation.

Component bindings collect properties and required fields from the component itself
and every nested `allOf` layer, including common mixins. Repeated property names
combine their schemas as `allOf`, so TypeScript requires every layer's constraints
regardless of their order. Identical schemas are deduplicated. Python uses the
approximations below; Rust uses `Value` for multiple distinct layers. Required
fields are combined across layers. Fields inside conditional branches are not
added as unconditional component properties.

Component-level `anyOf` and `oneOf` are unsupported, including in nested `allOf`
layers and resolved local references. Generation reports the component and union
keyword before modifying any output. Unions within property schemas, including
array items and nested object properties, remain supported.

Python has no intersection type operator for property-level `allOf`. If all
remaining layers generate the same Python type, that type is used (including
`dict[str, Any]` for object layers); a single layer keeps its type. Other
combinations and empty `allOf` use `Any`. As with the other bindings, layers with
`if` or `then` are skipped; if none remain, Python uses `Any`. These approximations
also apply inside arrays and unions. They preserve JSON values but do not enforce
every schema constraint. An empty schema `{}` adds no constraints and uses
`unknown` in TypeScript, `Any` in Python and `Value` in Rust. Unsupported layers,
such as boolean schemas, still fail generation before any output is written.
