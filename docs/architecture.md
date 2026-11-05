# Codebase architecture and implementation specification

[Documentation map](README.md) · [Protocol contracts](../spec/README.md) · [Contributing](../CONTRIBUTING.md)

This document describes the current implementation for contributors: runtime
semantics, module responsibilities, connection boundaries, persistence and
verification. It covers the three SDKs, native services, browser renderer,
dashboard, Flutter client and distribution tooling. Detailed wire contracts are
linked where they apply. User instructions live in the bilingual documentation
site; dated acceptance records and archived designs have their own scope.

Omochi attaches declarative UIs to running programs. Applications own state and
action handlers, SDKs emit A2UI v0.9, and clients render and return concrete action
input. The server implementation is Rust. React and Flutter are separate
rendering adapters. npm CLI packages launch native executables.

## System paths

```text
TypeScript / Python / Rust app
    │ manifest + Unix JSONL (A2UI v0.9 and resources/1)
    ▼
Rust Gateway
    │ Gateway v4: local Unix streams or authenticated iroh streams
    ▼
Rust Hub ── HTTP / WebSocket / SSE ── local React dashboard
    └────── iroh owner/share protocols ── hosted React / external-Hub Flutter client

Rust Gateway ── authenticated iroh ── Flutter app's embedded Rust Hub ── GenUI

Single app ── stdio JSONL ── Rust dev bridge ── WebSocket / HTTP resources ── preview
```

The hosted dashboard's web server supplies static assets. Application traffic
flows between the client and the selected native Hub. Direct browser-to-Gateway
aggregation is an opt-in implementation under `experiments/browser-dashboard`.

## Domain objects and ownership

| Object | Identity and owner | Meaning |
| --- | --- | --- |
| Application state | SDK application's memory | Authoritative values changed through runtime updates and action handlers |
| Declaration | One SDK Surface render | Normalized IR version `2`: catalog, component tree, model defaults, locals and required capabilities |
| Surface | Application-defined `surfaceId` | One A2UI UI/model lifetime, containing Surface-local component IDs |
| Local/input model | Client's Surface model | Draft values; editing alone does not mutate application state |
| Service | Validated `serviceId` in a private runtime directory | One published SDK/utility connection, potentially containing several Surfaces |
| Service generation | Gateway-assigned live instance | Rejects actions/resources aimed at a replaced SDK connection |
| Node | iroh EndpointId, 64 lowercase hex characters | Gateway identity; names and addresses are descriptive/routing data |
| Hub | Its own identity and authorization stores | Aggregates Gateway services and owns saved dashboards |
| Dashboard/board | Hub-issued ID and positive revision | Name and panel references/geometry; independent of live application state |
| Panel | Board-local ID | Title, `{nodeId, serviceId, surfaceId}` reference and `{x,y,w,h}` geometry |
| Gateway trust | Authenticated peer ID plus registered role | Permits a Hub to use the Gateway's exposed services |
| Owner grant | Hub-local record bound to a client EndpointId | Full Hub UI/control authority, with owner enrollment/revocation kept local |
| Share grant | Hub-local record for one saved board | Bearer links with independent interaction/layout permissions |
| Surface lease/handle | Issued by a live subscription | Opaque authority for actions/images during that Surface and authorization lifetime |
| Resource | SDK registry's random 32-character hex ID | Immutable registered media addressed as `omochi://resources/<id>` |

Board source references do not establish a connection or grant access. Gateway
enrollment, owner enrollment and dashboard sharing are separate authorization
domains. None promotes a peer into another role implicitly.

## Source layout and dependency boundaries

