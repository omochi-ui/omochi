# Action input expressions

An action's `context.input` is JSON with embedded A2UI v0.9 path references and
function calls. The renderer evaluates expressions at click time and sends only
concrete JSON. SDK schema validation runs before the handler.

## Object composition (capability `omochi.mergeObjects/1`)

The omochi renderer registers this synchronous function in its catalog:

```json
{
  "call": "omochiMergeObjects",
  "args": {
    "objects": [ {"title": "x"}, {"path": "/$local/nested"} ]
  },
  "returnType": "object"
}
```

Each operand is resolved recursively as an input expression. The function then
shallowly combines the operands' own JSON properties into a new object. Operands
may themselves contain merge calls. An empty operand list produces `{}`.
Ordinary function arguments, including nested arrays and objects, are resolved
recursively before invocation.
Resolved model values and function results are data: their `path` or `call`
properties are not reinterpreted as expressions. The existing A2UI reserved
`path`/`call` syntax still applies to inline input objects.

Every operand must resolve to a non-null JSON object. Missing references, scalar
or array operands, and duplicate keys (even with equal values) are errors.
Duplicate nested objects are not recursively merged. Evaluation does not mutate
operands. Prototype-sensitive keys are retained as own data properties.
Expression nesting is limited to 100 levels. An evaluation error is displayed by
the client and the invalid action is not sent; the connection remains usable.

All SDK builders detect the function recursively in component declarations and
require `omochi.mergeObjects/1`. Transport capabilities are explicit integration
configuration, not negotiated by this extension. Only enable this capability
when the connected renderer supports it. A renderer without the function must
not be advertised as supporting it. Default transports remain conservative and
reject declarations requiring unsupported capabilities before emitting a surface.
Full `getSurface` snapshots preserve the runtime's configured capabilities and
use fresh reconciliation state without changing subsequent incremental updates.
The wire protocol version and existing non-merge fixtures are unchanged.

## Rust Serde boundary

Generated `ActionInput` types preserve Serde serialization for concrete fields.
A flattened whole-value reference is emitted as an object composition operand,
so dynamic map keys do not need to be known by the SDK. Generated structured
inputs containing nested merge calls follow the same rule. Known ordinary fields
(including enum tags) form the first operand; unresolved flattened fields follow
in declaration order. Duplicate keys are rejected regardless of order.
Internally tagged newtype enum payloads use this same composition rule, keeping
the tag separate from a referenced payload until the renderer resolves it.

A whole-value operand must reference a complete value, for example a separate
`Local<Nested>`. Projecting a `#[serde(flatten)]` field from its parent produces
`Local<Nested, FieldsOnly>`: chained field projections are allowed, but `.value()`
and `.bind()` on the flattened object do not compile because sibling keys share
the same JSON object. This restriction also applies to internally tagged newtype
payloads and survives newtype/transparent wrappers. Reconstruct a structured
input from individual field expressions when a separate local is not available.

The derive implementation uses a private typed Serde map-key marker to carry
unresolved flattened operands to the SDK serializer. This marker is not a JSON
string key and is never transmitted. Serialize generated inputs through
`encode()` or `ActionHandle::call()` so this serializer is used.

Browser values are already JSON. Arbitrary Rust serializer code is never shipped
to the browser. Custom serialization requires an explicit `ExpressionAdapter`.
`skip_serializing_if` works on concrete values; an unresolved value at such a
field is rejected at encoding rather than silently ignoring its predicate.
For tuple structs and tuple enum variants, no field projection is generated for
a conditionally skipped field or any later field: the field can be absent and
later serialized indices depend on its value. Earlier fields and generated input
encoding remain supported. Unconditionally skipped fields do not occupy an index.
Concrete null/Option flattening retains Serde behavior; a reference resolving to
null is not an object and is rejected. Use an empty object to represent no keys.

`fixtures/action-input.json` contains the cross-SDK merge encoding and resolved
value, an internally tagged newtype merge, a merge with ordinary function
arguments, and the existing handler validation cases.
