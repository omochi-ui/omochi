# Implementation readiness — round 1

Reviewer: `egm-implementation-readiness`, freshly dispatched as
`browser_v2_readiness_r1` with `fork_turns: none`.
Standard: EGM Implementation Readiness Standard.
Target: [revised plan](../../../dashboard-remote-access.browser-v2.md), DetailedRange 278–365,
FeatureRange 1–277 as context.
SHA-256: `4f989994ec084d39d0e70393c01a36a47890fc5fd75bf168073d03b025cedcc6`.

## Verdicts

Feasibility: supported for the demonstrated single-gateway path. The recorded
experiment provides credible end-to-end evidence for browser-to-gateway access.
No evidence was found that requires a native hub intermediary or makes the
direction technically infeasible.

Implementation readiness: FAIL under the zero-decision standard. M1–M5 remain
design milestones. Several acknowledged release gates contain architectural
decisions required before implementation can produce a predictable result.
These are unresolved prerequisites, not evidence against feasibility.

## Evidence assessment

The experiment uses the real gateway and Rust SDK through Unix discovery, a
separate iroh WASM wrapper, the existing renderer, and a static-only asset server.
The harness starts no native hub and uses no certificate-verification bypass.
Recorded JSON has 16 passing assertions, authenticated IDs, real WSS requests,
an authorization close before enrollment, counter update, heartbeat,
identity-preserving reload, and connection close after native forget. The
screenshot and source agree with those claims.

Reconnect is explicitly invoked by the harness, not proof of automatic recovery.
Atomic snapshots, form metadata, cancellation safety, production storage,
multiple sources, unsupported-media isolation, and hosted HTTPS/browser coverage
are not demonstrated. The plan acknowledges these limits.

The spike is not a production template: `web/main.tsx:18–25` races timers without
cancelling the WASM read; `151–164` renders snapshot messages before completion;
`140–142` sends null model metadata; `168–174` uses mutable connection/service
fields for resources; `79–104` uses localStorage. Its WASM wrapper has no stream
reset/stop API. These support the planned production work, not rejection of M0.

## readiness-v2-r1-1 — Blocker — Production package and adapter APIs are undecided

Scope: M1, lines 290–299; affects M2–M4.

“Define a small WASM transport,” “reuse types or fixtures,” and “add the live
session interface” leave language ownership, workspace location, exports, and
dependency direction undecided. There is no production Create/Modify/Delete map.

The renderer hook directly constructs `A2uiWebSocketClient`; `SurfaceView` uses its
inferred return type; `BrowserImages` assumes native image transport. The spike
bypasses exports through source/node_modules imports. Native transport also adds
mDNS and inherits full Tokio features, unlike the demonstrated small wrapper.

Specify package/crate paths, Rust/TypeScript ownership, session/image/repository
interfaces and lifecycle methods, exports, toolchain pins, build integration,
and files to refactor. Select shared types or concrete conformance fixtures.

## readiness-v2-r1-2 — Blocker — Session algorithm and capacity contract are unspecified

Scope: M1, lines 291–298; M5, lines 332–335.

The plan names atomic snapshots, generations, capacity, cancellation and backoff
without state structures, transitions, or exact failure policy. Missing decisions:

- Snapshot staging and per-service commit while another gateway remains live.
- Aggregate gateway/service/snapshot/action/resource limits and admission order.
- Accounting lifetime and overflow recovery.
- Which invalid messages end a service or its complete gateway session.
- Deadlines, partial-read disposal, cleanup waits, and shutdown.
- Backoff bounds/reset conditions and suspension recovery.

Native `hub/src/lib.rs:559–632`, `capacity.rs`, and `heartbeat.rs` supply reference
behavior, not a selected browser algorithm. Specify a state machine, numeric
budget and validation/error tables, and cancellation contract. Add deterministic
assertions for interrupted snapshots, same-ID recreation, service replacement,
overflow, stale callbacks, and A-fails/B-remains-live.

## readiness-v2-r1-3 — Blocker — Enrollment storage and recovery contract is unresolved

Scope: M2, lines 303–312; bootstrap context lines 131–135.

IndexedDB and one endpoint owner are chosen, but database version/object stores,
records, transactions, lock name/lifetime, contention, crash recovery, storage
failure recovery, wrapper schema and encoded bound are not defined.

Write ordering does not settle a lost acceptance reply or remote enrollment
followed by failed local commit. Define persistent pending state, reload behavior,
when ordinary traffic is legal, retries after expiry, and corrupt-profile recovery
that creates a different identity. Specify accepted-but-unsaved and outcome-unknown
states, lock shutdown, and recovery actions while retaining same-identity retries.