| Directory | Responsibility |
| --- | --- |
| `packages/sdk/typescript/src` | `App`, `Runtime`, builders, expressions, schemas, reconciliation and resources; Node/Bun server transports in `node/` |
| `packages/sdk/python/src/omochi_ui` | Equivalent runtime plus typed Python input composition and context-based UI declarations |
| `packages/sdk/rust/src`, `packages/sdk/rust/macros` | Equivalent Rust runtime, Serde-aware typed expressions and `ActionInput` derive |
| `packages/core/src` | TypeScript JSONL, snapshots, resource broker, client-frame validation, browser session contracts, namespacing and test helpers |
| `crates/protocol` | Gateway v4 framing, sessions, data-path validation, bounded fanout and queues |
| `crates/downstream-core` | Transport-independent owner/share client protocol and apply/ACK state for WASM/native adapters |
| `crates/mdns` | Platform address-discovery backends, record aggregation and cancellation |
| `crates/transport` | iroh endpoint binding, relay selection, enrollment, identity/trust and durable storage |
| `crates/discovery` | Local SDK manifest validation and filesystem watching |
| `crates/gateway` | One upstream SDK connection per service, snapshots, subscriptions, action/resource routing |
| `crates/hub` | Gateway aggregation, scoped subscriptions/images, boards, owner/share control, local HTTP/WS/SSE and embedded assets |
| `crates/cli` | `omochi-ui`, process supervision, compositions, stdio preview and catalog generation |
| `crates/utils` | `omochi-utils` publishers, command/process/watch/stream/tail/HTTP modes and MCP server |
| `crates/native-bridge` | C ABI v2 for Flutter's embedded Hub and external-Hub client |
| `packages/renderer` | React A2UI session/model application, Surface rendering, input expressions and media adapters |
| `packages/hub-ui` | Local/hosted dashboard entries, board repositories, layout editor, owner/share UI and client identity/recovery |
| `packages/dev-server` | Vite single-application preview using the shared renderer |
| `packages/hub-transport` | Hosted client's WASM iroh transport to a native Hub; independent Cargo workspace |
| `packages/npm/{cli,utils}`, `packages/npm/launcher.cjs` | Private checkout stubs, release templates and native executable selection/forwarding |
| `apps/dashboard` | Static hosted dashboard Worker and HTTP header policy |
| `apps/mobile` | Flutter/GenUI, native library build, OS storage, deep links and lifecycle adapters |
| `apps/site`, `apps/mobile-site` | Bilingual documentation and mobile landing page |
| `examples` | SDK programs, media, quickstart compositions and agent-approval integration |
| `spec` | Protocol contracts, normalized IR/board schemas and conformance fixtures |
| `tests`, `scripts`, `.github/workflows` | Cross-process/browser checks, build/package/release automation and CI |
| `deploy` | Native container, relay and static-site deployment configuration |
| `vendor` | Narrow platform/network compatibility patches used through Cargo overrides |
| `experiments` | Direct-Gateway browser implementations, probes and dated evidence |

The main native workspace is [`crates/Cargo.toml`](../crates/Cargo.toml). The
Rust SDK is an independent package; native crates reuse its snapshot support
through a versioned path dependency with runtime features disabled where
appropriate. `downstream-core` and `native-bridge` are private implementation
crates. Bun manages SDK/browser packages and launcher stubs; root scripts invoke
Cargo explicitly. `packages/core` contains reusable TypeScript helpers, while
native service implementations live in `crates/`.

## SDK declaration and execution contract

The shared IR is defined by [`ir.schema.json`](../spec/ir.schema.json). Each
Surface declaration contains `irVersion`, `surfaceId`, `catalogId`, `root`, `data`,
`locals` and `requiredCapabilities`. Nodes have a unique key, catalog component
name, JSON props and optional child slots. Slots describe a JSON Pointer and
`single`, `list` or `template` children. Builders create a `Column` root with key
`root`; explicit stable keys preserve component identity across renders.

Applications register Surfaces and typed actions before building the runtime.
TypeScript uses explicit schemas, Python supports schemas or annotated input
types, and Rust deserializes through Serde. Python additionally provides
context-based UI functions and `mount()`. Rust's `ActionInput` derive generates
typed expression/projection support. These APIs differ in language idiom while
sharing declaration and wire semantics.

The runtime serializes action execution and state publication. A successful
update increments the application revision, evaluates registered views, reconciles
them and sends the resulting messages. Action contexts provide update operations
so a running handler can publish intermediate progress. TypeScript and Python
also batch pending external updates before a render. Rendering is a synchronous
declaration step; long-running work belongs in handlers or runtime tasks.

