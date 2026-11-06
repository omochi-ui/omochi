---
title: "Transports"
description: "Transports · omochi"
sidebar:
  order: 6
---

Choose the connection between an SDK app and its clients.

| Transport | Use | Availability |
| --- | --- | --- |
| `UnixSocketServerTransport` | Publish an app for discovery by a Hub | Python, Node.js/Bun, Rust |
| `StdioTransport` | JSONL stdin/stdout for a bridge or custom client | All SDKs; default for `app.start()` |
| `WebSocketTransport` | Attach a caller-owned WebSocket-like connection | Python, TypeScript |
| `WebSocketServerTransport` | Host browser clients directly | Node.js/Bun |
| `CallbackTransport` | Integrate with another message channel | All SDKs |

Python exports its transports from `omochi_ui`; TypeScript server transports are in `@omochi-ui/sdk/node`, and the other transports in `@omochi-ui/sdk`. Rust exports its `Transport` trait and adapters from `omochi_ui`.

## Unix discovery

See the [SDK guide](../../guides/sdks/) for complete examples. Run clients and services as the same OS user with the same runtime directory. The default is `$XDG_RUNTIME_DIR/omochi`, or `<system temporary directory>/omochi-<uid>/omochi` when unset or empty. Supply the SDK transport's runtime-root option when overriding this location; CLI `OMOCHI_RUNTIME_ROOT` does not configure standalone SDK apps.

Choose a service ID matching `[A-Za-z0-9._-]{1,64}`, excluding `.` and `..`. Concurrent publishers with the same ID are rejected. Keep the ID stable to reuse saved layouts. TypeScript Unix discovery requires its native optional dependency on Linux/macOS x64 or arm64; keep optional dependencies enabled.

| SDK | Runtime directory option | Capability option |
| --- | --- | --- |
| Python | `runtime_dir=` on `UnixSocketServerTransport` | `capabilities=` |
| TypeScript | `runtimeDir` in transport options | `capabilities` |
| Rust | `UnixSocketServerOptions::with_runtime_dir(...)` | `.with_capabilities(...)` |

## Stdio and custom clients

`await app.start()` selects stdio in Python and TypeScript; Rust uses `app.start().await`. Logging must go to stderr because stdout carries JSONL. A stdio app needs a bridge or a client to display its UI. The source-based Vite preview is described in [Contributing](https://github.com/omochi-ui/omochi/blob/main/CONTRIBUTING.md#preview-a-stdio-application).

For an explicit transport, use `await app.start(transport)` in Python/TypeScript and `app.start_with(transport).await` in Rust. Each transport carries one app. Configure renderer capabilities before the first Surface is rendered; enable a capability only when the client implements it.

Python stdio and Unix transports accept up to 4 MiB of UTF-8 bytes per JSONL line, excluding LF; a preceding CR counts. Oversized lines end stdio reception with `ValueError` or close the offending Unix connection. A final line without LF is accepted within the same limit.

## Direct WebSocket server

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

WebSocket handshakes and HTTP resource requests share an Origin policy: only
HTTP(S) origins on `localhost`, `127.0.0.1`, or `[::1]` (with an optional port)
are accepted. External, malformed, empty, and `null` origins are rejected with
HTTP 403 before the WebSocket opens, so they cannot read snapshots or resources
or invoke actions. Non-browser clients may omit Origin. Requests marked
`Sec-Fetch-Site: cross-site` are rejected even without Origin. Changing `host`
does not widen this policy; use the hub's explicit origin configuration for
external browser access.


## Shutdown

In Python and TypeScript, `runtime.shutdown()` stops admitting transport messages,
stops background tasks, and closes the resource registry. Built-in server transports
also close their listeners and remove Unix discovery publications. Use
`runtime.wait_closed()` (Python) or `runtime.waitClosed()` (TypeScript) to wait for
cleanup to finish. An action already running may finish before `start()` returns.

In TypeScript, `shutdown()` returns a resolved promise once shutdown is requested,
including on the first call; `await runtime.shutdown()` does not wait for cleanup.
Tasks registered with either `spawn(async signal => { ... })` or `spawn(promise)`
may request shutdown and then finish their own cleanup. Outside those tasks, use
`await runtime.shutdown(); await runtime.waitClosed();` to wait for every registered
task and the resource registry to finish closing. Tasks must not await `waitClosed()`
themselves because it waits for them to finish. Both `waitClosed()` and `start()`
report cleanup failures; `start()` preserves an earlier action or transport failure.

Custom transports can opt into runtime cleanup with `async stop_receiving()`
(Python) or `stopReceiving()` (TypeScript). The hook should stop reads and release
resources owned by that transport. Callback and WebSocket adapters stop their
receive loop; the caller retains ownership of the underlying connection. Runtime
shutdown does not call an arbitrary custom transport's `close()` method.


## Custom resource delivery

Custom transports must preserve resource frames separately from A2UI snapshots and reply to the originating connection. Built-in transports handle this. The browser renderer's `useA2uiSession(wsUrl, resourceBaseUrl?)` accepts an HTTP resource base override; otherwise it uses the WebSocket origin.

See the [resource wire contract](https://github.com/omochi-ui/omochi/blob/main/spec/resources.md) when implementing an adapter.
