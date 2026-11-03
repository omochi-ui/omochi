# Implementation readiness re-audit — browser hub v3, round 2

Reviewer: `browser_v3_readiness_r2`, fresh-context, read-only.
Standard: EGM Implementation Readiness Standard.
**Completed; not clean: five Major and one Nit.**
This is a specification verdict, not a rejection of browser-to-gateway feasibility.
No edits, builds/tests, or agent delegation by the reviewer.

## Required reads and integrity

Read in full: readiness persona/standard; main plan 1–468 (FeatureRange 1–290,
DetailedRange 291–468); contract 1–706; board schema/fixtures; v3 manifest and
resolution report; v2 synthesis/readiness report; baseline README/gateway v4;
experiment README/RESULTS/evidence result. Also inspected relevant renderer/core
snapshot/routing, hub-ui contracts/App/services, protocol/session/path, pairing,
gateway dispatch/revocation, transport and build sources.

All four hashes match [review-state.md](review-state.md): main
`4d09c4a447c7098002ee913ed48f97ec8b20b2e8acbb7852c65b36d0646950b0`,
contract `5fa790443baf7e6656e1b3dbea067f07f4be6e2e7835c1454b28ad8b5b0bae3c`,
schema `70236ee5f1f95228fcc7e45c39bc1cbc0b3f3df824124a9a3945645137db2aae`,
fixtures `5eea92624b334b8750ba2be405ad45b3c47a80c9cc1673fd6198885efc4c6661`.
Missing or inaccessible required reads: none.

## readiness-v3-r2-1 — Major — Commit application success lacks feedback

Affected: `spec/browser-hub-v1.md:66–82,94–101,179–184,290–295`;
main plan M1 `docs/dashboard-remote-access.md:377–394`.

The manager marks ready only after successful synchronous processor application,
retracting that service on failure. Its only application interface is subscribe:
listeners return void and exceptions are isolated from other listeners. There is
no authoritative renderer listener or distinction between its failure and an
unrelated observer exception. A caught processor error cannot be reported, while
a thrown error relies on unspecified manager handling. Implementations can thus
disagree about readiness after failed application.

Define an authoritative synchronous apply callback returning an explicit result,
or an equivalent commit-result API. Separate observer exception isolation. Test
partial-commit throws: retract affected service, abort leases, forbid its actions,
keep another service ready.

## readiness-v3-r2-2 — Major — ProfileOwner cannot expose/execute required recovery workflows

Affected: `spec/browser-hub-v1.md:107–114,391–403,414–450`;
main M2 `docs/dashboard-remote-access.md:399–409`.

snapshot() returns only selected accepted gateways (max two). No separate view
exposes unselected accepted/pending/accepted-but-unsaved enrollment state. Through
this boundary the UI cannot list a newly paired unselected gateway, show pending
enrollment on reload, retry only acceptance persistence, or perform identity reset.

pair(invitation) specifies full validation/network redemption; it cannot express
a local-only persistence retry after acceptance, especially after invite expiry.
Add immutable profile/UI state containing all records and recovery states, plus
local commit retry and identity reset methods. Define recovery when profile open
rejects corrupt identity. Keep live construction limited to selected accepted
records; assert every workflow is reachable through the public API.

## readiness-v3-r2-3 — Major — Explicit source/service Retry lacks a public operation

Affected: `spec/browser-hub-v1.md:75–82,265–295,318–324,395–403`; main M1/M2.

Gateway protocol failures are terminal until Retry/re-enrollment; capacity-terminal
services also need Retry or capacity-releasing desired-source changes. Neither
LiveSession nor ProfileOwner supplies retry or defines an existing call for it.
start is idempotent and forbidden after stop; unchanged setServices has no defined
terminal effect. Gateway deselect/reselect replaces the full session, interrupting
the other gateway.

Define retry scope, cleared terminal states, backoff reset, new generations/leases
and unaffected-source preservation. If service remove/re-add is recovery, state
it explicitly and define the corresponding gateway operation.

## readiness-v3-r2-4 — Major — Service reads retain unbounded phases

Affected: `spec/browser-hub-v1.md:243–250,287–295,328–345`;
M1 checkpoint `docs/dashboard-remote-access.md:390–394`.

Snapshot timeout starts at snapshotStart; nothing bounds stream open/wait for
that first frame. A healthy control heartbeat can coexist with an indefinitely
awaitingSnapshot service. Ready reads have no idle deadline but do not distinguish
idle before a frame from a header/body that begins and stalls. Control heartbeat
does not detect that stall, leaving partial-frame timeout promises non-executable.

Specify service-open/first-snapshotStart and in-progress-frame deadlines while
retaining unlimited healthy ready idle before the next frame's first byte. Define
classification, abort/join, accounting release and service-only retries. Test with
control heartbeat healthy throughout.

## readiness-v3-r2-5 — Major — M1 browser checkpoint requires an M2 entry/build

Affected: `docs/dashboard-remote-access.md:323–327,347–363,384–405`;
`spec/browser-hub-v1.md:603–621,662–669`.

M1 requires a real fixture/gallery and session-mode harness before persistence.
The map first creates main.hosted.tsx, hosted HTML/Vite and ConnectionScreen in
M2; build:hosted depends on them. No M1-only entry/build is identified. Implementers
must prematurely do M2 or invent a page/build. The isolated experiment does not
exercise the production code required by the checkpoint.

Move a minimal in-memory hosted/gallery entry/build to M1, with unfinished features
disabled, or specify a separate M1 test entry/build and --mode session prerequisites.
M2 then replaces profile source and exposes the connection workflow.

## readiness-v3-r2-6 — Nit — Duplicate-ID fixture also duplicates the reference

Affected: `spec/fixtures/board-export.json:210–247`;
M4 fixture checkpoint `docs/dashboard-remote-access.md:437–440`.

duplicate-panel-id duplicates both the ID and complete Surface reference. A codec
missing ID uniqueness but enforcing reference uniqueness still rejects it. Give
the second panel a distinct valid reference while retaining duplicate ID and
non-overlapping geometry. duplicate-reference already isolates the other rule.

## Prior-group assessment

Original severities retained; resolution means specification coverage, not runtime.

| Group | Original severity | Assessment |
| --- | --- | --- |
| V2-01 | Blocker | Partial: packages/pins/exports/map/compatibility/build separation concrete; apply feedback and M1 checkpoint remain (1,5) |
| V2-02 | Blocker | Partial: generations/leases/budgets/cancel/isolation/most timers concrete; recovery and read phases remain (1,3,4) |
| V2-03 | Major | Resolved: v4 bounds/bare channels/payload prefixing/metadata/max-ID tests |
| V2-04 | Blocker | Partial: DB/transactions/lock/bootstrap/recovery policy concrete; public recovery API incomplete (2) |
| V2-05 | Major | Resolved: captured resource scope/chunks/inflation/budgets/unsupported media; runtime unverified |
| V2-06 | Blocker | Resolved with fixture nit (6): schema/semantics/both repos/CAS/errors/import-as-copy concrete |
| V2-07 | Major | Partial: matrix/headers/relay/harness/load/recovery concrete; M1 entry missing (5) |
| V2-08 | Major | Resolved: origin/epoch/session/barrier/bounded writes/ACK/Local rules explicit; runtime unverified |

M0 still supports the recorded single-gateway browser path. M1/M2 need these
interface/recovery/deadline/checkpoint decisions before satisfying zero-decision
readiness. Final verdict: completed, not clean — five Major and one Nit.
