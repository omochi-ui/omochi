# Browser hub, gateway access, and board sharing

Status: implementation retained as the opt-in
[browser dashboard experiment](../experiments/browser-dashboard/README.md).
Browser integration, persistent profiles, PNG resources and board-file sharing
were implemented, with local acceptance recorded on 2026-11-03. The browser hub,
WASM transport, dedicated entries, harness and delivery notes now live under
`experiments/browser-dashboard/`; the normal dashboard remains a native-hub
client. The plan and original file map below describe the implementation before
that relocation, not the selected design for future readonly URL sharing.
See the [operator instructions](../experiments/browser-dashboard/hosted/README.md)
and [acceptance record](../experiments/browser-dashboard/hosted/acceptance.md) for
historical verification and rollout limits. Separate-network/public-host
acceptance remains unperformed.
The normative API, state, storage, media, build and
test decisions are in [browser-hub v1](../spec/browser-hub-v1.md); read it with
the implementation plan and [bounded snapshot contract](../spec/browser-snapshot-v1.md).
The [board JSON Schema](../spec/board-export.schema.json)
and [fixtures](../spec/fixtures/board-export.json) are shared conformance inputs
for the implemented runtime codec.

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

The experiment completes single-gateway feasibility. The later two-gateway
implementation and its local acceptance are recorded separately in
[acceptance record](../experiments/browser-dashboard/hosted/acceptance.md); the original experiment remains
unchanged as historical evidence.

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
iroh wrapper works, not that the full hub crate ports. Create the two private
browser-transport/browser-hub packages and core/browser entry point specified
in the contract. Renderer owns its processor/catalogs and never imports hub-ui.
npm native launcher stubs remain launchers.

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

A hosted bootstrap URL uses `/#connect:<base64url-json>` with exactly
`{version:1,invitation:<native-invite>}` and the contract's 32 KiB fragment bound.
Validate its native invitation and derive the intended ID/address from it.
The wrapper is navigation, not a new authority. Board references, names, relay
URLs and discovery never grant trust. The contract fixes relay handling,
pending/accepted storage records and recovery for unknown or unsaved enrollment.

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
stat/read streams, with byte, CRC/IHDR/IDAT validation, dimension, pixel,
concurrency and decoded-memory checks before drawing. Initial supported PNGs
are non-interlaced 8-bit RGB/RGBA; unsupported variants show unavailable rather
than weakening the contract. Reuse renderer lifecycle/decode budgets
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

Initial sharing is a versioned JSON export/import of one board, using the linked
board schema and strict semantic rules in browser-hub v1. Source display names
are advisory import-preview text, not persistent source configuration.
Exclude board IDs/revisions/timestamps, private
keys, invitation secrets, gateway trust records, live data models, and resources.

Validate the version, bounds, IDs, references, and layout before creating a new
local ID/revision. Keep at most 200 panels and a 1 MiB input limit, applying the
contract's exact UTF-16, duplicate and geometry rules. Reject unsupported fields,
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

## Release acceptance gates

The choices previously listed as missing release specifications are fixed by
browser-hub v1. Remaining gates verify implementation of those choices:

1. Verify IndexedDB records/transactions, Web Lock ownership, and the specified
   pending/accepted/unknown/unsaved enrollment recovery paths.
2. Verify strict v4 parsing, decimal u64 bridge values, atomic snapshots,
   immutable leases, stream cancellation and every numerical budget.
3. Verify board schema/semantics and both repository adapters without imported
   authority or changes to native persistence contracts.
4. Verify simultaneous sources, form metadata, generation replacement, queued
   gateway-action revocation, reconnect, cancellation and load/suspension recovery.
5. Pass desktop Linux Chromium 156+ facility/media tests. Other browsers remain
   unsupported until added by a separate tested change.
6. Verify the static delivery/header contract over HTTPS and the public-relay
   preview path. Dedicated-relay SLO/public publication remain separate operator
   rollout steps; no cloud-host selection blocks implementing the static artifact.

## Detailed Implementation Plan

### M0. Direct-gateway feasibility — completed within recorded scope

Retain the isolated experiment and evidence under `experiments/browser-hub`.
Its 16 assertions establish the single-source path, not production completeness.
Promote its real-browser harness/fixture patterns into integration checks while
keeping the experiment as inspectable evidence. Do not mark its untested cases
complete or repeat the spike in place of production work.

### Implementation file map

