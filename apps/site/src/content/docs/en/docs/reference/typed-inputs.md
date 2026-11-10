---
title: "Typed inputs"
description: "Typed inputs · omochi"
sidebar:
  order: 7
---

Actions receive concrete values. Controls edit renderer-local drafts, and input expressions read those drafts when an action is invoked. See [browser-owned input](../../concepts/inputs/) for binding syntax and [the SDK guide](../../guides/sdks/) for a complete app.

## Schemas and validation

TypeScript actions require a schema; the handler input is inferred from it. Python infers a schema from the handler's first parameter annotation, or accepts an explicit `schema=`. Rust deserializes the handler's concrete input type through Serde.

`schema` provides string, boolean, number, integer, literal, nullable, optional, array, tuple, map, object, and union validators. Python names tuple/map/object constructors `tuple_`, `map_`, and `object_`, and provides `from_type` for annotations. Object schemas reject unknown fields by default; set `unknownKeys` (TypeScript) or `unknown_keys` (Python) to `strip` or `allow` to change this. `custom` supplies a typed conversion boundary. Strings are not automatically converted to numbers; Python booleans are not integers for validation.

A control's range does not validate received input. In Python, for example, `schema.from_type(RunInput).refine(lambda v: 0 <= v.temperature <= 100, "out of range")` adds a server-side constraint.

Client input validation failures reject that action without running the handler; the service continues accepting messages. Direct `invoke()` still rejects. Invalid wire input raises `ProtocolError` in Python/TypeScript; invalid or cyclic declarations raise `DeclarationError`. Handler exceptions and transport failures propagate.

## TypeScript

```ts
const run = app.action("run", schema.object({ temperature: schema.number() }), handler);
// Inside a Surface:
const form = ui.local("form", { temperature: 50 });
ui.slider("Temperature", 0, 100, { bind: form.field("temperature").bind() });
ui.button("Run", run.call({ temperature: form.field("temperature").value() }));
```

Select a field before `.bind()` or `.value()`. `.value()` on the form reads its entire value. The compiler checks required fields, nested structures, and expression result types.

## Python actions

Use `@app.action` on an annotated handler, or `run = app.action(handler, name="run")` to keep the plain handler separately. Unannotated handlers need an explicit schema. The decorator returns an Action handle; `.call()` builds a declaration without executing the handler. `on_click=` accepts registered Action handles.

`app.mount(main)` accepts a synchronous `main(state)` function using `from omochi_ui import ui`. You can also register `(ui, state)` functions with `@app.surface("main")`. `App[State, UI]` and `ActionContext[State]` support static annotations; the package includes `py.typed`.

Pass `input=form` for the complete form, or `input={"temperature": form.temperature}` to compose fields. Separate action declarations use `run.call(form)` or `run.call(run.compose({"temperature": form.temperature}))`. Do not combine an Action call with a separate `input=` on the same button. Omit `input=` only when the action's schema or annotation is `None`. `InputSchema(Concrete, Expression)` and explicit field descriptors are also available; an expression TypedDict is optional.

Dataclass inputs include only `init=True` fields. Computed `field(init=False)` members are omitted from encoded inputs and expression TypedDicts; the constructor or `__post_init__` supplies them when decoding. Sending a computed field is rejected as unknown. A union field accepts expressions for a member or subset: `str | None` accepts `Expr[str]`, `Expr[None]`, and `Expr[str | None]`.

## Python forms and reusable UI

State is an ordinary Python object, shared among an app's connected clients. Each renderer owns independent Local drafts. Use context updates in actions and runtime updates for background work; rendering reads State without mutating it.

A dataclass initial value records its schema. Dictionary and container locals through the `ui` facade require an explicit `type=`:

```python
from typing import TypedDict


class FormInput(TypedDict):
    temperature: float


# Inside a Surface:
form = ui.local("form", {"temperature": 50.0}, type=FormInput)
```

The type preserves omitted optional fields, unions, and empty-container element types. To bind an optional field, include it in the initial value; a missing key does not become `None`.

`form.temperature` and `form["temperature"]` create references checked when connected to a control or action. Use brackets for names that conflict with methods, such as `form["path"]`. `form.field("temperature", float)` returns a checked `Local[float]`. Writable types must match exactly: do not narrow nullable types or widen literals. `.value()` returns a read-only `Ref[T]`; `.bind()` returns an editable binding.

Use functions for reusable components and stable scopes for repeated instances:

```python
def temperature_control(temperature):
    with ui.column():
        ui.slider("Temperature", 0, 100, bind=temperature)


def main(state):
    for key, initial in [("low", 20.0), ("high", 80.0)]:
        with ui.scope(key):
            form = ui.local("form", RunInput(initial))
            temperature_control(form.temperature)
            ui.button("Run", on_click=run, input=form)
```

`scope()` namespaces component and Local keys and preserves drafts when reordered. It adds no layout element and leaves Surface IDs and Action names unchanged. A column changes layout without creating a namespace.

