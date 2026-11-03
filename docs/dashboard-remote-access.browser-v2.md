# Browser hub, gateway access, and board sharing

Status: revised implementation proposal, updated 2026-11-03. Direct browser
connectivity is demonstrated by an isolated experiment; production integration
and board sharing are not implemented. EGM review remains interrupted.

## Problem and decision

Users need to open `app.omochi.fyi` or a self-hosted dashboard, connect to their
applications, and share the boards they arrange. A mandatory native hub server
would add another protocol and authorization boundary between the browser and
gateways that already speak iroh.

Move the gateway-consuming hub functions into the browser for remote access.
Share access at the gateway boundary using existing trusted-Hub enrollment.
Share board configuration separately as a document. A board reference is not an
access credential. Keep the native hub as an optional local/native deployment
with its current HTTP/WS behavior.

This replaces the earlier native-hub-sharing proposal. The
[original reviewed proposal](dashboard-remote-access.server-hub-v1.md) and
[review history](reviews/dashboard-remote-access/review-state.md) are retained;
their results do not establish approval of this revised architecture.

## Evidence and remaining limits

The [browser-to-gateway experiment](../experiments/browser-hub/RESULTS.md) passed
16 real-browser assertions on 2026-11-02. Chromium 156 connected over a public TLS
relay to the unchanged Rust gateway and a real SDK app, without linking or
starting a native hub. It exercised enrollment/rejection, exact gateway identity,
service discovery, snapshot rendering, an action and resulting update, SDK PNG
drawing, heartbeat, reconnect, reload with a persisted identity, and gateway
revocation. The static host received no application-data HTTP calls.

Browser and native gateway ran on one Linux host while their connection crossed
the public relay. This is not a test of separate physical networks, app.omochi.fyi
hosting, multiple gateways, mobile browsers, or production key storage. The
experiment uses disposable localStorage, a PNG-only adapter, and a button without
form input. Its WASM was 4,808,265 bytes, or 1,317,730 bytes with gzip.

The experiment completes single-gateway feasibility. Production session handling,
renderer integration, persistent storage, and board sharing remain to be built.

## Goals and non-goals

- Run remote gateway aggregation and A2UI rendering in an ordinary browser.
- Reuse gateway v4, pairing, generation, and SDK resource contracts.
- Enroll and revoke a browser at each gateway independently.
- Aggregate multiple gateways with independent reconnect and failure handling.
- Save boards locally and export/import configuration without credentials.
- Keep local hub HTTP/WS, SDK protocols, stdio preview, and native clients working.
- Serve remote UI assets without a mandatory application backend.

Per-Surface sharing permissions, read-only gateway roles, untrusted-hub delegation,
hub-to-hub routing, cloud board synchronization, collaborative editing, reusable
public invitation secrets, and a new dashboard ALPN are outside initial scope.
This work does not remove the native hub or migrate native dashboard files
automatically.

## Architecture and responsibilities

```text
Hosted or self-hosted static UI
  browser: board repository + renderer + gateway session manager
     +---- iroh WASM / relay ----> gateway A ---- Unix ----> SDK apps
     +---- iroh WASM / relay ----> gateway B ---- Unix ----> SDK apps

Existing local mode
  localhost UI ---- HTTP / WS / SSE ----> native hub ----> local/remote gateways
```

The web host distributes HTML, JavaScript, CSS, and WASM. The relay forwards
encrypted traffic. Native gateways remain beside apps for discovery and Unix
access. The browser's hub functions are client-side aggregation, session handling,
routing, and bounded rendering state, not a new server.

| Component | Responsibility |
| --- | --- |
| SDK app | Own Surface lifetime and business state, handle actions, register resource bytes |
| Gateway | Discover services, expose v4 streams, authenticate/authorize enrolled Hub identities, enforce revocation |
| Browser gateway session manager | Connect independently, track services/generations, materialize state, route actions/resources, cancel stale work |
| Dashboard UI | Arrange views, create/delete board documents, present connection and import/export workflows |
| Board repository | Store layouts and local revisions; never grant source access |
| Optional native hub | Existing local WS/Unix integration, native clients, offline operation and native image policy |

Use a live-Surface session interface for the renderer. A direct-gateway adapter
uses WASM streams; a local-hub adapter uses existing WS/HTTP/SSE routes. Board
storage has a separate repository interface. Do not put board CRUD into the
gateway protocol or model it as a Surface permission.

