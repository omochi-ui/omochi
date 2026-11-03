# Codebase details review — browser hub v6, round 5

Reviewer: browser_v6_codebase_r5, completed fresh-context/read-only.
Standard: EGM Implementation Readiness Standard.
**Clean: 0 Blocker, 0 Major, 0 Nit. No new finding IDs.**
Focused correction agrees with source snapshot/installed processor/admission/
lifecycle/native boundary. Plan acceptance in assigned scope, not runtime acceptance.

## Prior disposition

parent-v5-1 Major: resolved. style-v5-r4-1 Nit: resolved at main 319. Historical
codebase-v4-r3-1 also remains resolved by shared planner/current-live-root/root
replacement/scratch contract. Original severities retained.

## Source reconciliation

- Core a2ui-snapshot.ts 93–113 stores undefined/hasData:false; 31–78 selects arrays
  for canonical initial numeric segments, replaces primitive parents and shifts
  deletions. Rust snapshot.rs data:None/has_data:false and pointer helpers agree.
- Installed SurfaceModel creates DataModel({}) at surface-model.js 48; DataModel
  defaults {} at data-model.js 31. Snapshot 58–64 corrects replay immediately after
  create; 247–257 corrects ready creation with admitted one-batch initialization.
- Installed processor 176–179 iterates synchronously and 262–272 forwards absent
  value; DataModel 65–68 replaces root with undefined and notifies signals.
- First /0 now selects the same array container; delete/recreate follows the same
  initialization. Real roots override; native defaults remain {}.
- Snapshot byteSize remains source replay; synthetic bytes belong only to payload
  or ready scratch (47–64,195–212,247–253). Route sizing precedes copying, active
  create charge remains, commit avoids double initialization, finally releases.
- Direct patch normalization reads current root, reserves then materializes
  (222–239). Processor action metadata uses those roots (143–156); drafts/reset
  policy is at 259–265. Hub 201–227 gates publication/actions/ready/cleanup.

Processor internally emits create before the next message; the proposal correctly
requires the sink to batch notifications and gate actions rather than assuming
processMessages provides transaction isolation. Tests 287–299 cover all initial
root/patch/recreate/explicit-root/budget/action/parity/draft/failure/native cases.

## Completion and integrity

Read persona/standard; full main516, hub1005, snapshot299, schema64, fixtures361;
v6 state/resolution; v5 codebase/style; README/AGENTS/gateway-v4. Inspected actual
core/Rust snapshots; installed processor/DataModel/Surface/group/events/signals;
renderer client/hooks/Surface, bound inputs, routing/metadata/manifests; gateway
snapshot/wire dispatch and shared disposal test infrastructure. All five hashes
and counts matched at initial and final verification. Required reads missing: none.

No edits/builds/tests/probes/experiments/delegation/history retrieval. Future Create
files were treated as future work.