The `ui` facade is available only during synchronous rendering. Use from another task or after rendering is rejected. Refs and cached Action calls cannot cross Surface or render lifetimes. Custom catalogs are available through `ui.catalog(CustomUI)` and explicit catalog builders.

Supported inputs include finite numbers, strings, booleans, null, Literal/Union types, non-recursive dataclasses and TypedDicts, arrays, fixed or homogeneous tuples, and string-keyed maps. Custom classes and recursive types require explicit schemas. A receive-side schema alone does not provide a form encoder for custom values.

## Rust generated inputs and Serde

Derive `Serialize`, `Deserialize`, and `ActionInput` on the concrete type. A type named `Request` generates `RequestInput` with `Input<T>` fields. Use `.into()` for concrete values or expressions, and `Input::fields(...)` for a nested generated input, array, tuple, option, or string-keyed map. `Request::field_name()` produces a typed field projection.

Enum payloads use `local.variant(Request::variant_name())`, returning `None` when the Local's initial value is another variant. Tagged enums match their serialized discriminator and support serialization-only payloads; untagged variants need `DeserializeOwned`. Generated variant field descriptors are named `RequestVariantFields`. Variant-level `#[serde(untagged)]` overrides the container's tag/content path.

A flattened projection returns `Local<T, FieldsOnly>`. Continue through individual fields before calling `.value()` or `.bind()`, for example `form.field(Form::field_nested()).field(Nested::field_count())`. Reading or binding the flattened object as a whole is a compile error because it also contains sibling fields. Compose its individual expressions with `Input::fields(NestedInput { ... })`, or use a separate `Local<Nested>`.

Internally tagged newtype payloads have the same restriction because their object also contains a tag. Newtype and transparent projections keep the restriction when chained from a flattened field; standalone wrappers remain readable.

The expression serializer supports all four enum representations, struct forms, generics, rename/default/skip attributes, and flatten. Duplicate flattened keys and non-finite numbers are rejected. `skip_serializing_if` runs only for concrete values; unresolved fields with that predicate are rejected. Predicates support ordinary argument coercions, and generated helpers infer lifetime arguments.

Custom serialization requires `#[action_input(adapter = Adapter)]` and an `ExpressionAdapter<T>` that declares a serialized `Wire` type, encodes literals/expressions, and defines projection paths. Apply an adapter to a field or the entire type. Bindings use `Wire`: a number serialized as a string cannot bind to a numeric control. Adapters may reject expressions they cannot encode faithfully.

## Object composition

Combine explicit fields with a browser object when its keys are dynamic. The renderer resolves every operand on click and rejects duplicate keys. Object composition requires `MERGE_OBJECTS_CAPABILITY`; enable it only when the renderer supports it. See [Transports](../transports/) for capability configuration.

TypeScript infers the result type:

```ts
import { mergeObjects, MERGE_OBJECTS_CAPABILITY, StdioTransport } from "@omochi-ui/sdk";
// Inside a Surface; run expects { title: string; count: number }.
const counts = ui.local("counts", { count: 1 });
ui.button("Run", run.call(mergeObjects({ title: "x" }, counts.value())));
// At app startup, with a compatible renderer:
await app.start(new StdioTransport([MERGE_OBJECTS_CAPABILITY]));
```

Python uses schemas for each operand and result. `Schema.input()` accepts a value or `Expr[T]`; `merge_objects()` checks field types and requiredness against the result schema. Dynamic maps need a matching map result schema.

```python
from typing import TypedDict
from omochi_ui import merge_objects, schema, StdioTransport, MERGE_OBJECTS_CAPABILITY


class Counts(TypedDict):
    count: int


title = schema.object_({"title": schema.string()})
counts = schema.object_({"count": schema.integer()})
result = schema.object_({"title": schema.string(), "count": schema.integer()})
# Inside a Surface, with a counts Local and an action registered with result:
counts_form = ui.local("counts", {"count": 1}, type=Counts)
input_expr = merge_objects(
    result,
    title.input({"title": "x"}),
    counts.input(counts_form.value()),
)
ui.button("Run", run.call(input_expr))
# At app startup, with a compatible renderer:
await app.start(StdioTransport(capabilities={MERGE_OBJECTS_CAPABILITY}))
```

Rust uses generated input fields:

```rust
#[derive(serde::Serialize, serde::Deserialize, omochi_ui::ActionInput)]
struct RunInput {
    title: String,
    #[serde(flatten)]
    counts: std::collections::BTreeMap<String, i64>,
}
// counts is a Local<BTreeMap<String, i64>> inside a Surface.
let call = run.call(RunInputInput {
    title: "x".into(),
    counts: counts.value().into(),
})?;
// At startup, use StdioTransport::with_capabilities([MERGE_OBJECTS_CAPABILITY]).
```

Concrete flatten values use their Serde representation. Unresolved flatten inputs require the capability. Custom serialization still needs an adapter, and unresolved `skip_serializing_if` fields are rejected because Rust predicates cannot run in the browser.
