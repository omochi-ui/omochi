# Codebase details re-audit — browser hub v5, round 4

Reviewer: browser_v5_codebase_r4, fresh-context, strictly read-only.
Standard: EGM Implementation Readiness Standard.
**Completed; clean: no unsettled Blocker/Major. No new findings or IDs.**
Implementation-plan acceptance in this scope, not production/runtime acceptance.

## Prior finding resolution

| Prior finding | Original severity | Assessment |
| --- | --- | --- |
| readiness-v4-r3-1 / V4-01 | Major | Resolved: hub 512–595 separates refresh/retry/redemption, dispatch, private capture/expiry/failure; 610–633 specifies save retry/restoration |
| readiness-v4-r3-2 / V4-02 | Major | Resolved: hub 475–485 separates invite-only temporary set and selected-record union with close ordering/no truncation; 982–985 tests disjoint maximum sets |
| codebase-v4-r3-1 / V4-03 | Major | Resolved: snapshot 159–180,206–244 supplies shared planner/live-root normalization, forbids nested processor increment, preserves drafts, states authoritative reset and reserves scratch before materialize |
| style-v4-r3-1 | Nit | Resolved: main 335 fully qualifies hosted HTML/Vite/TS paths |
| style-v4-r3-2 | Nit | Resolved: hub 941 consumes existing fixture |

Resolution means specified, not implemented. Historical severities preserved.

## Source reconciliation

- Core a2ui-snapshot.ts 32–76 and Rust snapshot.rs 74–151 replace primitive
  parents and shift array deletions.
- Installed @a2ui/web_core 0.10.5 message-processor.js 262–272 delegates updates
  to DataModel.set. Installed data-model.js 65–68 supports root replacement and
  existing signal notification; adapter bypasses its different nested semantics.
- Installed processor 143–156 reads action metadata from current Surface roots;
  normalizing from that root preserves draft values used by this API.
- Snapshot 196–204,220–231 charges incoming plus scratch against the same 16 MiB
  slot, reserving before construction and releasing after synchronous application.
- Snapshot 266–274 requires installed-processor parity, rendered drafts/action
  metadata, capacity failures/cleanup and native bypass regressions.

The mechanism probe supports only root updating, not future planner/adapter/
integration correctness or production acceptance.

## Completion and integrity

Read persona/standard, full main 515 (Feature1–291, Detailed292–515), hub 1003,
snapshot 274, schema 64, fixture 361; v5 state/resolution; v4 synthesis/codebase;
README/gateway-v4; experiment README/RESULTS/evidence result. No required read
missing. Five counts/hashes match [manifest](review-state.md) at start and end.

Inspected core snapshot/routing/metadata/exports/build; Rust snapshot/gateway
validation; installed processor/DataModel/signals/Surface/bindings/inputs; renderer
WS/hooks/images/catalog/Markdown; hub-ui App/contracts/layout/SSE/type/build;
native dashboard geometry/HTTP/Origin/scopes; gateway queues/streams/pairing/trust
ACK; release/native packaging; experiment WASM/adapter/build/fixture; disposal
and native/packaging test infrastructure. Create paths remain future work.

No edits, builds, tests, probes, new experiments, delegation or history retrieval.
