# Rust server migration

This workspace implements the native CLI, gateway, hub and utilities. Source
checkout entry points launch these Rust binaries. npm stubs contain only launchers;
the former TypeScript services remain in Git history. SDK APIs and the browser
dashboard remain unchanged apart from full EndpointId resource routes.
The wire contract is documented in [gateway-v4.md](../spec/gateway-v4.md).
Production relay validation across networks and platform/browser validation
remain release checks.

## Implemented

- `omochi-protocol`: protocol version 4, separate gateway and enrollment ALPNs,
  bounded length-prefixed JSON, control/service messages, session-generation guard.
- `omochi-transport`: pinned iroh 1.4.1, authenticated `relay.omochi.fyi` by default
  in release builds (n0 relays in debug builds), optional custom relays, public
  address lookup in online mode, mDNS lookup, and offline endpoints without public services.
- Durable role-specific keys and serialized state writes with atomic rename and
  fsync; 180-second single-use enrollment, identity-bound retry, and record revocation.
- Enrollment exchange over the dedicated ALPN, using the authenticated remote
  EndpointId rather than a peer identity supplied in the request.
- `omochi-local-discovery`: existing SDK manifest validation and bounded scanning
  of private runtime directories, ignoring invalid manifests and symlink directories.
- Renderer resource routing accepts full lowercase hex EndpointIds as well as
  current identifiers while the old servers remain in use.

## Verification

```sh
bun run test:server-rust
cargo clippy --manifest-path crates/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path crates/Cargo.toml mdns_resolves -- --ignored --nocapture
bun run --filter @omochi-ui/renderer test
```

The explicit multicast test resolves an EndpointId without providing its address,
using endpoints configured without public DNS or relays. This is a same-host
integration test, not proof of reachability between every pair of LAN machines.
The ordinary direct-connection test provides an address explicitly. A separate relay integration test disables IP transports and external lookup on
both endpoints, then verifies enrollment and framed traffic via a local relay.

- `omochi-gateway`: shared SDK connections, filesystem notifications with polling fallback, snapshot subscriptions,
  actions, bounded resource requests, generation changes, and Unix stream dispatch.
  Integration tests cover replay, action/resource forwarding, version rejection,
  duplicate socket refusal, and owned-socket shutdown cleanup.

- `omochi-hub`: Unix/iroh stream clients, service snapshot aggregation, action
  routing with client-draft isolation, generation replacement, resource forwarding.
- Gateway remote dispatcher checks authenticated peer roles, bounds active
  connections/streams, separates enrollment, and closes revoked connections.
  Cross-process leases acknowledge each authorization generation only after revoked
  sessions and their stream tasks are stopped. A durable revocation watermark forces
  fresh sessions even when a peer is removed and re-enrolled between polls.
  `trust forget` accepts a peer ID or unique name and reports an error when any
  live process fails to acknowledge; the trust removal remains durable.

- Browser HTTP boundary: WebSocket A2UI, services/SSE, dashboard CRUD with
  revision conflicts and atomic persistence, presets, static assets, and streamed
  resource GET/HEAD/range responses. HTTP and WebSocket share Origin checks.
  Integration tests drive real HTTP/WebSocket connections through gateway to a
  synthetic SDK socket, including draft isolation and resource range reads.

- Native `omochi-ui` binary starts the gateway/hub, embeds dashboard assets, imports
  invitations via stdin, persists role-specific identities, and lists trusted peers.
  Registered gateways connect in the background; peers added after hub startup are
  picked up without restarting. Local-only startup creates no iroh endpoint.
  Control-stream heartbeats disconnect stalled sessions so supervision can reconnect.
  Materialized snapshots are checked against the per-frame limit before publication.
- Native process tests run with Bun unavailable on PATH and verify embedded UI,
  local services, persistent identities, and graceful socket cleanup.

Build both native binaries and their embedded renderer/dashboard assets:

```sh
bun run build:native
bun run omochi-ui --help
```

- Native `up` reads the existing TOML composition format, validates semantic rows,
  starts command arrays without a shell, exposes presets, and opens the selected
  preset with `--open`. Shutdown terminates owned process groups, including descendants.
  Utility entries resolve the sibling `omochi-utils` binary; currently its
  `button`, `watch`, `process`, `tail` and `http` modes are available for compositions.
  Custom `up --runtime-dir` paths must end in `/omochi` so child SDKs retain the
  existing `XDG_RUNTIME_DIR/omochi` discovery contract.
- Process tests verify cross-process trust acknowledgement (including a stopped
  gateway that must fail acknowledgement), composition presets and child cleanup.