All Create entries below are proposed production files, not claims that files
already exist. There are no Delete operations. Contract/schema/board fixture
files linked above are already present as design artifacts. The prior reviewed
milestone-only plan is archived as
[browser v2](dashboard-remote-access.browser-v2.md). The exact previous detailed
plans are [browser v3](dashboard-remote-access.browser-v3.md) and
[browser v4](dashboard-remote-access.browser-v4.md); the latest author
request explicitly authorizes this additional fix/review beyond the earlier cap.

| Phase | Action | Files | Exact responsibility |
| --- | --- | --- | --- |
| M1 | Create | `packages/browser-transport/Cargo.toml`, `Cargo.lock`, `.gitignore`, `package.json`, `tsconfig.json` | Private standalone WASM workspace and TS-only default build; contract pins/dependencies |
| M1 | Create | `packages/browser-transport/src/lib.rs`, `index.ts`, `ffi.ts` | Framing, decimal-u64 DTOs, pure existing-secret identity derivation, cancellable streams and lazy assets; gloo-timers frame deadline |
| M1 | Create | `scripts/build-browser-transport.ts` | Locked cargo/bindgen generation; check target and exact CLI version with actionable setup errors |
| M1 | Create | `packages/core/src/browser.ts`, `browser-session.ts`, `gateway-v4.ts`, `data-path.ts`, `data-patch.ts`, `json-size.ts` | Browser-safe exports, LiveSession/RenderBudget/result types, strict DTO/path validation and shared pure JSON sizing/patch plans |
| M1 | Modify | `packages/core/src/service-id.ts`, `prefix.ts`, `packages/core/package.json` | 64-hex/129-char routing, metadata narrowing and `./browser` export; preserve bare service channels |
| M1 | Create | `packages/browser-hub/package.json`, `tsconfig.json`, `src/index.ts`, `session.ts`, `scopes.ts`, `budget.ts` | Private TS manager, authoritative renderer sink/results, scoped Retry, immutable leases and numeric budgets |
| M1 | Create | `packages/browser-hub/src/snapshot.ts`, `packages/browser-hub/test/snapshot.test.ts`, `packages/core/test/data-patch.test.ts`, `packages/core/test/json-size.test.ts` | Browser-only bounded snapshot API, virtual preflight, exact replay sizing, reservation/rollback and detached payloads per browser-snapshot-v1 |
| M1 | Create | `packages/renderer/src/session.ts`, `local-source.ts`, `messages.ts`, `native-images.ts`, `data-patch.ts`, `packages/renderer/test/data-patch.test.ts` | Renderer/session/WS ownership, native images and direct shared-plan/root-update boundary, missing-root initialization and draft/parity/budget tests |
| M1 | Modify | `packages/renderer/src/client.ts`, `surface.tsx`, `images.ts`, `index.ts`, `resources.tsx`, `packages/renderer/package.json` | Authoritative apply result/partial cleanup, compatible wrappers, direct unavailable catalog, loader overload, public APIs and public core dependency |
| M1 | Create | `crates/gateway/src/dispatch.rs` | Remote epoch/session provenance and dispatch barrier |
| M1 | Modify | `crates/gateway/src/lib.rs`, `wire.rs`, `remote.rs` | Carry origins through Action/Resource queues, Local Unix wrapper, cancellation before dispatch and barrier before ACK |
| M1 | Create | `spec/fixtures/browser-gateway-v4.json` | Framing/control/u64/path conformance cases shared by Rust/TS; raw wire JSON is a string to preserve u64 precision |
| M1 | Create/Modify | `packages/browser-transport/test/*`, `packages/browser-hub/test/session.test.ts`, `packages/core/test/prefix.test.ts`, `gateway-v4.test.ts` | FFI disposal, routes/metadata, state/budget/fake-clock regressions |
| M1 | Create | `crates/gateway/examples/browser_fixture.rs`, `tests/browser/browser-hub.mjs`, `cdp.mjs`, `fixtures.mjs`, `static-host.mjs` | Real two-gateway/SDK fixture, explicit fault gates, harness modes, static host and cleanup |
| M1 | Modify | `crates/gateway/Cargo.toml`, `crates/Cargo.lock`, root `package.json`, `bun.lock` | Fixture-only TLS dependencies, new private workspaces and build/test scripts; native workspace gets no WASM crate |
| M1 | Modify | `scripts/release.ts`, `release.test.ts`, `package-native.py`, `test_native_packaging.py` | Public SDK version selector excludes private/native inputs, preserve dependency guards, core-first native packaging and regression checks |
| M1 | Create | `scripts/test-renderer-package.ts` | Isolated local-tarball declaration consumer with declared public dependencies; no publication |
| M1 | Create | `packages/hub-ui/browser-test/index.html`, `packages/hub-ui/src/main.browser-test.tsx`, `SessionGallery.tsx`, `packages/hub-ui/vite.browser-test.config.ts`, `tsconfig.browser-test.json`, `deploy/hosted/headers.json` | Test-only memory-profile real browser entry/build; shares production session/renderer and static header contract |
| M1 | Modify | `packages/hub-ui/package.json`, `tsconfig.app.json` | Private build-input devDependencies, browser-test build alias and local-only typecheck excludes |
| M2 | Create | `packages/browser-hub/src/storage.ts`, `profile.ts`, `bootstrap.ts` | Exact DB schemas, complete profile/recovery APIs, explicit refresh/redemption, separate temporary/live relay sets, saved-proof retry, lock and strict link parser |
| M2 | Create | `packages/hub-ui/src/ConnectionScreen.tsx`, `packages/hub-ui/src/main.hosted.tsx`, `packages/hub-ui/hosted/index.html`, `packages/hub-ui/vite.hosted.config.ts`, `packages/hub-ui/tsconfig.hosted.json` | Connection/gallery preview with live sessions, no board CRUD until M4; hosted-only entry |
| M2 | Modify | `packages/hub-ui/package.json`, root `package.json` | Hosted build/test aliases and hosted-only typecheck |
| M2 | Modify | `crates/cli/src/main.rs`, `docs/cli.md` | Gateway invitation output includes optional `--web-url` wrapper; flag is an explicit configured HTTPS UI base, default unset; no new pairing wire format |
| M2 | Create | `packages/browser-hub/test/profile.test.ts`, `bootstrap.test.ts` | Transaction/owner/parser unit cases plus CDP profile-mode failures |
| M3 | Create | `packages/browser-hub/src/png.ts`, `resource.ts`, `test/png.test.ts`, `resource.test.ts` | Scoped stat/read validation, bounded PNG validation/inflation, cancellation and responses |
| M3 | Modify | `packages/renderer/src/resources.tsx`, `image_view.tsx`, `images.ts` | Catalog factory with explicit direct-mode media unavailability and retained native behavior |
| M3 | Modify | `packages/hub-ui/src/ConnectionScreen.tsx`, `tests/browser/browser-hub.mjs`, fixture source | Preview image and real canvas/resource/abort tests |
| M4 | Create | `packages/hub-ui/src/board-codec.ts`, `board-repository.ts`, `browser-boards.ts`, `http-boards.ts`, `environment.ts` | Exact exchange codec and repository interfaces/transactions; environment supplies session/repository |
| M4 | Modify | `packages/hub-ui/src/App.tsx`, `main.tsx`, `main.hosted.tsx`, `contracts.ts`, `services.ts` | Inject adapters, remove direct API/SSE ownership from App, add import/export in both modes; main.tsx stays local-only |
| M4 | Create | `packages/hub-ui/test/board-codec.test.ts`, `board-repository.test.ts` | Shared schema/semantic fixtures, CAS/delete/quota and cross-repository tests |
| M4 | Modify | `crates/hub/src/dashboards.rs` tests only | Native valid/invalid geometry fixture parity, without changing stored schema or serde unknown-field behavior |
| M5 | Create | `deploy/hosted/README.md` | Operator instructions for the M1 header/artifact contract; no provider provisioning |
| M5 | Modify | `tests/browser/*`, browser fixture, `scripts/package-crates.py` tests, `docs/cli.md` | Hosted TLS/header/assets, fault/load tests, native no-WASM packaging and operator acceptance documentation |

