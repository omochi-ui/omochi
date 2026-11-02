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
| Register action | `@app.action(name)` | `app.action(name, handler)` | `app.action(name, handler)?` |
| Register view | `@app.surface(name)` | `app.surface(name, view)` | `app.surface(name, view)?` |
| Start stdio | `await app.start()` | `await app.start()` | `app.start().await` |
| Explicit transport | `await app.start(transport)` | `await app.start(transport)` | `app.start_with(transport).await` |
| Transaction | `await ctx.update(mutator)` | `await ctx.update(mutator)` | `ctx.update(mutator).await?` |

## Runtime and declarations

`App.build()` builds a runtime without selecting a transport. The runtime exposes `update`, `handle`, and action invocation. `handle` accepts `getSurface` observations and `invoke` commands as JSON. Explicit keys identify components; the root is always `root`.

`Action.call(input)` produces an action declaration for a component. `Local.bind(pointer)` binds an editable field; `Local.value(pointer)` creates an input expression. Inputs become concrete JSON before the action reaches the runtime.

## Transport exports

Python and TypeScript expose `StdioTransport`, `CallbackTransport`, and `WebSocketTransport`. TypeScript's `WebSocketServerTransport` and `UnixSocketServerTransport` are exported from `@omochi-ui/sdk/node`. Python exports `UnixSocketServerTransport` at its package root. Rust exposes a `Transport` trait, `StdioTransport`, `CallbackTransport`, and `UnixSocketServerTransport`.

## Errors

`DeclarationError` reports invalid declarations, `ProtocolError` invalid protocol usage, and `CapabilityError` unsupported rendering capabilities in Python and TypeScript. Rust uses its `Error`/`Result` types. Context update and transport failures propagate to the caller; completed updates are not rolled back.

## Source-level signatures

Full typed signatures and generated catalog properties are in the source:

- [TypeScript exports](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/typescript/src/index.ts)
- [Python exports](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/python/src/omochi_ui/__init__.py)
- [Rust exports](https://github.com/omochi-ui/omochi/blob/main/packages/sdk/rust/src/lib.rs)

See [complete examples](../../guides/sdks/), [components](../components/), and [media](../../guides/media/).
