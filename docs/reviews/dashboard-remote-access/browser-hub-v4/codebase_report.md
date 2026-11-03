# Codebase details re-audit — browser hub v4, round 3

Reviewer: browser_v4_codebase_r3, fresh-context, strictly read-only.
Standard: EGM Implementation Readiness Standard.
Main scope 292–504, context 1–291; all four normative specs in full.
**Completed; not clean — one Major, no Blocker or Nit.**

Twelve v3 findings are addressed at their original stated gap scope. One additional
source-backed integration contradiction remains. These are future instructions,
not claims of implemented production integration.

## codebase-v4-r3-1 — Major — Ready data patches differ between snapshot and renderer

Affected targets:

- `docs/dashboard-remote-access.md:319–322,398–420`
- `spec/browser-hub-v1.md:70–75,195–213,328–335`
- `spec/browser-snapshot-v1.md:132–142,158–164,168–182`

Bounded snapshots adopt core semantics: array removal shifts later entries;
primitive intermediates become suitable containers. Ready increments update that
state and deliver the detached/prefixed increment to the installed renderer, whose
patch semantics differ. No normalization/adapter is specified.

Source evidence:

- `packages/core/src/a2ui-snapshot.ts:39–55` replaces primitive intermediates;
  lines 59–71 shift own later array indices and shorten length on removal.
- `packages/sdk/rust/src/snapshot.rs:74–95,103–115` agrees. Actual gateway uses
  that snapshot at `crates/gateway/src/lib.rs:291–301`.
- Installed @a2ui/web_core 0.10.5 `src/v0_9/processing/message-processor.js:262–272`
  calls surface.dataModel.set(path,value) directly.
- Its `src/v0_9/state/data-model.js:81–86` throws through primitive intermediate
  values; lines 97–100 delete array entries by assigning undefined, retaining
  length and subsequent indices.

Example: `{items:["a","b"]}` plus path /items/0 with no value becomes
`{items:["b"]}` in gateway/bounded snapshot but `{items:[undefined,"b"]}` in the
live renderer. Reconnect materializes the root and changes the displayed model.
Similarly `{a:1}` plus setting /a/b succeeds in retained state but throws in the
renderer, terminating an otherwise valid service through the authoritative sink.

Snapshot-vs-core unit parity and renderer cleanup tests cannot detect successful
live-versus-replay divergence. Specify the direct renderer boundary's exact
normalization/patch adapter, API and file ownership. Define unfinished client-form
preservation and delivery reservation accounting, keep native compatibility
explicit, and add installed-processor integration cases for array deletion,
primitive replacement and live/reconnect equivalence. Catching exceptions or
terminating a valid service does not reconcile supported semantics.

## Previous twelve findings

Original severities retained; resolved means specified, not runtime verified.

| Prior finding | Severity | Assessment |
| --- | --- | --- |
| readiness-v3-r2-1 / V3-01 | Major | Resolved: authoritative result/readiness/cleanup (hub 97–114,195–207); installed deleteSurface removes map entry before disposal |
| readiness-v3-r2-2 / V3-02 | Major | Resolved at prior scope: all-record/recovery views, local save retry, reconnect, board-preserving reset |
| readiness-v3-r2-3 / V3-03 | Major | Resolved: scoped retry, epochs/instances, coalescing/isolation (338–354,554–563) |
| readiness-v3-r2-4 / V3-04 | Major | Resolved: open/first/partial frame bounds and healthy idle (286–291,392–412) |
| readiness-v3-r2-5 / V3-05 | Major | Resolved: independent M1 entry/config/gallery/enrollment/build/prerequisites |
| codebase-v3-r2-1 / V3-06 | Major | Resolved: exact release selector/guards/tests/check (778–784) |
| codebase-v3-r2-2 / V3-07 | Major | Resolved: public core/native order/clean-output/packed consumer (771–797,822–831) |
| codebase-v3-r2-3 / V3-08 | Major | Resolved: pure key derivation/validation/outputs/errors/no-sockets (234–266) |
| codebase-v3-r2-4 / V3-09 | Major | Resolved at original scope: bounded API/sizing/reserve/rollback/delivery/files/tests now explicit; new renderer semantic mismatch remains |
| readiness-v3-r2-6 | Nit | Resolved: duplicate ID isolated from reference/overlap |
| style-v3-r2-1 | Nit | Resolved: board-ID exclusion distinguished from panel/reference retention |
| style-v3-r2-2 | Nit | Resolved: section 3 facade reference |

## Completion inventory

Required reads: main 504; hub 928; snapshot 182; schema 64; fixtures 361, all
full-file. Five hashes matched v4 manifest at start and end. Read persona/standard;
v4 state/resolution; v3 synthesis/codebase; README/gateway-v4; experiment README,
RESULTS/evidence result. No required read unavailable; future Create paths expected.

Baseline inspected: core routing/metadata/snapshot/exports/manifests/config;
renderer WS/hooks/catalogs/images/Markdown/views/exports/build; installed processor,
Surface group/data model/events/schemas; hub-ui contracts/SSE/layout/App/entries/
Vite/TS/native harness; gateway queues/dispatch/generations/authorization; protocol
session queues and Rust SDK snapshots; transport/pairing/store/revocation/ACK;
hub routing/scopes/Origin/dashboard/images/resources; root/release/native packaging,
test infrastructure/Cargo/npm launchers; experiment WASM/adapter/build/fixture.

No edits, builds, tests, new experiments, history retrieval or delegation.
Final verdict: not clean; concrete renderer patch parity contract still needed.