Extract browser-compatible transport/session code. The current Rust hub includes
Unix, native runtime, and image-fetch dependencies; its Cargo `browser` feature
enables a native HTTP server, not a WASM target. The experiment proves a small
iroh wrapper works, not that the full hub crate ports. Choose shared package
boundaries so renderer does not depend on dashboard UI. npm native launcher
stubs remain launchers.

## Gateway access and trust

The browser has a durable iroh identity and enrolls as the existing Hub role.
The gateway checks the authenticated EndpointId before application streams.
Authorization is gateway-wide in this release: an enrolled browser can use that
gateway's exposed services, actions, and resources. Displaying one Surface does
not narrow that authority. Explain this scope when issuing and accepting an
invitation; do not present the connection as read-only or one-board access.

There is no new browser-client role, per-board grant, per-Surface publication
allowlist, or downstream Surface access manager. Lifetime-bound handles,
generation checks, frame validation, service-scoped resource routing, and
cancellation remain necessary to prevent stale operations.

Each gateway is its own authorization authority. Enrollment on A does not
authorize B. Revocation on A closes its access, including pending reads and
actions not yet dispatched, without withdrawing B's independent access. Keep
existing durable-store and live-process acknowledgement rules; a failed
acknowledgement remains an error and never restores removed trust. Delivered
bytes and executed actions cannot be recalled.

Apps retain responsibility for business-level permissions. Access to an action
does not sandbox its effects. The hosted origin and its scripts are trusted
client software because they handle keys and app data. End-to-end encryption
does not protect against malicious client code.

## Enrollment, links, and browser identity

Reuse `omochi://invite/<hex-json>`, version 4, its existing 16 KiB bound and
180-second lifetime. Use `omochi/pair/1` for redemption and `omochi/gateway/4` for
ordinary traffic. The first authenticated identity wins; it may retry within
the invitation lifetime, and another identity cannot reuse the secret.

A hosted bootstrap URL uses `/#connect:<encoded-payload>` containing the native
gateway invitation. Bound the wrapper before decoding, validate the invitation,
and derive the intended gateway ID/address from it. The wrapper is navigation,
not a new authority or pairing protocol. Specify its schema and size limit before
implementation. Board references, names, relay URLs, and discovery never grant
trust.

Capture the fragment before third-party scripts and remove it from history.
Never persist or export its secret. Persist the browser identity before
redemption and the authenticated gateway record after acceptance, before ordinary
traffic. An uncertain result retains the same identity and offers a retry rather
than silently generating a replacement key.

Use an origin-local IndexedDB profile for identity material, accepted gateways,
and boards, logically separated in storage. Wait for transaction completion
before reporting success. The WASM key is exportable to app code; it is not a
non-exportable credential. Define recovery for denied, corrupt, or cleared
storage. A different origin has its own identity and needs enrollment; do not
transfer trust automatically.

Initially permit one active endpoint owner per origin/profile. Acquire a browser
lock before opening it; another tab reports the active owner instead of opening
another endpoint with the same key. Release ownership on shutdown and recover
after a crash. Shared-worker multi-tab operation is deferred. Validate lock and
storage support on the declared browser matrix before release.

Gateway owners continue to issue invitations, list peers, and forget identities
through native administration. Removing a local connection record is separate
from revoking it on the gateway; UI copy must explain the distinction.

## Live sessions, routing, and resources

Use the v4 control stream for identity, service lists, and matching heartbeat
replies. Use service streams for snapshots, updates, and actions, and independent
resource streams for `resources/1`. Preserve 4 MiB frame and existing snapshot/
queue limits, valid service IDs, and stale-generation rejection. Specify aggregate
browser limits in addition to per-stream limits.

Namespace Surfaces as `<gatewayId>~<serviceId>/<surfaceId>`. Retain current
`SurfaceReference` fields `nodeId`, `serviceId`, and `surfaceId`; `nodeId` is the
source gateway. Verify the authenticated gateway ID and control hello. Unwrap
actions only for the captured service/generation and narrow client model metadata
before forwarding. Support form draft data; the experiment's button has no form
input and does not establish this behavior.

Session loss retracts only that gateway's state and cancels dependent resource
reads/bitmaps. Reconnection requires fresh snapshots before actions resume and
never replays actions. Discard a stream on partial-frame failure or timeout; do
not leave a read running while starting another on the same stream. Use bounded
backoff with jitter and recover after browser suspension. A quiet but healthy
service stream is not a timeout merely because it produces no updates.