- Native `generate catalog` produces TypeScript, Python and Rust bindings, resolves
  local schema references, rejects circular references and supports `--check`.
  Golden fixtures from the previous generator verify byte-for-byte output parity.
  Generation does not initialize identity state or need Bun at runtime.

- `omochi-utils` Rust library now has serialized batch publication through the
  SDK-compatible native Unix transport and a bounded, non-consuming event cursor buffer.
  Tests cover batch rejection, late snapshot replay, actions and cleanup. The
  native binary now supports `button`, `watch`, `process`, `stream`, `tail` and `http`, with bounded UTF-8 output, split
  character decoding, EOF retention, file rotation/truncation and graceful signals.
  Process tests run without Bun and verify shutdown even while stdin is idle.
  The command runner uses argument arrays and bounded output channels, combines
  stdout/stderr, and terminates process groups on cancellation or timeout. Button
  process tests verify action-triggered execution and rejection of overlapping clicks.
  Process controls include manual start/stop/restart and all restart policies.
  Watch supports non-overlapping execution, timeouts, pause/resume/run-now, bounded
  unified diffs/history and unchanged-output suppression. Native process tests cover
  these controls and policies. HTTP monitoring supports status/body expectations,
  bounded display with streaming matching beyond the display limit, JSON formatting,
  failure/recovery state, pause/resume and manual probes. HTTP tests cover large
  bodies, split Unicode, binary responses, timeouts and real native recovery.
  MCP is served by the official `rmcp` Rust SDK, with the three existing tools,
  cancellable event waits, 4 MiB input lines and at most 16 active tool calls.
  `bun run test:native-mcp` verifies the existing TypeScript MCP client through
  the native utils, gateway and hub, including state replay, actions, cancellation
  and surface retraction on disconnect. Bun is only the test client.

- Native `dev -- COMMAND [ARGS...]` bridges JSONL stdio to WebSocket, keeping
  surface IDs and client metadata intact. It shares Origin checks and resource
  streaming/range behavior with the hub, using the standalone `_` resource route.
  Tests use a real child and WebSocket clients with no Bun on PATH: replay,
  actions, range reads, invalid input, normal exit and failure close codes.
  Child process groups are terminated on shutdown and the child exit code is preserved.

## Validation and release checks

The Rust workspace, renderer client/model, MCP client, SDK regression tests and
native sample composition pass locally. Control heartbeat tests cover stale pong
rejection. Active hub resource revocation tests verify interrupted reads and
surface removal when live authorization acknowledgement completes. Local relay-only
enrollment/data tests and process-exit recovery after invitation commits also pass.
[Local performance measurements](../docs/benchmarks/native-local.md) compare
five fresh-state runs of the retained TypeScript services and native release.

Gateway, hub, dev and utility fanout rings enforce 16 MiB of encoded JSON and
message counts; action and publication queues retain an 8 MiB budget through
processing. Native utils use bounded Unix JSONL I/O with at most 16 clients.
Slow receivers detect lost updates and reconnect for a snapshot. These are
wire-data budgets, not bounds on total process RSS.

The following environment-dependent checks remain before distribution:

- Visual browser validation. Real renderer client/model tests cover snapshots,
  actions, late joins and service restart, but do not exercise DOM layout.
- macOS packaging/runtime validation. Linux installation and runtime-free startup
  pass locally; the macOS CI job is configured but has not been executed here.
- Validate `relay.omochi.fyi` between separate networks. The
  [deployment templates](../deploy/relay/README.md) include TLS, public admission,
  rate limits, systemd limits and monitoring. The upstream binary passes a local
  verified-TLS/config/metrics smoke test. Release clients use the deployed project
  relay with shared-token admission; cross-network validation remains separate.

## Migration decisions

Use EndpointId as the new NodeId. Preserve SDK APIs and local application transport;
allow only necessary browser changes. Existing peers and saved dashboards are not
migrated. Keep legacy state untouched in a separate identity domain. Users import
an invite on the hub PC; possession grants access without a gateway confirmation.
Registered gateways reconnect automatically. Discovery never grants authorization.
Remote endpoints must only be created after explicit remote enablement.

## Native artifacts

`bun run build:native:release` builds the dashboard and both optimized binaries,
then creates a host-specific archive and checksum under `artifacts/native`.
The archive includes an installer and per-file checksums. Run
`python3 scripts/smoke-native.py artifacts/native/<archive>.tar.gz` to verify an
isolated installation and embedded UI with language runtimes absent from PATH.
The native CI workflow builds/tests Linux and macOS. The separate native release
workflow publishes the Cargo crates and stages generated npm CLI/utils wrappers
under their existing names. Source server workspaces remain private; only the
generated npm distributions are published. See [native registry releases](../docs/native-releasing.md).
