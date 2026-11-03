# Final correction disposition

Status: resolved — corrections applied and final fresh review completed.
User authorization: current request to fix and review. All original v4 findings
resolved in the [v5 auditor reports](../browser-hub-v5/readiness_report.md), with
only style-v5-r4-1 Nit. Parent identified the counterexample below before claiming
final acceptance, so no full convergence/pass was inferred from those results.

## parent-v5-1 — Major — Missing initial root differs from renderer default

Source: installed DataModel constructor initializes {}; core A2uiSnapshot create
stores undefined/hasData:false. The new planner reads the live root, so first /0
without a prior data root creates {"0":"x"} live while a server snapshot creates
["x"]. A deterministic installed-processor/core-oracle probe confirmed both values.

PROPOSE FIX, applied: direct renderer payload inserts a root-removal message
immediately after each replay create, with its bytes admitted before construction.
Ready create uses applyDirectCreate with scratch reservation and one synchronous
[create,rootRemoval] batch. No action/notification occurs between them. Later real
root overrides initialization. Native default remains unchanged. Add first /0,
/a/b, deletion and delete/recreate/explicit root cases plus exact/+1 budget tests.
No new shadow model, Surface ownership role or protocol change.

A second mechanism probe confirmed installed-processor root becomes undefined
and then ['x'] after normalized first /0, matching server snapshot. This tests
existing machinery only, not the proposed new helper implementation.

## style-v5-r4-1 — Nit — Core manifest path

PROPOSE FIX, applied: main table now explicitly names packages/core/package.json.
The exact reviewed v5 main is archived before that isolated path edit. Current
v6 includes the initialization contract too. Historical hashes are preserved.

Both corrections are within the authorized spec repair, with no contested,
escalated, declined or accepted-risk finding. Fresh review determines adequacy.

Final review: all three reviewers completed, no new findings, both corrections
resolved at specification level. [Combined report](implementation_readiness_report.md).
Unaddressed findings zero; no production implementation/runtime claim.