Client edits live in the renderer's data model. Unbound inputs use `/inputs/…`;
explicit locals use `/$local/<key>/…`. Local values are initialized when declared
and reset when `resetKey` changes. Changing `initial` alone does not overwrite a
surviving local. Bindings refer to locals declared by that builder. Input changes
are not implicit server action callbacks; a button/action explicitly collects
input and sends it for validation and handling.

Reconciliation has the following observable behavior:

- Initial publication is `createSurface`, `updateComponents`, then a root
  `updateDataModel` containing declared data and local initial values.
- Subsequent renders emit changed component definitions and model path patches.
  Model deletion is a path update without `value`; arrays are replaced as values.
  Removed children are disconnected through their parent's updated references.
- Changing catalogs deletes and recreates the Surface. Closing a known Surface
  emits `deleteSurface`.
- Full `getSurface` output uses fresh reconciliation state with the configured
  capabilities, leaving the incremental reconciler unchanged.
- Required capabilities are checked before emission. Transport configuration
  explicitly declares support; it is not automatically negotiated.

Action expressions are evaluated recursively by the renderer at invocation
time. Only resolved JSON reaches a handler. `context.input` takes precedence,
including explicit `null`; only its absence falls back to the legacy action-level
`input`. Invalid schema input is rejected before calling the handler and is
recoverable at the receive loop. Renderer error reports do not invoke actions;
handler failures are not treated as input-validation failures.

Object composition uses `omochiMergeObjects` and capability
`omochi.mergeObjects/1`. It rejects non-object operands and duplicate keys rather
than merging nested objects. Rust's flattened Serde fields and projection
restrictions follow the [action input contract](../spec/action-input.md).
Catalog generation in [`catalog.rs`](../crates/cli/src/catalog.rs) produces
typed TypeScript, Python and Rust bindings; adding a catalog name alone does not
install its client rendering implementation.

Runtime shutdown stops receiving, cleans up owned tasks and closes the resource
registry. Transport-specific shutdown controls socket/listener ownership. See
the three SDK `app`, `builder`, `reconcile` and transport modules for the concrete
language APIs, and the [basic message fixture](../spec/fixtures/basic.json) for
cross-language ordering.

## Local publication, discovery and Gateway

SDK transports default to stdio JSONL for a single child-process integration.
All three SDKs expose a Unix socket server publisher for discovery. TypeScript
also supplies callback/WebSocket transports and a Node/Bun WebSocket server.
The Unix service publishes `<runtimeRoot>/<serviceId>/manifest.json` with
`{name,socket,icon?}` and normally listens on `a2ui.sock` in that directory.
Service IDs use 1–64 ASCII letters/digits/`.`/`_`/`-`, excluding `.` and `..`.

The runtime root normally follows `$XDG_RUNTIME_DIR/omochi`; when XDG is unset or
empty, the fallback is `<tempdir>/omochi-<effective uid>/omochi`. The private
owned directory is local authentication. SDK publishers and native utilities
hold the same nonblocking exclusive flock for a service's complete lifetime.
Persistent `.<serviceId>.lock~` files must keep their inode across shutdown and
crash recovery. Only the lock owner may probe/remove a stale socket and publish
the manifest. See [local service ownership](../spec/local-service-ownership.md).

[`crates/discovery`](../crates/discovery/src/lib.rs) validates the root, service
IDs, bounded regular-file manifests, socket filenames and icon metadata before
use. Filesystem notifications wake a validated scan; periodic scans remain
necessary. Invalid/partial manifests are omitted until a valid later scan.
Discovery supplies candidates and never grants remote trust.

The Gateway shares one SDK connection across downstream subscribers, maintains
replayable A2UI state and brokers resource requests separately. Each live service
has a generation. Losing/replacing it retracts its Surfaces; reconnect requires a
fresh snapshot. Resource announcements/replies are excluded from UI snapshots.
One service exceeding replay bounds is retracted without invalidating unrelated
services. Roster admission is bounded by both service count and encoded bytes.

