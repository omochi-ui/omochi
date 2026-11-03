# Browser hub v5: auditor results and final-check follow-up

2026-11-03. All three fresh-context auditors completed, with all required inputs
read and five frozen hashes verified. Main reviewed FeatureRange 1–291,
DetailedRange 292–515; [exact reviewed main](../../../dashboard-remote-access.browser-v5-reviewed.md).

| Reviewer | Completion | Findings | Report |
| --- | --- | --- | --- |
| Implementation readiness | Completed | None; clean | [Readiness](readiness_report.md) |
| Codebase details | Completed | None; clean | [Codebase](codebase_report.md) |
| Style | Completed | One nonblocking Nit; clean | [Style](style_report.md) |

All five previous v4 findings were resolved at specification level. The one new
Nit, style-v5-r4-1, was applied after every auditor finished: qualify
packages/core/package.json. This isolated path correction changed main hash from
9376ddb9f97ba8bf5b88d8566250b3a34dd7935e4aa7688548f20513b0a71903 to
aa9a29e33bd6cf6a4373c1e02011788e77589bf26b94fd5ccbdff38512c17ae8.
The old main is archived exactly; the path edit is not represented as audited.

## Parent verification found a remaining initial-root case

Before finalizing, a deterministic existing-processor/core-oracle probe confirmed
that untouched renderer roots default to {}, whereas source snapshots start with
missing data. A first /0 update therefore produced {"0":"x"} from the live root
but ["x"] from the source root. The parent recorded parent-v5-1 as Major rather
than inferring final acceptance from clean auditor reports.

This was corrected in the current proposal: synthetic root removal immediately
after direct creation, both in staged renderer payload and ready create, with
preflight/scratch accounting and action/notification ordering. Native behavior
stays unchanged. The mechanism probe confirms root missing and first array value
after initialization, but proposed helper implementation remains future work.

The [v6 correction review](../browser-hub-v6/review-state.md) verifies this boundary
with new contexts before final disposition. Thus no full readiness/convergence
claim was made from v5 alone. This preserves the actual clean auditor results and
the parent's additional counterexample, rather than hiding either.

No production code, connectivity experiment or runtime acceptance is claimed.
