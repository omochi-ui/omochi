# Browser hub v4: requested fixes and additional re-audit

Completed 2026-11-03. The author's “fix and review” (translated from Japanese) explicitly authorized
one additional repair/review beyond the earlier Stage 9 cap. This round used
three new fresh-context reviewers; all completed and all required reads succeeded.

Target: [main plan](../../../dashboard-remote-access.browser-v4.md), FeatureRange 1–291,
DetailedRange 292–504, with full [hub contract](../../../../spec/browser-hub-v1.review-v4.md),
[bounded snapshot contract](../../../../spec/browser-snapshot-v1.review-v4.md),
[board schema](../../../../spec/board-export.schema.json) and
[fixtures](../../../../spec/fixtures/board-export.json).
Line counts/hashes are frozen in [review state](review-state.md).

## Decision

**Not implementation-ready: three Major and two nonblocking Nit findings remain.**
Keep the browser-to-gateway direction supported by the recorded experiment.
This audit identifies no need for a mandatory server hub or per-Surface grants.
It is not a production implementation, runtime pass or public deployment claim.

The requested nine Major and three Nit edits are applied. Across reviewers,
eleven previous items are resolved at specification level; V3-02's profile/API
coverage remains partial because accepted-ID invitation refresh has no executable
public invocation. Codebase considers the original narrower recovery-view gap
resolved; readiness identifies incomplete coverage of the broader profile workflow.
Both assessments are preserved, rather than treating either as absent.

V3-09's original missing bounded-admission API is resolved. The new live-renderer
patch mismatch is a distinct integration contradiction, not a claim that the
admission API is still unspecified. Historical severities are unchanged.

## Applied changes

[Resolution map](goldfish_review_report.md) covers all twelve prior IDs.

- Split observer notices from one authoritative synchronous renderer result;
  specify failed partial commit cleanup and service-only failure.
- Add complete profile/recovery views, local-only acceptance persistence retry,
  explicit connection restoration, identity reset preserving boards and pure
  existing-secret identity derivation without socket binding.
- Define independent gateway/service Retry and service-first/partial-frame
  deadlines, preserving quiet healthy streams.
- Add a standalone M1 test-only browser entry/build and the exact prerequisites;
  public core dependency, core-first native builds, private release selection,
  separate typecheck graphs and isolated package-consumer checks.
- Specify the browser-only bounded snapshot API, exact replay sizing, virtual
  preflight, reservation/materialization/rollback and detached delivery.
- Isolate the duplicate-ID fixture and correct ID-exclusion/cross-reference prose.

## Actual review completion

| Reviewer | Status | Findings | Report |
| --- | --- | --- | --- |
| Implementation readiness | Completed, all required reads | 2 Major | [Readiness](readiness_report.md) |
| Codebase details | Completed, all required reads | 1 Major | [Codebase](codebase_report.md) |
| Style | Completed, all required reads | 2 Nit | [Style](style_report.md) |

Each ran with fork_turns:none, read all five targets, verified frozen hashes and
was read-only. Readiness/codebase used the Implementation Readiness Standard;
style used its persona only. No failed/missing reviewer is counted as clean.
The style result is clean under Blocker/Major criterion. The round is not clean,
and no two-clean-round convergence is claimed.

## Open findings and proposed resolution

All five are PROPOSE FIX; none contested, escalated, declined or accepted risk.
No post-review target edits have been applied.

### V4-01 — Major — Accepted-gateway invitation refresh API

Raw ID: readiness-v4-r3-1. Scope: hub 490,521–528,554–563,595–598; main M2.
The policy says authenticate using new invitation hints, persist after valid
hello, then offer redemption on rejection. The only invite-taking operation,
pair(invitation), instead always redeems. Retry/connect cannot accept hints.

Define an explicit refresh invocation and a distinct user-selected redemption
invocation, including retained invite/error state, expiry and restoration.
Test refresh without pair traffic, failure preserving accepted data, and explicit
redemption after rejection. Original source rationale is in readiness report.

### V4-02 — Major — Temporary endpoint relay set admission

Raw ID: readiness-v4-r3-2. Scope: hub 429,453–454,461–464,521–526; main M2.
Two selected four-relay sources plus a disjoint four-relay invite produce twelve
URLs, while the selected-plus-pending union is capped at eight. Stopping sockets
does not remove selected records from that rule; no admission policy is stated.

Specify separate temporary and live sets: temporary enrollment/refresh can use
only invitation hints (1–4), close before restoring selected union (<=8). Reconcile
the general union prose and test maximum disjoint hints plus restoration failure.

### V4-03 — Major — Live renderer versus replay patch equivalence

Raw ID: codebase-v4-r3-1. Scope: main 319–322,398–420; hub 70–75,195–213,328–335;
snapshot 132–142,158–182. Core/Rust snapshots shift array removal and replace
primitive intermediates; installed renderer assigns undefined for array deletion
and throws through primitive values. Sending the same increment to both diverges.

For `{items:["a","b"]}`, deleting /items/0 leaves ["b"] in retained state but
[undefined,"b"] live; reconnect materializes a different model. Setting /a/b after
{a:1} is accepted by snapshots but fails live. Define a direct-renderer patch
normalization/adapter API and ownership, preserving unfinished client form data,
delivery budgets and native compatibility. Add actual installed-processor
live-versus-reconnect cases. Catching errors is not semantic reconciliation.

### Nonblocking editorial findings

| Raw ID | Severity | Correction |
| --- | --- | --- |
| style-v4-r3-1 | Nit | Fully qualify package-root hosted HTML/Vite/TS paths in M2 table; the current directory shorthand conflicts with its own rule |
| style-v4-r3-2 | Nit | Hub section 10 should consume the existing board fixture, rather than instruct creating it again |

## Validation and boundary

Parent checks: all local Markdown links/fences valid; original v3 plan, contract
and fixture archives exactly preserve their prior hashes. Draft 2020-12 schema
meta-validation, two positive and nine structural-negative fixture checks pass.
Nine semantic-only negatives intentionally require the planned codec; no semantic
runtime pass is claimed. Duplicate-ID case independently has distinct reference
and non-overlapping geometry. No production build/test was run for this doc change.

All five normative target hashes remain unchanged after reviewer dispatch.
The author-requested extra round is complete. Remaining findings need specification
resolution or explicit acceptance before readiness can pass; no further review
round, production implementation or deployment begins automatically.
