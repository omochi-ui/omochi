# Dashboard remote access review state

> Historical review state from 2026-11-03. The implementation was later retained
> as the [browser dashboard experiment](../../../experiments/browser-dashboard/README.md).
> Use the [current remote access overview](../../dashboard-remote-access.md) and
> [specification index](../../../spec/README.md) for the active native-Hub design.
> Statements about pending implementation below describe the review checkpoint.

Current author task remains fix and review, 2026-11-03. V5's three fresh auditors
completed clean with one editorial Nit, which was corrected. Parent final-check
probe found an initial missing-root mismatch before final acceptance. Its small
normalization/budget/test correction completed a new focused v6 review: all three reviewers returned no findings.
No production implementation/deployment or automatic further round is inferred.

## Current target

[Main plan](../../dashboard-remote-access.browser-experiment.md), FeatureRange 1–291,
DetailedRange 292–516, and full [hub contract](../../../spec/browser-hub-v1.md),
[snapshot/patch contract](../../../spec/browser-snapshot-v1.md),
[board schema](../../../spec/board-export.schema.json),
[fixtures](../../../spec/fixtures/board-export.json).

[V6 final report](browser-hub-v6/implementation_readiness_report.md) records
completion with zero unresolved Blocker/Major/Nit. The reviewed scope is ready
for implementation; no production runtime acceptance is claimed.
[V6 state](browser-hub-v6/review-state.md) preserves all frozen inputs and actual
reviewer completion. [Correction map](browser-hub-v6/goldfish_review_report.md) records
parent-v5-1 Major and style-v5-r4-1 Nit, both corrected before dispatch.

[V5 disposition](browser-hub-v5/implementation_readiness_report.md) preserves all
clean auditor reports and the later counterexample. No runtime/production pass
or two-clean-full-round convergence is inferred. Await the Stage 9 terminal
checkpoint; no M1 implementation or further review starts automatically. Source access remains coarse
trusted-Hub gateway access; boards remain independent documents. M1–M5 are planned.

## Previous v4 round

[V4 combined report](browser-hub-v4/implementation_readiness_report.md): all three
reviewers completed, three Major/two Nit, not clean. Exact plan/hub/snapshot
inputs are archived with unchanged hashes. That round identified accepted-link
refresh API, temporary relay admission, live/replay patch equivalence and two
editorial corrections. The latest request authorized their resolution/re-audit.

## Previous detailed-contract re-audit

[V3 report](browser-hub-v3/implementation_readiness_report.md): all three reviewers
completed, nine Major and three Nit; not clean. Prior groups V2-03/05/06/08 were
resolved at specification level and V2-01/02/04/07 partial. Exact main, contract
and fixture inputs are now archived, preserving their frozen hashes. This round
follows the user's explicit request to fix those residuals and review again.

## Previous browser readiness audit

[Round 1 report](browser-hub-v2/implementation_readiness_report.md), all reviewers
completed: readiness 4 Blocker/3 Major, codebase 10 Major, style none. It supports
technical feasibility but fails the zero-decision implementation standard.
Its exact [archived target](../../dashboard-remote-access.browser-v2.md) remains
365 lines, FeatureRange 1–277, DetailedRange 278–365, SHA-256
`4f989994ec084d39d0e70393c01a36a47890fc5fd75bf168073d03b025cedcc6`.
Original findings and severities are preserved. Current fixes require re-audit;
no five-stage architecture pass or two-clean-round convergence is claimed.

## Historical review target

Archived target: [original native-hub-sharing proposal](../../dashboard-remote-access.server-hub-v1.md).
Baseline commit: `a21247426c694fe2e67bb16b43a566a0ffb3694a`.
SHA-256: `bc7a539431495d8445ddee42110a8c8f1cd30f0f540b611db3a495f2935efa5f`.
Total: 330 lines. FeatureRange: 1–239. DetailedRange: 240–330.
The archive exactly matches the hash used by the actual Stage 5–7 reviewers.

Mode was interactive full review. Every completed reviewer ran with a fresh
conversation context and read the target's linked baseline. No re-audit was used.

| Stage | Historical status | Report |
| --- | --- | --- |
| 5. Initial Zoom Out | Completed; 1 Major, ZO-001 | [Report](zoom_out_report.md) |
| 6. Comprehension | Completed; no comprehension findings | [Report](comprehension_report.md) |
| 7. Design Critique | Round 1 completed; design/style each 1 Major, structural none; not converged | [Report](critic_report.md) |
| 8. Intermediate Zoom Out | Not started before interruption | No report |
| 9. Implementation Readiness | Not started before interruption | No report |

ZO-001 and duplicate `design-1-1` concerned resource authorization under promised
Surface-scoped sharing. `style-1-1` concerned personal CRUD permission in the
combined hub/board grant model. They were not verified as fixed. The relevant
scope has now been removed: gateway access is deliberately coarse and board CRUD
is a repository concern. Record those findings as superseded by the architecture
change, not as clean audits or implemented fixes.

The [annotated repair proposals](goldfish_review_report.md) are historical and
must not be automatically applied to the current target.

## Decision trail

- [Surface-sharing implementation note](surface-sharing-design-note.md): an
  earlier proposed publication manager; superseded by gateway-level sharing.
- [Responsibility clarification](responsibility-clarification.md): existing
  native persistence and the reason to separate layout documents from source
  permissions.
- [Multiple-hub discussion](multiple-hubs-design-note.md): independent hub roles
  and trust boundaries, before adopting browser-resident gateway clients.
- [Browser alternative](browser-hub-alternative.md): experiment proposal, now
  followed by the recorded result and current implementation plan.

No production implementation or deployment is claimed by these documentation
updates. Earlier interruption remains part of the decision history; the latest
explicit request authorized detailed specification edits and this one re-audit.