For every multi-file cell, each bare filename after a fully qualified path shares
that directory; `src/` and `test/` prefixes reset relative to the named package.
The test-directory wildcard is limited to `framing.test.ts`, `cancel.test.ts` and
`fixtures.ts`. It does not delegate selection of additional production files.
Fixture devDependencies added to gateway are iroh-relay `=1.4.1` with server,
rustls 0.23 with ring/std and no defaults, rustls-pemfile `=2.2.0`, and tempfile 3.

### Build and harness commands to add

The browser-only bounded-snapshot algorithm is specified in
[browser snapshot v1](../spec/browser-snapshot-v1.md), required alongside the hub
contract. Add test:renderer-package (build:web then its isolated consumer script)
and require release:check after private workspace creation.

New package default build commands are `tsc -p tsconfig.json`. browser-hub test is
`bun test ./test`; browser-transport test runs its pure native Rust lib tests,
then `bun test ./test`. browser-transport also has `build:wasm` invoking the root build
script. Root build:web remains ordinary workspace type/JS builds and never calls
build:wasm. Add root `build:hosted` in this order: build:web, browser-transport
build:wasm, then hub-ui build:hosted. The latter runs its hosted Vite build and
copies generated/ intact to dist-hosted/iroh. Root build:browser-test uses
build:web, transport build:wasm, then the hub-ui
browser-test build, typechecking its dedicated config. Session mode serves
dist-browser-test; production modes serve dist-hosted. No normal native command requires
wasm32 or bindgen. Native root/package-native.py builds run core before renderer
and hub-ui; their local typecheck excludes both browser entry graphs.
The build script accepts WASM_BINDGEN for an existing binary,
requires CLI 0.2.130 and provides exact rustup/cargo-install setup instructions.