Gateway v4 uses a nonempty UTF-8 JSON body with a four-byte big-endian length,
bounded to 4 MiB, on both local Unix streams and remote iroh. Control streams
carry hello, full service rosters and ping/pong. Service streams carry
`snapshotStart`, messages, `snapshotEnd` and then live messages; the reverse
direction carries client actions/errors. Resource streams perform one request
and response. Stream classes have separate admission reservations so service
subscriptions cannot consume all control/resource capacity.

## Hub aggregation and browser boundary

The common [`Hub`](../crates/hub/src/lib.rs) owns Gateway sessions, snapshots,
service routes, actions and subscription lifetimes independently of HTTP.
Its [host adapter](../crates/hub/src/host.rs) observes platform authorization
state and acknowledges completed revocation teardown. Desktop defaults enable
browser and desktop features; the embedded
client reuses the common Hub with its own capacity policy.

Hub-facing service routes are `<nodeId>~<serviceId>`, and Surface IDs are
`<nodeId>~<serviceId>/<surfaceId>`. Action routing restores the SDK's Surface ID
and narrows client model metadata to the destination service. Epoch/generation
checks prevent a retired session from acting on or removing a successor's state.
If Unix and iroh both reach the same Gateway identity, one control session owns
subscriptions while other live sessions remain standby. Ownership transfers only
after old streams finish and their Surfaces are retracted.

The local Hub defaults to `127.0.0.1:5555` and starts a local Gateway if needed.
[`http.rs`](../crates/hub/src/http.rs) exposes these boundaries:

| Route | Purpose |
| --- | --- |
| `/` and static assets | Embedded dashboard or explicitly selected asset directory |
| `/ws` | A2UI updates and client actions; optional applied-ACK image extension |
| `/services`, `/events` | Current service roster; SSE sends complete roster replacements |
| `/api/dashboards`, `/api/dashboards/{id}` | Saved board list/create/update/delete |
| `/api/presets` | Read-only composition presets |
| `/api/connections` | Registered Gateway connection diagnostics |
| `/api/sharing`, `/api/shares`, `/api/shares/{id}` | Downstream configuration and share administration |
| `/api/owners`, `/api/owners/{id}` | Local-only owner enrollment/revocation administration |
| `/resources/{route}/{id}` | SDK media GET/HEAD with byte ranges |
| `/images/{reference}` | One-use, subscription-scoped validated image delivery |

HTTP routes and WebSocket upgrades share Origin enforcement. Loopback HTTP(S)
origins and explicit `--allow-origin` entries are accepted; requests without an
Origin are allowed. This is a local owner endpoint without TLS or user login.
`hub --remote` adds authenticated iroh downstream access without widening the
local HTTP bind or Origin policy.

## Identity, enrollment and remote authority

[`crates/transport`](../crates/transport/src/lib.rs) binds iroh endpoints with
separate durable Gateway and Hub keys. QUIC authenticates the exact EndpointId;
ordinary Gateway access also checks the registered Hub role. Pairing uses its
own ALPN, distinct from ordinary traffic:

| Boundary | Enrollment ALPN | Ordinary ALPN |
| --- | --- | --- |
| Hub enrolling at Gateway | `omochi/pair/1` | `omochi/gateway/4` |
| Client using a board link | None (bearer credential) | `omochi/dashboard/2` |
| Client enrolling as Hub owner | `omochi/owner-enroll/1` | `omochi/owner/1` |

Gateway invitations use `omochi://invite/<base64url-json>` or HTTPS `#g:` links;
owner/share links use `#owner:` and `#share:`. Identity enrollment secrets expire
after 180 seconds and bind the first redeeming authenticated identity. Successful
bound grants outlive invitation expiry. Sharing v2 uses reusable bearer credentials with independent interaction and
layout-editing permissions. All v1 sharing grants must be reissued. Enrollment success is persisted before it is acknowledged;
unknown outcomes and pending local saves need explicit recovery.

A remote owner can manage boards, presets, shares, connection diagnostics and
Gateway registration through the common
[`HubControl`](../crates/hub/src/control.rs) operations also used by local
handlers. Owner grant administration stays local. Gateway registration jobs are
bound to their initiating owner grant and client identity; a retained enrollment
proof permits save retry without redeeming a secret again. Shares permit only
their saved board's current membership. `interact` and `editLayout` are independent;
layout permission allows names/titles/geometry while membership/source edits
require owner authority.

