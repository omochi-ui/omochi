---
title: "SDK API reference"
description: "SDK API reference · omochi"
sidebar:
  order: 1
---

## Public entry points

| Concept | Python | TypeScript | Rust |
| --- | --- | --- | --- |
| App | `App(state)` | `new App(state)` | `App::new(state)` |
| Register action | `@app.action` or `@app.action(name)` | `app.action(name, schema, handler)` | `app.action::<Input, _, _>(name, handler)?` |
| Register view | `app.mount(view)` or `@app.surface(name)` | `app.surface(name, view)` | `app.surface(name, view)?` |
| Start stdio | `await app.start()` | `await app.start()` | `app.start().await` |
| Explicit transport | `await app.start(transport)` | `await app.start(transport)` | `app.start_with(transport).await` |
| Transaction | `await ctx.update(mutator)` | `await ctx.update(mutator)` | `ctx.update(mutator).await?` |

Python infers the action input schema from the handler's first parameter annotation;
use `schema=` for an explicit schema. Python also supports `async with ctx.update() as state:`.
TypeScript requires a schema argument. Rust deserializes the handler's input type;
derive `ActionInput` to construct typed expressions and field projections.

## Runtime and declarations

`App.build()` builds a runtime without selecting a transport. The runtime exposes `update`, `handle`, and action invocation. `handle` accepts `getSurface` observations and `invoke` commands as JSON. Explicit keys identify components; the root is always `root`.

`Action.call(input)` produces an action declaration for a component (`action.call(input)?`
in Rust). Project a field before binding it or reading its browser value:

| Language | Editable binding | Action input expression |
| --- | --- | --- |
| Python | `form.field("temperature", float).bind()` | `form.field("temperature", float).value()` |
| TypeScript | `form.field("temperature").bind()` | `form.field("temperature").value()` |
| Rust | `form.field(RunInput::field_temperature()).bind()?` | `form.field(RunInput::field_temperature()).value()` |

Here `form` is a Local whose input type contains a numeric `temperature` field;
Rust's `RunInput` derives `ActionInput`. Python also accepts `form.temperature`
directly as a control's `bind=` or in an action's `input=`.
`bind()` and `value()` take no pointer argument and refer to the current Local,
including the whole value when no field is selected. Inputs become concrete JSON
before the action reaches the runtime. See [browser-owned input](../../concepts/inputs/).

## Transport exports

Python and TypeScript expose `StdioTransport`, `CallbackTransport`, and `WebSocketTransport`. TypeScript's `WebSocketServerTransport` and `UnixSocketServerTransport` are exported from `@omochi-ui/sdk/node`. Python exports `UnixSocketServerTransport` at its package root. Rust exposes a `Transport` trait, `StdioTransport`, `CallbackTransport`, and `UnixSocketServerTransport`.

In Python and TypeScript, `runtime.shutdown()` stops admitting transport messages,
stops background tasks, and closes the resource registry. Built-in server transports
also close their listeners and remove Unix discovery publications. Use
`runtime.wait_closed()` (Python) or `runtime.waitClosed()` (TypeScript) to wait for
cleanup to finish. An action already running may finish before `start()` returns.

Custom transports can opt into runtime cleanup with `async stop_receiving()`
(Python) or `stopReceiving()` (TypeScript). The hook should stop reads and release
resources owned by that transport. Callback and WebSocket adapters stop their
receive loop; the caller retains ownership of the underlying connection. Runtime
shutdown does not call an arbitrary custom transport's `close()` method.

## Errors

`DeclarationError` reports invalid declarations, `ProtocolError` invalid protocol usage, and `CapabilityError` unsupported rendering capabilities in Python and TypeScript. Rust uses its `Error`/`Result` types. Context update and transport failures propagate to the caller; completed updates are not rolled back.

## Source-level signatures

Full typed signatures and generated catalog properties are in the source:

- [TypeScript exports](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/typescript/src/index.ts)
- [Python exports](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/python/src/omochi_ui/__init__.py)
- [Rust exports](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/rust/src/lib.rs)

See [complete examples](../../guides/sdks/), [components](../components/), and [media](../../guides/media/).