Add `test:browser-session`: build:native, build:browser-test, locked cargo build
of the browser_fixture example, then the harness with --mode session.
Add `test:browser-hub`: build:native, build:browser-test, build:hosted,
locked cargo build of
`omochi-gateway --example browser_fixture`, then
`bun tests/browser/browser-hub.mjs --mode all`. The harness accepts
`--mode session|profile|media|boards|hosted|all`, plus `--public-relay` for the
unmodified real relay smoke; its default uses a controlled TLS relay and faults.
CHROMIUM_BIN selects the executable. Results/screenshots go to a selected private
temporary directory unless BROWSER_HUB_RESULTS names a preserved output path.

The browser-transport Rust tests run with
`cargo test --manifest-path packages/browser-transport/Cargo.toml --locked --lib`
for pure framing/normalization logic under cfg(test) native-compatible modules;
WASM lifecycle behavior is tested by the actual CDP harness. The package default
test includes those pure cases through its test script, not a nonexistent native
build of the browser-only endpoint. Put platform-specific endpoints behind
cfg(target_arch = "wasm32") while sharing wire code without native networking.

### M1. Sessions, routing, gateway dispatch, and native compatibility

Depends on the contract, existing gateway v4 and experiment, not on persistent
browser enrollment. Create the transport/core/session files and refactor renderer
APIs first, keeping the existing local main.tsx/useA2uiSession functional. Implement
the contract's state, budget, error and u64 rules; source APIs are not selected by
the implementer. Correct core ID validators and existing prefix fixtures. Then
add gateway command provenance/dispatch barrier and its blocked-SDK regression.

The M1 browser-test HTML/entry/Vite/TS config and SessionGallery are created in
this phase. The test entry acquires its isolated Web Lock, pairs its memory-only
identity using real fixture invitations, closes the enrollment endpoint, then
constructs production sessions/renderer. No test identity touches user storage.
Direct sessions are accessible only to this separately built test consumer at
this phase; no M2 file or old experiment build is a prerequisite. Production native UI remains the default and buildable. Direct image
calls return UnsupportedMedia until M3, and no unfinished board feature is exposed.

Checkpoint: core/browser/renderer unit suites, native gateway dispatch tests,
build:native, the existing hub-ui native browser test, and
`bun run test:browser-session` (or its direct harness command after all builds).
Also run release:check, test:renderer-package and packaging regressions. Check two source namespaces,
max-u64 generations, atomic snapshots, full form metadata, deadlines/cancellation,
revoke-with-blocked-SDK, authoritative apply failure/partial cleanup, bounded
snapshot pre-copy accounting, actual-processor array/primitive patch equivalence,
unrelated draft preservation, scratch admission, native bypass, Retry without interrupting B, silent-first-frame
and partial-frame timeouts with healthy heartbeat, and A loss while B remains live.
Clean-output native builds and native packaging must work without private package
dist/generated assets or WASM tooling, building core before renderer/hub-ui.

### M2. Durable profile and hosted connection preview

Depends on M1 transport lifetime APIs. Implement the DB/profile/bootstrap contract
exactly, including pure key derivation before sockets, complete ProfileView and
ProfileRecovery APIs, local-only save retry/reset and explicit connectSelected,
main-thread Web Lock ownership and temporary pairing endpoint ordering. Implement refreshInvitation, retryInvitationRefresh and redeemInvitation,
with temporary endpoints using invitation-only relays and live endpoints using
selected-record union. Add hosted ConnectionScreen and gallery; the hosted entry remains a
connection/view preview until M4, while native App is unchanged. Add the explicit
gateway --web-url output wrapper without emitting secrets via normal discovery.
Keep M1 browser-test entry separate; production hosted typechecks tsconfig.hosted.json.

