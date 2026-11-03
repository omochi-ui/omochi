# Codebase details and feasibility — round 1

Reviewer: `egm-codebase-details`, freshly dispatched as `browser_v2_codebase_r1`
with `fork_turns: none`; includes its requested read-only queue-revocation addendum.
Standard: EGM Implementation Readiness Standard.
Target: [revised plan](../../../dashboard-remote-access.browser-v2.md), DetailedRange 278–365,
FeatureRange 1–277 as requirements.
SHA-256: `4f989994ec084d39d0e70393c01a36a47890fc5fd75bf168073d03b025cedcc6`.

## Verdicts and evidence

Feasibility: supported for the recorded single-gateway browser path. No production
source contradiction establishes a need for a mandatory native hub.

Readiness: FAIL. M1–M5 still require API, schema, lifecycle, packaging, and build
decisions. There are no Blocker findings for false current-state claims in the
active plan. The findings below are Major readiness defects.

Recorded evidence has 16 passing assertions, real WSS traffic, authenticated
identity, action/update, PNG reads, identity-preserving reload, and revocation.
The fixture invokes real `Gateway::listen_remote`, `Invite::issue`, and
`live::forget` (`fixture/src/main.rs:59–98`) without a hub crate dependency.
`run.mjs:51–60` only serves static files; `113–146` exercises Chromium.
`wasm/src/lib.rs:44–83` uses real iroh streams. The spike's direct source imports,
empty form metadata and limited lifecycle are acknowledged limitations, not
claims of production completeness.

## codebase-v2-r1-1 — Major — File-change map and stable checkpoints are absent

Scope: lines 288–338.

There is no Create/Modify/Delete map or buildable intermediate result. Changes
cross renderer `client/surface/images/resources`, hub-ui `App/services/contracts`,
manifests/build scripts and browser integration. Report this once, rather than
flagging proposed-but-absent paths individually. Define each file's responsibility,
public API after its milestone, and checks keeping local mode usable.

## codebase-v2-r1-2 — Major — Browser package boundaries and renderer APIs are undecided

Scope: lines 290–299, 314–320; context 86–96.

The spike uses source and nested dependency imports (`web/main.tsx:1–8`). Production
renderer exports only root/CSS (`package.json:8–14`), and its root exports only
client/Surface APIs, not `BrowserImages` or `resourceCatalog`. `A2uiSession` derives
from the WS-constructing hook (`surface.tsx:9–38`). Core's root exports native child,
HTTP and WS-server modules; core resources import Node modules. Native transport
also constructs mDNS (`transport/src/lib.rs:30–36`), so disabling desktop is not
a demonstrated browser extraction.

Choose WASM/session package locations, dependency direction, browser-safe exports,
session/image interfaces, and who owns `MessageProcessor`/catalog creation.
Keep dashboard repositories outside renderer.

## codebase-v2-r1-3 — Major — Existing TS routing helpers retain legacy IDs

Scope: lines 292–293, 316–319; requirements 169–174.

`core/src/service-id.ts:14–19` defines 16-character base32 nodes and an 81-character
channel limit. `splitRouteId:46–52` validates that old format. Native v4 uses
64-character lowercase hex EndpointIds (`protocol/src/session.rs:44–57`).
`splitRouteId` rejects v4 nodes. `routeClientMessage` calls `splitSurfaceId`, which
enforces the channel limit (`prefix.ts:24–29,112–123`); a valid v4 route with a
service ID longer than 16 characters exceeds it, though services may be 64 long.

Explicitly align/replace helpers and fixtures before reuse. Useful metadata
narrowing exists in `prefix.ts:68–89,112–123` and `hub/src/routing.rs:34–63`.
Preserve it and send `getClientDataModel()` as the local client does
(`renderer/client.ts:152–158`), rather than the spike's null metadata.

## codebase-v2-r1-4 — Major — Session, snapshot and cancellation contract is missing

Scope: lines 295–299, 317–319, 332–335, 344–345.

The spike's `Promise.race` does not cancel reads (`main.tsx:18–25`); its wrapper
has no reset/stop (`lib.rs:87–121`); it renders before snapshotEnd and has no
ready guard (`main.tsx:139–164`); reads use mutable current service/connection
(`168–174`). Copying it would not achieve the stated requirements.

Reusable references: protocol `Session` tracks epoch/generation/readiness
(`session.rs:60–119`); native hub stages snapshots (`lib.rs:559–632`); scopes track
instance and Surface generation (`scoped.rs:18–25,90–109`); actions check cancellation
(`lib.rs:674–695`); resource handles validate before/after I/O (`scoped.rs:170–197`).

Specify transitions, staging/commit, captured IDs, same-ID recreation, stream
disposal/reset, outstanding promise ownership, queued-action discard, and
service-versus-gateway failure behavior.

## codebase-v2-r1-5 — Major — Numerical capacity and pre-allocation validation are unresolved

Scope: lines 290–296, 332–335, 352–353; gate 2.

Existing limits occur at different layers: 4 MiB frames (`protocol/lib.rs:16,37–55`),
16 MiB fanout (`fanout.rs:17`), 8 MiB action/publication queues retained through
processing (`queue.rs:5–13,46–76`), and native embedded capacity of two gateways,
eight services, 16 MiB snapshots per service and 32 MiB total
(`hub/capacity.rs:11–15`). Path validation rejects prototype keys, depth and combined
numeric indices before sparse expansion (`protocol/data_path.rs:5–27`).

Select browser admission/accounting/overload rules and numbers, validation
placement, and how Rust u64 generation/heartbeat values cross JavaScript's number
boundary. Frame size alone does not bound retained snapshots or parsed allocation.

