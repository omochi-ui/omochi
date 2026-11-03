# Browser hub v2: feasibility and implementation readiness

Review completed: 2026-11-03, round 1.
Target: [current plan](../../../dashboard-remote-access.browser-v2.md), DetailedRange 278–365,
with FeatureRange 1–277 as requirements/context.
SHA-256: `4f989994ec084d39d0e70393c01a36a47890fc5fd75bf168073d03b025cedcc6`.

## Decision

**Proceed with the browser-to-gateway direction, but complete the implementation
contracts before treating M1–M5 as executable tasks.**

Technical feasibility is supported by the recorded real-browser experiment and
the production-source audit. No reviewer found evidence that the selected path
requires a mandatory native hub or a new downstream dashboard protocol. The
gateway-wide trust model and separate board documents are internally consistent
with this direction. Full multi-source and production behavior remains unproven.

**Implementation readiness fails the EGM zero-decision standard.** The plan
currently describes what each milestone must achieve and asks implementers to
resolve release gates. It does not yet say exactly where the code belongs, what
APIs/state machines/schemas to implement, or what numbers and failure rules to
use. This is not a verdict that the architecture cannot be built.

The next work is detailed design for M1/M2 and the board contract, not another
single-gateway connectivity experiment. Actual production coding and deployment
were not requested or performed in this review.

## Review completion and evidence

| Reviewer | Standard | Status | Raw findings |
| --- | --- | --- | --- |
| Implementation readiness | Implementation Readiness Standard | completed | 4 Blocker, 3 Major |
| Codebase details | Implementation Readiness Standard | completed, including queue-revocation addendum | 10 Major |
| Style | Persona only | completed | None |

All three reviewers ran in fresh conversation contexts. There were no failed
reviewers or unreadable required sources. They inspected the actual experiment,
recorded evidence, and production code; no new builds/tests were run. The reports
are [readiness](readiness_report.md), [codebase](codebase_report.md), and
[style](style_report.md). Original severity assignments are preserved there.

The 17 raw findings consolidate into eight action groups below. The four groups
labelled Blocker block an implementation-ready specification under the selected
standard; they are not evidence of failed browser connectivity or impossible
technology. Group severity is the highest original severity mapped to that group.

## Required design work

### V2-01 — Blocker — Fix package/API/build ownership and the file-change map

Choose the production WASM crate, browser session package, and public renderer
interfaces. Specify which side owns `MessageProcessor`, catalogs and image scope,
and keep board persistence outside the live renderer contract. Give every task a
Create/Modify/Delete target, dependencies, stable intermediate behavior and checks.

The spike uses internal source imports that package consumers cannot use. Current
renderer exports and inferred `A2uiSession` are tied to WebSocket construction;
core's root also exports Node-only modules. A browser-safe entry point and public
API are real work. Define bindgen/iroh pins, lock/workspace ownership, generated
bindings/assets, native-only build behavior and hosted/native loading paths.

Affected plan: M1–M3, plus shared build/checkpoints in M5.

### V2-02 — Blocker — Specify sessions, snapshot commits, cancellation and budgets

Define gateway/service/Surface states, epoch/generation capture, snapshot staging
and commit, action readiness, per-operation resource scope and failure boundaries.
Specify reset/stop/disposal through WASM, cleanup waits, partial-frame handling,
timeouts, matching-pong logic, backoff and suspension recovery.

Select concrete per-source and aggregate limits with admission, accounting, and
overflow rules. Include strict roster/envelope/A2UI validation, sparse-array/path
limits and safe handling of wire u64 values in JavaScript. The spike's timer race
does not cancel its read, snapshot messages render before completion, and resource
reads use mutable current service state. These must not become production defaults.

Affected plan: M1, M3, M5. Reuse native Session, snapshot staging, scope/generation,
capacity and heartbeat behavior as specified references rather than implicit rules.

### V2-03 — Major — Align existing routing helpers with v4 EndpointIds

`packages/core/src/service-id.ts` retains 16-character base32 nodes and an
81-character channel bound. `splitRouteId` rejects v4's 64-hex node IDs, and
`routeClientMessage` rejects valid v4 routes when service IDs exceed 16 characters.
Add explicit helper/fixture alignment to M1/M3 before reuse. Preserve metadata
narrowing and exercise two gateways, maximum-length service IDs and form drafts.

This is a concrete source compatibility trap, not a reason to reject the browser
architecture. The successful spike bypasses these helpers and sends null metadata.

### V2-04 — Blocker — Define durable enrollment, bootstrap and endpoint ownership

Specify IndexedDB version/stores/records and transactions, exact bootstrap schema
and encoded bound, lock name/lifecycle, pending enrollment state and normal-traffic
admission. Cover lost replies, remote success followed by failed local persistence,
reload, expiry, denied/corrupt/cleared storage and owner crash. Preserve the same
identity on uncertain outcomes; define recovery rather than inferring it from
disposable localStorage. This design can proceed alongside V2-01/V2-02.

