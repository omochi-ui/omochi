# Browser hub v3: detailed specification re-audit

Completed 2026-11-03. Re-audit 1 of 1 in the current Stage 9 remediation scope.
Targets: [main plan](../../../dashboard-remote-access.browser-v3.md), DetailedRange 291–468
with FeatureRange 1–290 context; full [contract](../../../../spec/browser-hub-v1.review-v3.md),
[board schema](../../../../spec/board-export.schema.json), and
[fixtures](../../../../spec/fixtures/board-export.review-v3.json).
Exact hashes/line counts are preserved in [review state](review-state.md).

## Decision

**Keep the direct browser-to-gateway architecture. Do not yet label M1–M5
implementation-ready.** The requested specification work is applied and the
fresh re-audit is complete. This review raises nine Major and three Nit findings.
All reviewers completed; no failed/missing reviewer is counted as clean.

The architecture remains supported by the recorded single-gateway experiment.
No new connectivity experiment, production implementation, build or runtime
verification was performed. Several interface and phase dependencies still force
an implementer to make design decisions or discover omitted changes.

Of the prior eight groups, four are resolved at specification level (V2-03, 05,
06, 08) and four partially resolved (V2-01, 02, 04, 07). Their original severities
remain unchanged in the [v2 report](../browser-hub-v2/implementation_readiness_report.md).
New Major severity assignments do not retroactively downgrade partially resolved
historical Blockers. The earlier four-Blocker design debt is not declared cleared
merely because no new finding uses that label.

## What was specified

The [applied resolution](goldfish_review_report.md) maps all 17 original findings
to the revised implementation instructions. Added detail includes:

- Private transport/session package ownership, public renderer/transport APIs,
  WASM dependency pins, native/hosted build separation and file-scoped phases.
- Atomic snapshots, u64 normalization, immutable source/Surface lifetimes,
  cancellation/join, concrete admission limits, budgets and most deadlines.
- IndexedDB records/transactions, one-owner lock, exact bootstrap and enrollment
  recovery policy; PNG resource validation and unsupported-media behavior.
- Strict one-board schema, semantic rejection fixtures, browser/native repository
  contracts and import-as-copy; gateway queued-origin revocation barrier.
- Static delivery/header/browser contract and measurable real-browser acceptance.

These are future implementation contracts, not implemented production behavior.

## Actual reviewer results

| Reviewer | Completion | Findings | Report |
| --- | --- | --- | --- |
| Implementation readiness | Completed, required reads complete | 5 Major, 1 Nit | [Readiness](readiness_report.md) |
| Codebase details | Completed, required reads complete | 4 Major | [Codebase](codebase_report.md) |
| Style | Completed, required reads complete | 2 Nit | [Style](style_report.md) |

Each used a new `fork_turns:none` context and verified all four target hashes.
Readiness/codebase used the Implementation Readiness Standard; style used its
persona only. No reviewer modified files, ran tests/builds, or delegated work.
Style's original verdict says not clean because of Nits; under the loop rule
Nits are nonblocking, so style is clean for that criterion. The round as a whole
is not clean because of the nine Majors. No convergence is claimed.

## Remaining implementation decisions

All nine are **PROPOSE FIX**; none are contested, escalated or accepted risks.
They are not duplicate reports of the same defect.

| ID | Severity / raw finding | Required correction | Prior group |
| --- | --- | --- | --- |
| V3-01 | Major / readiness-v3-r2-1 | Define one authoritative synchronous renderer apply-result path, separate from observer notification. Failed commit must retract only its service, abort leases and prevent ready/actions | V2-01, V2-02 |
| V3-02 | Major / readiness-v3-r2-2 | Expose all profile/enrollment states; add local acceptance-commit retry and explicit identity-reset/recovery operations, including corrupt-open failures | V2-04 |
| V3-03 | Major / readiness-v3-r2-3 | Specify source/service Retry API, cleared terminal states, backoff/generation behavior and preservation of the unaffected gateway | V2-02 |
| V3-04 | Major / readiness-v3-r2-4 | Bound service open/first snapshotStart and frames already in progress; keep healthy ready-stream idle unlimited; specify service-only failure/cleanup/retry | V2-02 |
| V3-05 | Major / readiness-v3-r2-5 | Give M1 its own production-code browser entry/build or move minimal hosted gallery/config into M1; name prerequisites for session-mode execution | V2-01, V2-07 |
| V3-06 | Major / codebase-v3-r2-1 | Add release.ts selector change/tests excluding private 0.0.0 build inputs from SDK version coordination, retaining native exclusions and private dependency restrictions | V2-01, V2-07 |
| V3-07 | Major / codebase-v3-r2-2 | Declare renderer's public core dependency; build core first in root native and package-native.py paths; add clean-output and isolated consumer checks | V2-01, V2-07 |
| V3-08 | Major / codebase-v3-r2-3 | Add pure existing-secret identity derivation to WASM/FFI, with validation/errors and no-endpoint-open assertions | V2-04 |
| V3-09 | Major / codebase-v3-r2-4 | Choose concrete bounded A2uiSnapshot API/implementation and pre-copy replay sizing/reservation/rollback algorithm; add source files and boundary cases to M1 | V2-02 |

Source locations and detailed rationale are preserved in the raw reviewer reports.
In particular, existing test:packaging does not call loadRelease; it cannot be
claimed to cover V3-06. The package changes also affect independently installed
renderer declarations, not only a workspace with old dist output.

Three nonblocking corrections remain:

| Raw finding | Severity | Correction |
| --- | --- | --- |
| readiness-v3-r2-6 | Nit | Give duplicate-panel-id fixture a distinct second Surface reference, isolating ID uniqueness from reference uniqueness |
| style-v3-r2-1 | Nit | Say board IDs/revisions/timestamps are excluded; panel and source-reference IDs remain |
| style-v3-r2-2 | Nit | Point transport constructor prose to section 3, not methods “above” |

## Verification and boundary

The parent checked local document links and exact preservation of the v2 archive
hash. Draft 2020-12 meta-schema validation passed; two valid fixtures pass and
nine structural-negative fixtures reject. Nine semantic-only negatives intentionally
pass the structural schema and await the future codec. This is not a semantic
validator or runtime test pass; the duplicate-ID fixture isolation Nit remains.

No target changed during or after the re-audit. Production integration is still
planned. This completes the user's requested detailed-specification update and
one fresh re-audit. The current Stage 9 re-audit allowance is consumed. Remaining
findings require further specification edits or explicit accepted-risk decisions;
do not silently start another review or mark the plan ready.