## readiness-v2-r1-4 — Major — Resource/media adapter is not concrete

Scope: M3, lines 316–320; depends on M1 adapters.

The plan does not define an immutable source/generation scope for reads,
response/chunk validation, stream abort disposal, or renderer capability selection.
Current `resources.tsx:49–50` creates HTTP SDK Audio/Video URLs and delegates
external media to basic components. Replacing image fetch alone leaves active
HTTP paths despite direct mode's PNG-only scope.

Reuse existing image budgets: 8 MiB encoded, 4 Mi pixels, two readers, and
32 MiB decoded accounting. Specify captured route/lifetime fields, PNG checks,
resource validation/abort rules, explicit unsupported-media rendering, and native
behavior preservation. This does not request implementation of unsupported media.

## readiness-v2-r1-5 — Blocker — Board exchange and repository contracts are undefined

Scope: M4, lines 324–328.

The exact export JSON, optional source-name representation, nested field rules,
panel-ID policy, revisions and repository errors remain choices. Existing
`DashboardFile` is native multi-board storage with IDs/revisions, not the new
one-board exchange shape. Native serde accepts unknown fields, unlike the planned
strict importer; existing validation therefore cannot be reused without a new
contract and compatibility fixtures.

Specify exchange/browser-record schemas, ID/revision allocation, compare-and-save,
delete conflicts, quota/abort errors, native import/export entry points, validator
location, and fixtures for nested unknown fields, duplicate references/IDs,
geometry, unsupported versions, and rejection without storage/trust mutation.

## readiness-v2-r1-6 — Major — M5 includes unmade product/deployment decisions

Scope: M5, lines 332–338; deployment context lines 237–246.

Browser/version matrix, required facilities, relay, hosted configuration,
operational limits/metrics, and measurable load/recovery targets are absent.
“Declared browsers” and CSP verification have no declaration/policy to execute.
The current UI/Vite setup starts same-origin native endpoints.

Select the first support matrix, unsupported-facility behavior, hosted/native
entry/adapter selection, static/WASM/CSP/base-path settings, relay configuration,
and acceptance targets. Actual publication can remain a separate operation.

## readiness-v2-r1-7 — Major — Stable file-scoped implementation checkpoints are missing

Scope: M1–M5, lines 288–361.

Acceptance behaviors are useful, but commands do not name the production direct
harness, build prerequisites, fixtures, or phase pass conditions. The existing
hub-ui browser script starts a native hub and is a local compatibility check.

Decide when the UI selects a new adapter, how unfinished features stay disabled,
how M1 is exercised before M2 enrollment, and how M4 remains testable with both
repositories. Then split phases into file-level changes, dependencies, interim
behavior and exact build/test checks. Reuse the experiment's real fixture and
sanitized CDP evidence without claiming its assertions cover new production code.

## Milestone startability

| Milestone | Zero-decision startability | Dependencies |
| --- | --- | --- |
| M0 | Evidence retention is actionable; recorded scope complete | Harness promotion needs a production test/package boundary |
| M1 | No | Package/API, state machine, validation/capacity/cancellation contracts |
| M2 | No; storage design can proceed alongside M1 design | Durable schema, locking/recovery/bootstrap, endpoint lifecycle API |
| M3 | No | M1 scope/session APIs, PNG/media capabilities; end-to-end use also needs M2 |
| M4 | No; schema/validator design can proceed independently | Exchange/repository transactions, M2 profile storage and M1 unavailable-source interface |
| M5 | No as executable acceptance; matrix/harness design can start | Declared support/deployment/capacity targets and M1–M4 behavior |

Prioritize M1 package/session contracts and M2 durable identity. Define the board
schema/repository in parallel, then resource capabilities and phase-level checks.
Repeating the successful connectivity spike is not the missing prerequisite.

## Read inventory and completeness

Read persona and standard; full target/ranges; experiment README/RESULTS,
browser/WASM/fixture source, Cargo manifests and relevant lockfile pins, build/run
harness, Vite/HTML, recorded JSON and screenshot. Production reads included
gateway dispatch/auth; protocol framing/session/path validation; pairing and
transport setup/manifests; hub snapshot/routing/capacity/heartbeat/storage;
renderer session/client/images/resources/image-view/exports; hub-ui contracts,
services/layout/App/package/Vite configuration and native browser harness;
renderer client tests, native asset build setup and gateway v4 specification.

No required source was unreadable. Historical findings were not current
requirements. No files were edited, no builds/tests run, and no agents spawned.