Downstream owner/share sessions freeze a snapshot at a subscription cursor,
then stream updates with opaque Surface leases and canonical decimal sequence
strings. Clients acknowledge only after authoritative renderer application.
Actions/images require a current authorized lease. Board revisions, replacement,
revocation and connection loss invalidate subscriptions/leases as defined by the
contracts. Requests and actions are never replayed after loss.

Trust, owner and share stores serialize mutations across processes, re-read
under locks and use atomic durable replacement. Revocation is successful only
after every registered live participant reloads authorization and destroys old
sessions. Acknowledgement failure remains an error after durable removal;
stale caches must not restore deleted records. Native state is isolated under
`omochi/rust-v1`, with no migration of legacy identities, trust or dashboards.

Remote networking is opt-in. Online release builds select the production relay;
debug builds select n0 relays, and `--relay` overrides the relay set. Offline mode
uses direct/LAN paths without public lookup or relays. Relay admission is separate
from application authorization. [`crates/mdns`](../crates/mdns/README.md) uses
Bonjour on Apple, system DNS-SD on Windows, Avahi on supported Linux hosts, and
Rust mDNS otherwise. Discovery advertises public addressing data only.

The exact framing, limits and recovery rules are in
[Gateway v4](../spec/gateway-v4.md),
[dashboard sharing](../spec/dashboard-sharing-v2.md) and
[owner connection](../spec/owner-connection-v1.md).

## Boards and dashboard clients

[`dashboards.rs`](../crates/hub/src/dashboards.rs) persists version-1
`dashboards.json`. Each board has an ID, revision, name and at most 200 panels.
Geometry uses 12 columns with nonoverlapping panels, positive sizes and validated
source references. Updates and deletes require the current revision and return
409 on conflict. A corrupt store causes an error instead of an empty replacement.
Presets are composition-provided inputs, separate from saved board records.

The React dashboard accepts a live session and a
[`BoardRepository`](../packages/hub-ui/src/board-repository.ts). Local repositories
use HTTP; hosted repositories use downstream requests. Committed board state is
polled for invalidation without replacing editor drafts. Board export/import uses
`omochi.board` version 1, with a 1 MiB UTF-8 bound and strict source/geometry
validation. It transfers configuration and source labels, excluding credentials,
grant records and live application state. See the
[schema](../spec/board-export.schema.json) and
[codec](../packages/hub-ui/src/board-codec.ts).

[`packages/renderer`](../packages/renderer/src/session.ts) connects a `LiveSession`
to one authoritative A2UI processor sink. It applies source messages synchronously,
tracks leases and exposes snapshots for React observers. Observers do not apply
messages or acknowledge delivery independently. Closing a session stops its
source, retracts Surfaces and releases model/image state. Layout and media
overrides adapt the Basic Catalog while preserving A2UI actions and data paths.

The dashboard editor keeps a draft, detects conflicting saves and supports grid
drag/resize plus keyboard/button layout adjustments. Board view hides navigation
and editing while retaining live UI and scrolling; it is a display mode without
additional authority. Local and hosted dashboard shells share bundled Japanese
and English translations; app-supplied content remains application data. See
[dashboard UI](dashboard-ui.md) and the
[hub-ui guide](../packages/hub-ui/README.md).

Hosted connections use [`packages/hub-transport`](../packages/hub-transport/src/lib.rs)
for WASM iroh and `downstream-core` for session validation/apply state. Browser
identity is committed to IndexedDB before enrollment; one tab owns the endpoint
through Web Locks. Pending/accepted records support uncertain enrollment recovery.
Owner/identity invitation secrets are not persisted. Public-link reload context
uses the tab's sessionStorage. Forgetting a local profile does not revoke its Hub
grant. The static Worker supplies no API, WS or resource proxy fallback.