Checkpoint: profile/bootstrap unit cases, build:hosted, `--mode profile` and
`--mode session`. Test blocked/denied storage, corrupted identity, owner crash,
competing tabs/identities, lost acceptance, commit failure, reload pending state,
expired invite and same-key new-invite recovery, public-API reachability of all
recovery states, local save retry after expiry with zero network calls, and
identity reset retaining boards, accepted-ID refresh with no pair request, explicit
redemption after NotAuthorized, and eight selected plus four disjoint temporary
relay hints without truncation. Normal traffic must remain
absent until accepted peer persistence commits.

### M3. PNG transport and media capability policy

Depends on M1 scopes and renderer factory; hosted real-user flow also uses M2.
Implement resource/PNG algorithms and direct catalog overrides. Keep the native
catalog and applied-ACK behavior unchanged. Gallery now displays supported SDK
PNGs and accessible unavailable states for every unsupported media path.

Checkpoint: PNG/resource/renderer unit suites, `--mode media`, native browser
compatibility and session/profile regression modes. Exercise size/dimension/
inflation/CRC limits, canonical chunks, stat/read/decode abort, stale leases,
same-ID recreation and zero external/HTTP media requests in direct mode.

### M4. Repositories, complete dashboard UI, and file sharing

Depends on M2 storage; codec/schema tests can be implemented independently after
M1. Implement both repositories and shared strict exchange codec. Inject them
through DashboardEnvironment `{session:LiveSession, boards:BoardRepository}`;
native entry creates local source/HTTP repository, hosted entry creates profile
source/browser repository. The hosted entry now mounts the full shared App.
Connection changes replace environment.session only after closing its old renderer
session; board changes merely call setServices and never select/pair new gateways.

Use sources/presets from the adapters rather than App's direct fetch/EventSource.
Expose export and import-as-copy in both native/hosted UI, with preview labels and
missing-source panels. Preserve current revision conflict and unsaved-draft UX.

Checkpoint: board fixtures against JSON Schema and semantic codec, repository
CAS/quota/abort tests, `--mode boards`, session/profile/media regression, hub-ui
lint/typecheck/unit and native browser integration. Import/export must round-trip
between native and browser modes without importing identity, trust or board IDs; panel/reference IDs remain.

### M5. Hosted preview distribution and fixed acceptance matrix

Depends on M1–M4. Complete operator instructions and verify the static header contract
with the existing HTTPS test host. Run `test:browser-hub`, existing native integration and
`bun run test:packaging`. Verify desktop Chromium 156+ facilities, nested-base
asset URLs, WASM MIME, prescribed CSP, no API proxy, no Node imports, and that
local Cargo/npm artifacts do not include or require hosted/WASM inputs.

Run the contract's 10-minute two-gateway/eight-service load, budget-zero-on-stop,
90-second controlled-relay recovery and fault matrix. Document exact versions
and outputs. Prepare the static preview directory and operator instructions;
do not publish it or claim an SLO. A separate-network/public-host acceptance run
is required before deployment claims, but does not change the coding contracts.

### Re-review and traceability

The previous 17 raw findings map to the contract as follows: V2-01 package/API/build
to sections 1–2/9 and the file map; V2-02 session/budgets to sections 3–4/10;
V2-03 legacy IDs to section 3; V2-04 storage to section 5; V2-05 media to section 6;
V2-06 boards to section 7/schema/fixtures; V2-07 acceptance to sections 9–10 and
phase checkpoints; V2-08 queued revocation to section 8. Edits are requested
resolutions, not verified fixes until the fresh re-audit completes.

The applied repair batch covers V3-01..09 and all three Nits: authoritative
apply feedback, complete profile recovery, scoped Retry, frame/open deadlines,
M1 test entry, release selector, public core/native build order, pure key derivation
and bounded snapshot API/algorithm. Historical review targets/hashes remain archived. The latest repair batch adds
explicit accepted-link refresh/redemption and relay sets, a shared pure data-patch
planner plus direct renderer root normalization, and both v4 editorial corrections.
The normalization initializes new direct Surfaces to missing root, then uses
current live data to preserve unrelated form drafts;
server-root replacement and reconnect reset drafts explicitly. All five v4 findings
are proposed fixes applied for this requested review, not verified runtime changes.

The specification revision itself did not establish a runtime pass. The subsequent
implementation checkpoints are recorded in [the acceptance report](../experiments/browser-dashboard/hosted/acceptance.md).
Reviewers must read this file and all four linked
specification artifacts, inspect the relevant baseline, and preserve original
finding severities when checking whether each gap is resolved.