Affected plan: M2, with an endpoint-lifecycle dependency on M1.

### V2-05 — Major — Define the PNG adapter and explicit unsupported-media behavior

Bind each read to immutable source/service generation, Surface/subscription
lifetime, and cancellation. Specify metadata/chunk/header validation and disposal.
Reuse the existing two-reader, 8 MiB encoded, 4 Mi-pixel and 32 MiB decoded budgets
where the browser contract chooses them.

Replacing image fetch does not disable SDK Video/Audio HTTP URLs or external
basic-component behavior in the current catalog. Define explicit unavailable
rendering for unsupported direct-mode media while keeping native behavior.
This finding requires enforcing the selected narrow scope, not broadening it.

Affected plan: M3 and renderer/session API work in M1.

### V2-06 — Blocker — Specify the board format and both repository contracts

Define exact one-board JSON, optional source labels, nested field policy, panel-ID
handling, stored browser record shape, ID/revision allocation, compare-and-save,
delete conflicts, transaction completion and errors. Native `DashboardFile` has a
different multi-board storage shape; native serde accepts unknown fields, so it
does not already implement the proposed strict exchange validator.

Select shared validator/fixture locations and native import/export entry points.
Specify rejection without mutating storage/trust. Test both repositories; a
browser-only feature would not provide the promised origin/native bridge.

Affected plan: M4; schema work can proceed independently of networking.

### V2-07 — Major — Turn M5 into a measurable integration and delivery checkpoint

Declare initial browsers/facilities and fallback behavior, static entry/adapter
selection, WASM MIME/base-path/CSP/compression settings, relay configuration, and
load/recovery criteria. Name the production direct-browser harness, build
prerequisites, CI commands and per-phase pass conditions. Existing hub-ui browser
tests start a native hub and remain local compatibility checks.

Native asset embedding also needs to include the chosen WASM path; the current
native static server has no explicit WASM MIME mapping. Production deployment
remains a separate step. A loopback-page/public-relay experiment is not a hosted
HTTPS or separate-network acceptance result.

### V2-08 — Major — Include gateway queued-action provenance in revocation work

The plan promises cancellation of not-yet-dispatched operations. Current gateway
commands already enqueued for SDK delivery have no originating peer/session token:
`Command::Action(Value)` enters a service-wide queue, and `serve_sdk` sends it
without checking remote-session cancellation. Closing and joining the remote
connection does not invalidate that queue.

To keep the promise, include `crates/gateway/src/{lib,wire,remote}.rs` changes that
propagate session lifetime, reject cancelled commands before SDK dispatch, and
define the irreversible dispatch boundary and revocation barrier. Preserve local
Unix caller behavior. No new wire protocol is inherently required by this source
change; the existing implementation simply does not supply the stronger guarantee.

Add a regression with blocked SDK delivery, queued remote actions, revocation and
acknowledgement, then release of the blocked path. Undispatched revoked actions
must not arrive. The experiment tested connection closure, not this case.

## Source traceability

| Raw finding | Original severity | Consolidated group |
| --- | --- | --- |
| readiness-v2-r1-1 | Blocker | V2-01 |
| readiness-v2-r1-2 | Blocker | V2-02 |
| readiness-v2-r1-3 | Blocker | V2-04 |
| readiness-v2-r1-4 | Major | V2-05 |
| readiness-v2-r1-5 | Blocker | V2-06 |
| readiness-v2-r1-6 | Major | V2-07 |
| readiness-v2-r1-7 | Major | V2-01, V2-07 |
| codebase-v2-r1-1 | Major | V2-01 |
| codebase-v2-r1-2 | Major | V2-01 |
| codebase-v2-r1-3 | Major | V2-03 |
| codebase-v2-r1-4 | Major | V2-02 |
| codebase-v2-r1-5 | Major | V2-02 |
| codebase-v2-r1-6 | Major | V2-04 |
| codebase-v2-r1-7 | Major | V2-05 |
| codebase-v2-r1-8 | Major | V2-06 |
| codebase-v2-r1-9 | Major | V2-01, V2-07 |
| codebase-v2-r1-10 | Major | V2-08 |

## Recommended next sequence

1. Specify V2-01/V2-02/V2-03 and the gateway boundary in V2-08, making M1 a
   file-scoped, independently testable task list.
2. Define V2-04 for M2 alongside that work. Specify V2-06's board contracts
   separately; no new network experiment is needed to decide their schemas.
3. Complete V2-05 renderer capabilities and V2-07 phase/delivery checks, then
   re-audit the concrete implementation instructions before broad production work.

No proposal or production code was changed. This round is not clean under the
readiness standard and no convergence is claimed. The author may conclude the
evaluation, request resolution/detailed design, edit the plan, or pause review.