The first direct-browser release supports SDK-registered PNGs through resource
stat/read streams, with byte, signature, dimension, pixel, concurrency, and
decoded-memory checks before drawing. Reuse renderer lifecycle/decode budgets
through an adapter. IDs resolve within the captured source service, not paths,
cross-service addresses, or arbitrary fetch targets. Never resolve native
`/images` or `/resources` URLs against the static host. Other raster formats need
explicit validation support.

External HTTPS images are unsupported in initial direct mode and show a clear
unavailable state. The native resolver's DNS-pinning and redirect/fetch policy
cannot be assumed to work in a browser. Do not fall back to unrestricted image
tags or silently add a gateway proxy. Native mode keeps its existing policy.
If external images become a release requirement, specify a source-side resolver
or explicit native adapter as separate work. Audio/video and generic HTTP
Range/HEAD adapters are outside the first browser release; preserve resource
framing and chunk offsets for future adapters.

## Board storage and sharing

Direct-mode boards live in the origin-local repository. Creating, editing, or
deleting one needs no gateway permission. Panels store source references and
geometry, not live state or credentials. Native local mode keeps its HTTP-backed
`DashboardStore`, revision conflicts, and presets. No native file is migrated
automatically; explicit export/import bridges repositories and origins.

Initial sharing is a versioned JSON export/import of one board. Reuse board
name, panels, Surface references, and geometry validation. Include a schema
version and optional source display names. Exclude local IDs/revisions, private
keys, invitation secrets, gateway trust records, live data models, and resources.

Validate the version, bounds, IDs, references, and layout before creating a new
local ID/revision. Keep at most 200 panels and a 1 MiB input limit; specify exact
text and geometry constraints in a shared contract. Reject unsupported fields,
render names as text, and never interpret an import as endpoint configuration,
executable catalogs, HTML, or network requests.

An imported board lists missing source connections. It never enrolls a gateway,
silently selects an endpoint, or sends an action. Recipients connect/enroll
separately. An unavailable source leaves its panels unavailable while other
panels work. Board selection is a presentation choice, not a permission boundary.

File sharing transfers a copy. Sender edits/deletion do not change that copy or
revoke source access. Deleting a board, forgetting a local source profile, and
revoking a gateway peer are independent operations. Public short links, shared
live revisions, URL-encoded boards, and cloud synchronization are deferred.

## Local mode and deployment

Keep the localhost UI/native-hub route for Unix discovery, offline use, and native
consumers. Preserve loopback bindings and HTTP/WS Origin checks. Hosted HTTPS to
loopback WS needs separate browser-policy validation; do not assume it works.
Direct mode uses gateway iroh and never falls back to the static host's `/api` or
another source/hub on failure.

Build the same dashboard UI for native embedded and static hosted assets with
explicit connection/repository adapters. Load WASM only for direct connections.
The hosted entry point shows saved profiles or a connect workflow rather than
assuming a same-origin hub API.

Browser iroh currently needs a relay; local native mode supplies offline use.
Choose production relay capacity and hosting separately. The experiment's public
relay establishes connectivity, not a production availability or billing promise.
Existing deployment templates remain templates. Verify CSP, WASM compression,
WSS access, base paths, and origin trust before publishing hosted assets.

## Alternatives and trade-offs

| Alternative | Benefit | Decision |
| --- | --- | --- |
| Mandatory native hub | Reuses native aggregation/media policy | Optional; direct access is demonstrated and avoids a listener, enrollment boundary, and wire contract |
| Per-Surface/per-board grants | Narrows authority within one gateway | Outside scope; gateway-wide trusted access is the selected sharing unit |
| Port the entire Rust hub to WASM | Shares more native code | Extract reusable session/protocol pieces around a small wrapper, avoiding native Unix/HTTP/image dependencies |
| Cloud board backend | Synchronizes editable boards | Deferred; local documents plus export/import satisfy initial composition sharing |
| Required native extension/proxy | Supplies native discovery/networking | Keep local native mode optional; remote viewers use the browser path |
| WebRTC or reverse tunnel | Alternative reachability | Revisit if browser/relay requirements fail; unnecessary for the demonstrated path |

Trade-offs are broad gateway trust, browser-held keys, relay dependence, WASM
download cost, and reduced initial media coverage. These are explicit scope
choices, not claims of full native feature parity.

## Release gates

1. Specify browser storage, enrollment recovery, one-owner locking, and behavior
   when required browser facilities or durable storage are unavailable.
2. Specify package/API boundaries, strict message validation, atomic snapshot
   transitions, and numerical aggregate browser capacity limits.
3. Specify board export/import schema, repository transactions, and shared
   validation fixtures, with no source authority in board documents.