## codebase-v2-r1-6 — Major — Persistence, bootstrap and endpoint ownership lack schemas

Scope: lines 301–312; context 131–155.

M2 supplies no DB version/stores, records, transactions, lock name/lifetime,
recovery transitions, or bootstrap format/bound. Native references include
`Invite::decode` bounds/version/secret/expiry (`pairing.rs:53–74`), distinct invalid/
connect/rejected/unknown outcomes (`121–138`), exchange cleanup and durable peer
persistence requirements (`143–196`), and same-identity retry only within expiry
(`store.rs:159–193`).

Define key/peer commit failures, pending state, crash recovery, denied/corrupt
storage behavior, and lock acquisition/loss/shutdown. None follows automatically
from the spike's disposable localStorage.

## codebase-v2-r1-7 — Major — PNG fetch replacement does not enforce the media scope

Scope: lines 314–320; requirements 183–198, 234–235.

Reuse `BrowserImageScope.load` limits (`images.ts:16–66`) and `ScopedImage` cleanup
(`image_view.tsx:9–34`). The spike ignores `_handle`, checks abort only between
operations, and reads mutable source state. Its adapter must become lifetime-bound.

Separately, Video/Audio always resolve SDK resources to HTTP and retain external
basic-component behavior (`resources.tsx:49–50`). Copying that catalog can make
HTTP requests despite the unsupported-media policy. Define captured image scope,
stat/chunk validation, PNG header/IHDR checks, stream abort, direct-mode unavailable
Image/Markdown/Video/Audio behavior, and explicit native preservation.

## codebase-v2-r1-8 — Major — Board exchange and cross-repository path are unspecified

Scope: lines 322–328; requirements 200–227.

`DashboardFile` is multi-board native storage with IDs/revisions
(`hub-ui/contracts.ts:20–30`), not a one-board share schema. Rust structures accept
unknown fields (`hub/dashboards.rs:27–55,68–70`), unlike the planned importer.
Reuse precise UTF-16, ID/geometry, duplicate/reference/overlap and trim rules
(`65–116`) and revision/durable replacement behavior (`175–242`) through explicit
shared fixtures rather than assuming they already implement the new contract.

Specify export shape, panel-ID retention/regeneration, validator/fixtures,
repository methods/errors, IndexedDB conflict transactions, and native UI import/
export. App currently binds CRUD to HTTP (`App.tsx:9–16,58–84,140–160`) with separate
service subscription (`104`). Browser-only import/export would not implement
the promised repository/origin bridge.

## codebase-v2-r1-9 — Major — Production build/packaging and harness are not defined

Scope: lines 290–293, 330–338, 355–360; requirements 237–246.

The spike has its own workspace/lock/target/bindgen/Vite steps; root production
scripts do not build that WASM. Decide generated bindings/assets ownership,
lockfile ownership and how native-only builds avoid unnecessary WASM prerequisites.
Native distribution embeds hub-ui (`hub/build.rs:6–22`, `package-crates.py:29–46`),
and static serving has no explicit WASM MIME mapping (`hub/http.rs:352–360`).
Choose loading/asset paths and MIME/fallback handling for both delivery modes.

Existing `hub-ui/test/browser.mjs` starts a native hub and needs a prior build;
it is not a production direct-mode test. Specify the new harness, real fixture
lifecycle, fault injection, prerequisites, CI commands and phase checks. Reuse
the experiment and core's disposal conventions (`testing.ts:10–36`).

## codebase-v2-r1-10 — Major — Revocation does not cancel queued SDK actions today

Read-only addendum. Scope: feature 112–117; plan 295–299, 344–347.

Gateway revocation tears down connections before acknowledgement, but does not
invalidate their commands already in the independent SDK queue:

- `gateway/lib.rs:42–44`: `Command::Action(Value)` has no peer/session/cancellation
  provenance.
- `Gateway::action:104–124` enqueues into the service-wide input queue.
- That queue is created independently of remote sessions (`229–262`).
- `serve_sdk:303–314` drains and sends without origin authorization checks.
- `remote.rs:56–62` cancels sessions, joins tasks and acknowledges; `118–120`
  closes/aborts streams. Neither invalidates commands already in the SDK queue.

An accepted queued action may reach the SDK after session loss, potentially after
revocation acknowledgement. Browser cancellation cannot prevent that. This is
a plan gap, not a false claim that M0 tested a stronger guarantee. Feasibility
remains supported.

Include `gateway/src/{lib,wire,remote}.rs` in the change map. Propagate session
lifetime to queued remote actions, check cancellation before SDK dispatch, and
define synchronization with an in-progress dispatch. Explicitly define when
dispatch becomes irreversible. Preserve Unix callers through local provenance
or an equivalent design.

Add a regression fixture that blocks SDK delivery, queues remote actions, revokes
the peer, waits for acknowledgement, releases the path, and verifies undispatched
revoked-session actions do not reach the SDK. No tests were run for this addendum.

## Source inventory and completeness

Read persona/standard, full target/hash, experiment README/RESULTS/manifests/pins,
WASM/client/build/Vite/run/fixture and JSON; production gateway stream/auth/resource;
protocol framing/session/path/fanout/queue; transport pairing/store/live; hub scopes,
aggregation/heartbeat/capacity/images/dashboards/static/Cargo/build; renderer
client/session/catalog/image lifecycle; hub-ui contracts/services/layout/App/startup/
browser harness; core exports/routing/IDs/snapshot/resources/testing; root/package
build/release and native packaging. Searches included SDK local-data/action handling
and installed A2UI model-metadata APIs.

No required source was unreadable. Historical findings were not used as current
requirements. No files were edited, builds/tests run, or experiments rerun.