The Flutter app embeds the common Rust Hub through
[`omochi_hub.h`](../crates/native-bridge/include/omochi_hub.h), ABI version 2.
The host polls nonblocking commands/events and replies to storage requests until
close settles. Dart owns protected OS storage, lifecycle, GenUI input adaptation
and image decoding; Rust owns transport, enrollment, authorization, subscriptions
and image policy. A selected external owner/share connection suspends the
embedded Hub. External identity/storage uses a separate namespace; explicitly
saved public links include their bearer credential. Suspend/resume obtains fresh
snapshots and leases without resending uncertain actions. OS storage failures do
not regenerate identity or fall back to plaintext. Platform build/support and
capacity details live in the [mobile guide](../apps/mobile/README.md).

## Resources, image policy and capacity

[`resources/1`](../spec/resources.md) is separate control traffic on SDK
stdio/Unix/WebSocket connections. Registries copy immutable bytes into private
temporary storage and allocate opaque IDs. `stat` returns size/MIME/ETag;
`read` pulls at most 24 KiB. Gateway v4 carries one exchange per resource stream,
and local HTTP adapters retain HEAD, ETag and single-range semantics. Resource
frames/bytes never become A2UI snapshot state. Normal shutdown removes owned
temporary storage; abrupt termination may leave files.

Images have an additional subscription policy. Local browsers opt into
[`images/1`](../spec/hub-browser-images.md); hosted and native clients use scoped
downstream/native requests. A live Surface publication and handle authorize the
image. The common resolver validates public HTTPS:443 destinations, pins DNS,
revalidates redirects and checks encoded/decompressed raster bytes and dimensions.
SDK resources are read through the handle's service. Clients do not turn a failed
scoped image request into a direct URL fetch. Hosted downstream audio/video is
unavailable; local HTTP resource playback remains a separate capability.

The major bounds belong to different layers and must remain distinct:

| Layer | Bound / overload behavior |
| --- | --- |
| SDK action receive queues | 256 pending application messages or 4 MiB; overflow latches a receive error and ends the stream after the running handler |
| SDK Unix and TypeScript WS server output | 16 MiB outstanding encoded bytes per client, including active writes; overflow disconnects that client |
| Gateway wire / roster | 4 MiB per framed JSON message; at most 256 services and one frame's encoded roster capacity |
| Gateway snapshot / fanout | 16 MiB snapshot; fanout independently bounds message count and 16 MiB of encoded data; each materialized replay frame must also fit 4 MiB |
| Native action/publication queues | 8 MiB retained through processing |
| SDK resource registry defaults | 1 GiB per resource, 4 GiB total; reads at most 24576 bytes |
| Downstream listener | 16 connections; 8 in-flight request streams per connection |
| Scoped image pipeline | 8 MiB encoded/decompressed response bound, 4 M pixels per image; client image storage budget 32 MiB and two concurrent fetch/decode operations |
| Mobile embedded/external adapters | 2 Gateways / 8 services; these limits do not apply to hosted browser owner sessions |

SDK publisher bootstrap captures a snapshot and sends resource hello plus replay
before live updates/resource replies. Replay is encoded frame by frame, so its
total may exceed the per-client output budget. Each frame must fit with queued
live data; exhaustion disconnects instead of reordering/dropping frames. Native
Gateway replay additionally obeys its own snapshot/frame bounds. These limits
bound wire data or specific retained objects, not total process RSS. Detailed
timeouts, reservations and invalidation rules remain in the boundary contracts.

## CLI, utilities and composition

[`crates/cli/src/main.rs`](../crates/cli/src/main.rs) defines `dev`, `gateway`,
`hub`, `up`, `pair`, `id`, `trust`, `status` and `generate catalog`. `dev` supervises
one stdio application and exposes preview WS/resources. The preview's Vite proxy
defaults to `127.0.0.1:5555`; a `ws` query selects a direct endpoint. It displays
all Surfaces on its connection. The multi-service dashboard is a separate entry.

`up` loads version-1 `omochi.toml`, validates services and panel placement,
supervises child process groups and exposes a read-only preset. Command arrays
execute directly; the composition loader does not implicitly use a shell. It
sets service/runtime environment for discovery and terminates owned process
groups on shutdown. See
[`composition.rs`](../crates/cli/src/composition.rs).

