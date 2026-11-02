# SDK guide

[Overview](../README.md) · [Architecture](architecture.md) · [CLI reference](../docs/cli.md)

Examples below use the local packages in this repository. Build the workspace
before running TypeScript examples. The APIs produce A2UI v0.9 messages;
`app.start()` alone uses stdin/stdout and does not open a browser.

## Python

```python
import asyncio
from dataclasses import dataclass

from omochi_ui import ActionContext, App, ui


@dataclass
class State:
    temperature: float = 50
    result: str | None = None

@dataclass
class RunInput:
    temperature: float

app = App(State())


@app.action
async def run(input: RunInput, context: ActionContext[State]):
    async with context.update() as state:
        state.temperature = input.temperature
        state.result = f"Ran at temperature {input.temperature:g}"


def main(state: State) -> None:
    form = ui.local("form", RunInput(state.temperature))
    ui.text("Hello", key="title", variant="h2")
    ui.slider("temperature", 0, 100, bind=form.temperature)
    ui.button("Run", on_click=run, input=form, variant="primary")
    if state.result:
        ui.text(state.result, key="result")


app.mount(main)


asyncio.run(app.start())
```

Run the bundled example from the repository root:

```sh
uv run --project examples/python examples/python/app.py
```

See the [quick start](../README.md#quick-start) to connect it to a browser.

## TypeScript

```ts
import { schema, App } from "@omochi-ui/sdk";

const state: { temperature: number; result: string | null } = {
  temperature: 50,
  result: null,
};

const app = new App(state);

const run = app.action("run", schema.object({ temperature: schema.number() }), async (input, context) => {
  await context.update((current) => { current.temperature = input.temperature; current.result = `Ran at temperature ${input.temperature}`; });
});

app.surface("main", (ui, current) => {
  const form = ui.local("form", { temperature: current.temperature });
  ui.text("Hello", { key: "title", variant: "h2" });
  ui.slider("temperature", 0, 100, { bind: form.field("temperature").bind() });
  ui.button("Run", run.call({ temperature: form.field("temperature").value() }), { variant: "primary" });
  if (current.result) ui.text(current.result, { key: "result" });
});

await app.start();
```

In Node.js, the application can also host WebSocket renderers directly without
the stdio bridge:

```ts
import { WebSocketServerTransport } from "@omochi-ui/sdk/node";

const transport = new WebSocketServerTransport({ port: 5555 });
await app.start(transport);
```

The server listens on `127.0.0.1` at `/ws` by default. It broadcasts one shared
application state to every connected client and sends a compact snapshot to
clients that join later. Use `host` and `path` to override the listener, `port: 0`
to select an available port, `await transport.ready` before reading
`transport.url`, and `await transport.close()` for graceful shutdown. The direct
server has no authentication or TLS; keep the loopback default unless external
exposure is explicitly intended.

## Rust

The crate is named `omochi-ui` and imported as `omochi_ui`. Typed application
state is mutated only through Runtime transactions.

```rust
use omochi_ui::{ActionInput, Button, ButtonVariant, Input, Slider, Text, TextVariant};
use omochi_ui::{App, Result};
use serde::{Deserialize, Serialize};

struct State {
    temperature: f64,
    result: Option<String>,
}
#[derive(Deserialize, Serialize, ActionInput)]
struct RunInput {
    temperature: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            temperature: 50.0,
            result: None,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = App::new(State::default());

    let run = app.action::<RunInput, _, _>("run", |input, context| async move {
        context
            .update(|state| {
                state.temperature = input.temperature;
                state.result = Some(format!("Ran at temperature {}", input.temperature));
            })
            .await?;
        Ok(())
    })?;

    app.surface("main", move |ui, state| {
        let form = ui.local(
            "form",
            RunInput {
                temperature: state.temperature,
            },
        )?;
        ui.add(Text::new("Hello").key("title").variant(TextVariant::H2))?;
        ui.add(
            Slider::new("temperature", 0.0, 100.0)
                .bind(form.field(RunInput::field_temperature()).bind()?),
        )?;
        ui.add(
            Button::new("Run")
                .on_press(run.call(RunInputInput {
                    temperature: Input::expression(
                        form.field(RunInput::field_temperature()).value(),
                    ),
                })?)
                .variant(ButtonVariant::Primary),
        )?;
        if let Some(result) = &state.result {
            ui.add(Text::new(result.clone()).key("result"))?;
        }
        Ok(())
    })?;

    app.start().await
}
```

## Components and catalogs

The Basic Catalog exposes typed high-level APIs for all 18 A2UI v0.9 components,
including media, layouts, cards, tabs, modals, controls, and date/time input. Use
`component()` when you need the wire-level properties directly; its component name
selects the corresponding generated property type.

Generate typed bindings for a local custom catalog before compiling your application:

```sh
bun run omochi-ui generate catalog \
  --catalog ./catalog.json \
  --name ProductCatalog \
  --typescript ./src/generated/product-catalog.ts \
  --python ./src/generated/product_catalog.py \
  --rust ./src/generated/product_catalog.rs
```

Pass any subset of the three output flags. Add `--check` in CI to fail when an
existing generated file is stale. Generation is local-only and never fetches schemas
over the network.

The bundled preview and hub renderer use the Basic Catalog. Generated bindings
for a custom catalog also require a compatible renderer implementation; generating
bindings alone does not add those components to the bundled browser UI.

## State and actions

- `App` defines typed actions and surfaces; `App.build()` creates the transport-independent `Runtime`.
- `ui.local()` declares renderer-owned draft state. Its initial value is applied once and is preserved across rerenders; use `resetKey` / `reset_key` / `local_with` for an explicit reset.
- Actions receive only concrete typed input and `context.update()`. They cannot inspect a Surface, renderer model, binding, or transport.
- Each awaited `context.update()` mutates state, renders every Surface, and sends
  the resulting differences before returning. Completion means the transport's
  `send()` calls have completed, not that a browser has finished painting.
- `runtime.update()` remains ordered behind an active action. Actions are still
  serial; publishing progress does not allow another action to run concurrently.
- `runtime.handle()` accepts `getSurface` observations and `invoke` commands as JSON.
- Component keys become A2UI component IDs. The root ID is always `root` as
  required by A2UI. Use explicit keys for conditional or reorderable content.

### Progress during an action

In all three SDKs, await each context update before continuing the operation:

```python
@app.action("work", schema=schema.string())
async def work(input: str, context: ActionContext[State]) -> None:
    async with context.update() as state:
        state.status = "Working"
    await do_work(input)
    async with context.update() as state:
        state.status = "Done"
```

Multiple updates in an action expose intermediate states. Group
changes in one mutator when they should appear together. The action's `invoke()`
or `handle()` result contains all generated messages in order, including those
already sent; they are not sent a second time at action completion. Without an
attached transport, these methods still return the same generated message list.

Update, rendering, and transport errors propagate through `context.update()`.
An action error does not roll back state changes or messages already sent. Await
updates within the action's lifetime; use `runtime.update()` for independent
background work. Calling and awaiting `runtime.update()` from an active action
would wait on that same action.

## Renderer-local inputs

Bind editable components to a `Local` returned by `ui.local()`. `Local.value()`
builds an Action input expression; the renderer resolves it when the user invokes
the action, so the Runtime receives concrete JSON and never stores the draft.

## Transports

The Python and TypeScript SDKs include stdio, WebSocket-like, and callback
transports. Rust exposes `StdioTransport`, a `Transport` trait, and
`CallbackTransport` for framework adapters.

TypeScript additionally exports the Node-only multi-client
`WebSocketServerTransport` from `@omochi-ui/sdk/node`.

All transports use the same JSON messages; adapters can carry them over
WebSocket, A2A, AG-UI, SSE, or another channel without changing app semantics.
`Transport` represents one duplex connection and does not own history or session
routing. TypeScript and Python use `await app.start(transport?)`; omitting the
transport selects stdio. Rust uses `app.start()` for stdio and
`app.start_with(transport)` for an explicitly supplied adapter because Rust has
no default function arguments.

Transport capability sets are passed to the reconciler before the first surface
is rendered.

TypeScript, Python, and Rust each export a `UnixSocketServerTransport` that hosts
one application on a discoverable unix domain socket; see [Discovery and dashboards](discovery.md).

The A2UI wire format has no omochi-ui session envelope. A `Transport` still hosts
exactly one application, and session membership stays outside the messages: the
hub keeps one connection per service and namespaces surface ids instead of
wrapping the frames.

## Registered media

Register an immutable copy of a local file or bytes and pass its URI to Image,
Video, or AudioPlayer. This works through stdio dev preview and local/remote hubs;
the browser never needs access to the application's filesystem or localhost.

```python
photo = app.resources.file("photo.png")
clip = app.resources.bytes(video_bytes, mime_type="video/mp4")
# In a surface:
ui.image(photo)
ui.video(clip)
```

TypeScript uses `app.resources.file(path, mimeType?)` and
`app.resources.bytes(data, mimeType)`. Rust uses
`app.resources.file(path, Option<&str>)?` and
`app.resources.bytes(&data, mime)?`. All return `omochi://resources/<id>` strings.
Registration performs bounded synchronous disk I/O: register large files before
starting the app, or use a worker/thread when registering during interaction.

`release(uri)` prevents further reads. Keep media registered while a surface can
reference it, including while audio/video is playing. Runtime shutdown closes its
registry. Changed content must be registered again and its new URI rendered.

Storage limits default to 1 GiB per file and 4 GiB total. To change them, supply a
`ResourceRegistry` to the TypeScript App's third constructor argument, Python's
`resources=` argument, or Rust's public `app.resources` field. The registry accepts
per-file and total byte limits. Files infer common image/audio/video MIME types by
extension; byte registration requires a MIME type. There is no codec conversion.

Runnable examples: `examples/media/app.py`, `examples/media/app.ts`, and
`cargo run --manifest-path packages/sdk/rust/Cargo.toml --example media -- photo.png audio.wav clip.webm`.
Pass the same three filenames to the Python/TypeScript examples, and use `omochi-ui dev --`
to wrap the command. The Python SDK must be installed or on PYTHONPATH.

Custom transports must preserve resource frames separately from A2UI snapshots and
reply to the originating connection. The bundled transports provide this handling. The browser renderer's
`useA2uiSession(wsUrl, resourceBaseUrl?)` accepts an HTTP base override for custom
hosting; by default the WebSocket origin supplies the HTTP resource endpoint.

See [the resource wire contract](../spec/resources.md). Hub and gateway must both
use gateway protocol 3; existing SDKs without resource support can still display UI.

## Typed inputs and migration

This API change targets the next minor SDK release. A2UI v0.9 and existing
component/local IDs are unchanged. Existing applications must migrate action
registration and form references before updating.

TypeScript actions now require a schema: `app.action("run", schema.object({
temperature: schema.number() }), handler)`. The handler input is inferred from
that schema. Replace `form.bind("/temperature")` with
`form.field("temperature").bind()` and use `.value()` for an action expression.
The compiler checks required fields, nested structures and expression result types.

Python actions infer their input schema from the handler's first parameter:
`@app.action` above `async def run(value: RunInput, context): ...`.
Use `app.mount(main)` with a plain `main(state)` function and
`from omochi_ui import ui` for render-scoped UI declarations.
`ui.button("Run", on_click=run, input=form)` connects a registered action to the
form's current browser value. Partial inputs can be written directly as
`input={"temperature": form.temperature}`. The SDK validates the declaration and
the renderer resolves the references at click time.

An explicit `schema=` remains available for custom validation. For example,
`schema.from_type(RunInput).refine(lambda v: 0 <= v.temperature <= 100, "out of range")`
adds a server-side constraint; a Slider's range alone does not validate received input.
Unannotated handlers need an explicit schema. To retain a plain function for tests
or registration with another App, use `run = app.action(handler, name="run")`.
The decorator returns an Action handle; `.call()` builds a declaration and never
executes the handler. Raw callbacks and lambdas are not accepted as `on_click=`.

Existing `@app.surface("main")` functions receiving `(ui, state)`, `schema=`
action registration, `InputSchema(Concrete, Expression)` and field descriptors
remain supported. An expression TypedDict is optional; new applications do not
need one. `App[State, UI]` and `ActionContext[State]` annotations are available for
additional static checking. Python wheels include `py.typed`; both mypy and
Pyright consumers are checked, including the built wheel.

Dataclass inputs include only fields with `init=True`. Computed fields declared
with `field(init=False)` are omitted when encoding and from the expression
TypedDict; the constructor or `__post_init__` supplies them when decoding.
Sending a computed field is rejected as an unknown input field. An expression
may return a member or subset of a union: a `str | None` input field accepts
`Expr[str]`, `Expr[None]`, and `Expr[str | None]`.

`schema` provides string, boolean, number, integer, literal, nullable, optional,
array, tuple, map, object and union validators. Python names tuple/map/object
constructors `tuple_`, `map_`, `object_`. Python also provides `from_type` for
explicit annotations. Object schemas reject unknown fields by default; set
`unknownKeys` (TS) or `unknown_keys` (Python) to `strip` or `allow` explicitly.
`custom` supplies a typed conversion boundary for domain-specific validation.
No string-to-number conversion occurs. Boolean is not an integer in Python's
validator. Invalid wire input raises `ProtocolError` before invoking the handler;
invalid or cyclic declaration values raise `DeclarationError`.

Python update blocks and callback updates share the same lock. Normal exit
increments the revision once and waits for transport sends. An exception or
cancellation in the block retains mutations but does not increment the revision
or render. Send failures retain already applied changes and sent messages.
Same-task nesting raises `DeclarationError`; separate tasks serialize. An update
after the action completes is rejected. Long-running work should be placed between
update blocks so intermediate progress can be sent.

Rust uses `#[derive(Serialize, Deserialize, ActionInput)]`. A type named `Request`
generates `RequestInput`; its fields contain `Input<T>`. Use `.into()` for concrete
values or expressions, and `Input::fields(...)` for a nested generated input,
array, tuple, option or string-keyed map. `Request::field_name()` produces a typed
projection. Enum payloads require `local.variant(Request::variant_name())`;
projection returns `None` when the current initial value is another variant.
Tagged enums are matched by their serialized discriminator without deserializing
the payload, so projection also supports serialization-only types. Untagged
variants still require `DeserializeOwned` for Serde's shape-based matching.
Generated variant field descriptors are named `RequestVariantFields`.
Variant-level `#[serde(untagged)]` overrides the container's tag/content path.

A flattened field projection returns `Local<T, FieldsOnly>`. Continue through
its fields, such as `form.field(Form::field_nested()).field(Nested::field_count())`,
before calling `.value()` or `.bind()`. Reading or binding the flattened object
as a whole is a compile error because its JSON object also holds sibling fields.
To build an action input, use `Input::fields(NestedInput { ... })` with those
individual field expressions, or reference a separate `Local<Nested>`.
The same restriction applies to internally tagged newtype payloads, whose object
includes the tag. Newtype and transparent projections preserve the restriction
when chained from a flattened field; ordinary standalone wrappers remain readable.

Serde handles the concrete input at invocation. All four enum representations,
struct forms, generics, rename/default/skip attributes and flatten are supported
by the generated expression serializer. Flattened duplicate keys and non-finite
numbers are rejected. A skip predicate is evaluated only for a concrete value;
unresolved values at a `skip_serializing_if` field are rejected during encoding.
Predicates support ordinary Rust argument coercions, and generated helpers infer
lifetime arguments. Custom Serde serialization requires
`#[action_input(adapter = Adapter)]` and an `ExpressionAdapter<T>` implementation
that declares the serialized `Wire` type, encodes literals/expressions and defines field projection paths. Bindings use `Wire` so a number serialized as a string cannot accidentally be attached to a numeric control.
Apply the adapter to a field or the entire input type. An adapter may reject
expressions it cannot faithfully encode.

Rust Basic Catalog builders use `ui.add(...)`. For example,
`Button::new("Run").variant(ButtonVariant::Primary).on_press(run.call(input)?)`.
Input builders expose mutually exclusive `.default(...)` and `.bind(...)` methods.
`CatalogComponent::new(key, props)` adds generated custom-catalog properties
through the same interface. Layout closures and `App::build(self)` keep their
existing ownership semantics.

Run `python scripts/test-sdk-types.py` with mypy and Pyright installed for Python
positive/negative API checks. TypeScript checks run with the SDK test compiler;
Rust compile-fail examples run with `cargo test --doc`.

### Python State, forms, and reusable UI

State is an ordinary dataclass, dictionary, or other server-side object. There is
no required State base class and no automatic serialization or copying of State.
One Runtime's State is shared by its connected clients. Each renderer independently
owns its `ui.local()` drafts. Use `context.update()` inside actions or
`runtime.update()` for background work; rendering reads State and does not mutate it.

For a dataclass, `ui.local("form", RunInput())` records its declared schema.
Dictionary and container locals through the module facade require an explicit
`type=`, for example:

```python
from typing import TypedDict


class FormInput(TypedDict):
    temperature: float

form = ui.local("form", {"temperature": 50.0}, type=FormInput)
```

TypedDict instances are plain dictionaries at runtime. Their declaration cannot
be recovered from the initial value. The supplied type also preserves omitted
optional fields, Union types, and the element types of empty containers.
Binding to an absent optional field requires supplying that field in the initial
value; a missing key is not converted to `None`.

`form.temperature` and `form["temperature"]` return dynamic references whose
result types are checked when connected to a control or action input. Use brackets
for names that conflict with reference methods, such as `form["path"]`.
For additional static checking, `form.field("temperature", float)` returns a
`Local[float]` after checking the declared field type. Writable types must match
exactly: nullable fields cannot be narrowed to non-nullable fields, nor Literal
fields widened to strings. `.value()` returns a read-only `Ref[T]` and `.bind()`
remains available for legacy code.

Components are ordinary functions. Supply a stable namespace when reusing them:

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

`scope()` adds no layout element. It namespaces component and Local keys, leaving
Surface IDs and Action names unchanged. Reordering scopes preserves each draft.
Scoped keys use `$scope:` followed by a JSON array of scope segments and the key;
this prefix is reserved for SDK-generated IDs. Unscoped IDs retain their existing
form. `with ui.column()` only changes layout; it does not create a key namespace.

The `ui` facade is available only during synchronous rendering. It restores its
context on failure and rejects use from another task or after rendering has ended.
Refs and cached Action calls cannot be reused across Surface or render lifetimes.
Custom Catalog methods remain available through `ui.catalog(CustomUI)`; existing
Catalog-specific explicit Builders continue to work.

For separate construction of action declarations, use `run.call(form)` or
`run.call(run.compose({"temperature": form.temperature}))`. These use the same
validation and wire representation as `on_click=` with `input=`. Do not combine
these two forms on the same button. `input=` can be omitted only for an action
whose input annotation/schema is `None`.

Initial supported input types are finite numeric values, strings, booleans, null,
Literal/Union types, non-recursive dataclasses and TypedDicts, arrays, fixed or
homogeneous tuples, and string-keyed maps. Arbitrary custom classes and recursive
types require explicit schemas; a receive-side schema alone does not provide a
form encoder for custom values.

Run `bun run test:python-renderer` for a Python stdio app round trip through the
actual renderer model, including independent drafts in two renderer instances.

### Flatten and object composition

Use object composition when an action combines fields with an entire browser
object whose keys may change. The shared renderer resolves each operand on click
and rejects duplicate keys. See [the input expression contract](../spec/action-input.md).

TypeScript infers the resulting input type:

```ts
import { mergeObjects, MERGE_OBJECTS_CAPABILITY, StdioTransport } from "@omochi-ui/sdk";
// Within a surface; run expects { title: string; count: number }.
const counts = ui.local("counts", { count: 1 });
ui.button("Run", run.call(mergeObjects({ title: "x" }, counts.value())));
// When this integration uses the updated omochi renderer:
await app.start(new StdioTransport([MERGE_OBJECTS_CAPABILITY]));
```

Python uses schemas for each operand and the result, preserving the action's
concrete input type. `Schema.input()` accepts a concrete value or `Expr[T]`;
`merge_objects()` checks field types and requiredness against the result schema.
Dynamic maps require a matching map result schema.

```python
from omochi_ui import Expr, merge_objects, schema, StdioTransport, MERGE_OBJECTS_CAPABILITY

title = schema.object_({"title": schema.string()})
counts = schema.object_({"count": schema.integer()})
result = schema.object_({"title": schema.string(), "count": schema.integer()})
input_expr = merge_objects(
    result,
    title.input({"title": "x"}),
    counts.input(Expr("/$local/counts")),
)
# run is registered with result (or a matching concrete dataclass schema).
ui.button("Run", run.call(input_expr))
await app.start(StdioTransport(capabilities={MERGE_OBJECTS_CAPABILITY}))
```

Rust uses the existing generated input fields without a new construction syntax:

```rust
#[derive(serde::Serialize, serde::Deserialize, omochi_ui::ActionInput)]
struct RunInput {
    title: String,
    #[serde(flatten)]
    counts: std::collections::BTreeMap<String, i64>,
}
// counts is a Local<BTreeMap<String, i64>>.
let call = run.call(RunInputInput {
    title: "x".into(),
    counts: counts.value().into(),
})?;
// Configure StdioTransport::with_capabilities([MERGE_OBJECTS_CAPABILITY])
// when connecting to the updated omochi renderer.
```

Existing concrete flatten inputs retain their Serde wire shape. Unresolved
flatten inputs now require the renderer capability instead of producing an
incorrect object containing `path` alongside sibling fields. Custom Rust
serialization still requires an adapter; unresolved fields with
`skip_serializing_if` are rejected because arbitrary Rust predicates cannot run
in the browser. No server trust or gateway protocol changes are required.

TypeScript, Python, and Rust client input validation failures are isolated to the received action: the
handler does not run, and the service continues accepting messages. Direct
`invoke()` still rejects. Handler exceptions and transport failures propagate.
