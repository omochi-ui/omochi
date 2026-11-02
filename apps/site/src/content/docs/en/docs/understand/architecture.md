---
title: "Architecture"
description: "Architecture · omochi"
sidebar:
  order: 1
---

Omochi's server implementation is Rust. SDKs emit A2UI v0.9; the shared browser
renderer and dashboard remain TypeScript/React. npm CLI packages are launchers for
the native executables, not parallel server implementations.

```text
SDK app (TypeScript / Python / Rust)
    │ manifest + Unix JSONL
    ▼
Rust gateway ── Unix / authenticated iroh streams ── Rust hub
                                                       │ HTTP / WebSocket
                                                       ▼
                                              Dashboard + renderer
```

## Source layout

| Directory | Responsibility |
| --- | --- |
| `crates/protocol` | Gateway v4 framing, sessions, bounded queues |
| `crates/transport` | iroh endpoints, invitations, identity/trust persistence |
| `crates/discovery` | Validate local SDK manifests and watch service lifetimes |
| `crates/gateway` | Share SDK connections, replay snapshots, route actions/resources |
| `crates/hub` | Aggregate gateways, browser HTTP/WebSocket, dashboards and embedded assets |
| `crates/cli` | `omochi-ui` commands, compositions, stdio preview and catalog generation |
| `crates/utils` | `omochi-utils` modes and MCP server |
| `packages/npm/{cli,utils}` | Private checkout stubs and public npm metadata templates |
| `packages/npm/launcher.cjs` | Published native binary selection and process forwarding |
| `packages/sdk/{typescript,python,rust}` | Stable application-facing SDK implementations |
| `packages/renderer`, `packages/hub-ui`, `packages/dev-server` | Shared renderer, dashboard and preview frontend |
| `packages/core` | TypeScript SDK/browser transport helpers and test utilities |
| `tests/native` | Cross-process client integration tests against native services |

The Cargo workspace is `crates/Cargo.toml`. The existing Rust SDK stays an
independent package and is reused through a versioned path dependency. Bun manages
SDK/frontend packages and native npm stubs; root scripts invoke Cargo explicitly.
Native versions are defined by Cargo. npm stub version `0.0.0` is only a private
workspace placeholder and is replaced when generating distributions.

## Connection and authorization boundaries

SDK manifests and Unix JSONL remain language-independent. Private, owned runtime
directories are the local trust boundary; discovery validates IDs and paths before
using any manifest. A gateway shares one connection per SDK service.

Remote connectivity is opt-in. Invitations are imported on the hub PC and register
a gateway's full iroh EndpointId. Registered peers reconnect independently by ID;
addresses and LAN discovery never grant authorization. Session generations reject
stale service actions, and reconnects start with a new snapshot. Revocation waits
for all live participants to reload authorization and destroy revoked sessions.

The hub remains loopback-only by default. HTTP and WebSocket use the same Origin
checks. Browser surface IDs are namespaced by gateway and service; actions and
client draft metadata are narrowed to their target before returning to the SDK.
Resources use separate request streams, preserving browser range and HEAD reads.

See [gateway v4](https://github.com/omochi-ui/omochi/blob/main/spec/gateway-v4.md) for exact framing, limits and lifecycle;
[discovery](/en/docs/guides/remote/) for setup; and [native releases](https://github.com/omochi-ui/omochi/blob/main/docs/native-releasing.md) for
crates.io/npm packaging. Dashboard assets are built with Bun and embedded in the
native binary and published hub crate. Installed Cargo sources need no Bun build.

The former TypeScript server implementation is available in Git history before
this layout change (for example commit `b8a2f85`). It is not built or tested by the
active workspace. Historical performance measurements remain in
[benchmarks](https://github.com/omochi-ui/omochi/blob/main/docs/benchmarks/native-local.md).