[`crates/utils`](../crates/utils/src/main.rs) publishes utility Surfaces through
the same local discovery/ownership boundary. Modes cover a triggered command,
periodic command watch, managed process, stdin stream, file tail and HTTP monitor.
Its stdio MCP server exposes `publish_messages`, `list_surfaces` and `read_events`.
Events are cursor-based observations; publishing an action event does not execute
code by itself. The shared publisher validates ordered A2UI batches and maintains
replay state. The agent-approval example is an application built on these
boundaries, not a separate authorization mechanism in the Hub.

## Build, distribution and verification

[`package.json`](../package.json),
[`crates/Cargo.toml`](../crates/Cargo.toml) and the individual SDK manifests
define actual build dependencies and versions. `build:web` builds package assets;
`build` then builds native CLI/utils. `build:native` builds the required embedded
dashboard packages and native executables. Hosted WASM/dashboard and Flutter
builds are separate. Native distributions embed built dashboard assets so
installed Cargo packages need no Bun runtime/build step.

SDK/browser and native releases have separate version/check/package flows.
Cargo owns native versions; npm stubs use private `0.0.0` placeholders. Release
scripts generate public manifests and exact platform binary dependencies from
Cargo. The shared launcher selects a supported native binary and forwards the
process. Generated `dist`, `dist-test`, WASM and packaged assets are build output.
See [SDK/browser releases](releasing.md), [native releases](native-releasing.md),
[distribution](../crates/DISTRIBUTION.md) and [deployment](../deploy/README.md).

Changes should be verified at the affected boundary:

| Change | Verification entry points |
| --- | --- |
| Cross-language declaration/reconciliation/action behavior | SDK tests in each package plus `spec/fixtures`; `bun run test:python`, `bun run test:rust`, TypeScript package tests |
| Shared discovery ownership | `bun run test:sdk-ownership` and `spec/local-service-ownership.md` regressions |
| Native protocol, trust, Gateway and Hub | `bun run test:server-rust`; cross-process tests in `tests/native` through `bun run test` |
| MCP native client boundary | `bun run test:native-mcp` |
| React renderer and dashboard | Package tests/build/lint; native/browser scripts under `packages/hub-ui/test` and `tests/browser` |
| Hosted owner/share path | `bun run build:hosted`, `bun run test:dashboard-sharing`, `bun run check:dashboard-app` |
| Flutter/bridge | Flutter analysis/tests, C ABI checks and platform-specific validation from `apps/mobile` |
| Packaging | `bun run test:packaging`, SDK/native release checks and distribution smoke scripts |

`bun run test` builds packages/native binaries and runs package tests, native
workspace/client tests, approval, packaging and cross-publisher ownership checks.
Python SDK and Rust SDK suites have separate root scripts; the full language and
platform matrix is recorded in [CI workflows](../.github/workflows).
Real multicast, OS lifecycle, public-host and separate-network checks are
explicit environment-dependent verification. Dated
[sharing acceptance](dashboard-sharing-acceptance.md),
[mobile validation](../apps/mobile/docs/validation.md) and
[remote-client validation](../apps/mobile/docs/remote-connections-validation.md)
record evidence and remaining limits; a build or unit-test pass does not imply
those acceptance results.

## Experimental and historical boundaries

[`experiments/browser-dashboard`](../experiments/browser-dashboard/README.md)
contains the opt-in browser-resident Gateway aggregator, its WASM transport and
browser-local boards. It uses Gateway-wide trusted-Hub authority. Its
[browser Hub](../spec/browser-hub-v1.md) and
[bounded snapshot](../spec/browser-snapshot-v1.md) contracts describe that
implementation, separate from normal hosted/native-Hub sharing. The earlier
[`experiments/browser-hub`](../experiments/browser-hub/README.md) retains its own
probe and evidence.

The former TypeScript server is available in Git history, for example commit
`b8a2f85`, and is outside the active workspace. Historical diagrams, benchmarks,
review baselines and future-tense proposals retain their recorded revision's
meaning. The [documentation map](README.md) and
[specification index](../spec/README.md) identify current contracts and history.