4. Validate simultaneous sources, form metadata, identity separation, service
   replacement, revocation, reconnect, cancellation, load, and suspension.
5. Declare browser/media support. Only the recorded Chromium desktop case is
   established; initial direct media is SDK-backed PNG.
6. Select and verify production relay and actual hosted HTTPS delivery, CSP,
   compression, operational limits/metrics, and separate-network connectivity.

## Detailed Implementation Plan

### M0. Direct-gateway feasibility — completed within recorded scope

Retain the isolated experiment and evidence under `experiments/browser-hub`.
Its 16 assertions establish the single-source path, not production completeness.
Promote its real-browser harness/fixture patterns into integration checks while
keeping the experiment as inspectable evidence. Do not mark its untested cases
complete or repeat the spike in place of production work.

### M1. Production sessions and transport adapters

Define a small WASM transport and gateway session manager without desktop-only
dependencies. Pin iroh and the matching binding toolchain. Resolve gate 2 before
adopting spike code. Reuse v4 framing/types or shared conformance fixtures; do not
create `omochi/dashboard/1` or carry board controls in SDK messages.

Implement validated streams, heartbeat, snapshot cutover, generation guards,
capacity limits, cancellation and backoff. Aggregate two gateways independently;
test colliding service/Surface names and failure of one source while the other
remains usable. Add the live session interface and local-hub adapter without
changing local routes.

### M2. Durable enrollment and connection workflows

Resolve gate 1. Implement IndexedDB transactions, profiles, one endpoint owner,
strict bootstrap import, and uncertain-result recovery. Persist identity before
redemption and accepted peer records before traffic. Expose disconnected,
connecting, enrolled, reconnecting, revoked/unavailable, storage-error, and
another-tab-active states with recovery actions.

Reuse native gateway invitation/forget administration. Add web-link wrapping and
scope explanations to invitation output/help if needed without changing its
authority or advertising bearer secrets. Test expired/consumed invitations,
competing identities, lost replies, cleared storage, and different origins.

### M3. Renderer actions and SDK-resource PNGs

Replace experiment assumptions with the session adapter. Preserve namespace
translation, form metadata narrowing, image cancellation, and pixel/memory
limits. Add validated PNG reads and explicit unsupported-media behavior. Verify
late actions/images cannot affect replacement service/Surface generations.
Retain native image policy.

### M4. Browser board persistence and JSON sharing

Resolve gate 3. Separate board CRUD from connections in the UI and add browser/
local-HTTP repositories. Implement atomic saves, revision conflicts, export,
bounded import-as-copy, and missing-gateway panels. Preserve local boards and
presets. Test that importing cannot enroll sources or expand access, and deleting
a board cannot revoke peers.

### M5. Compatibility, capacity, and hosted delivery

Resolve gates 4–6 on declared browsers and the hosted build. Exercise multiple
gateways/tabs, background/sleep, gateway/service/relay restarts, slow readers,
oversized frames/images, malformed rosters, partial reads, revocation during
transfers, and repeated reconnects. Test separate networks and record passing
browser/media combinations. Verify static hosting has no app-data API dependency
or credential-bearing assets/logs. Publication is a separate operational step
after implementation and checks are reviewable.

### Acceptance checks and planned commands

- Direct: connect to two gateways, authenticate each, render fresh state, forward
  actions with form input, and read PNGs over iroh without a native hub/API proxy.
- Sessions: reconnect/revoke A while B works; stale generations, streams, and
  late image results cannot change replacement state.
- Trust: unknown/forgotten peers get no ordinary control data; gateway-wide access
  is explicit and independent of board selection.
- Boards: layout export/import excludes keys, invitations, trust, and live state;
  invalid input fails safely, and recipients connect separately.
- Local: retain HTTP/WS/SSE, Origin policy, boards, presets, stdio preview,
  native clients, and offline contracts.
- Capacity/recovery: bounded behavior under blocked reads/malformed input;
  discard interrupted streams and never replay actions.

Planned checks include `bun run --filter @omochi-ui/hub-ui test`,
`bun run --filter @omochi-ui/hub-ui lint`,
`bun run --filter @omochi-ui/renderer test`,
`cargo test --manifest-path crates/Cargo.toml --workspace --locked`,
`bun packages/hub-ui/test/browser.mjs`, new browser/protocol/repository fixtures,
and direct-gateway browser integration. These are future production checks; the
completed experiment is the evidence linked above.

This revision needs a fresh architecture review when review is explicitly
resumed. Historical per-Surface/hub-CRUD findings are not requirements for this
gateway-sharing scope and are not reported as fixed because the scope changed.
