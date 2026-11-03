# Implementation readiness audit — browser hub v4, round 3

Reviewer: browser_v4_readiness_r3, fresh-context, read-only.
Standard: EGM Implementation Readiness Standard.
**Completed; not clean: two Major, no Blocker or Nit.**

The repairs substantially improve actionability. Two profile orchestration rules
still require decisions. This does not challenge the recorded browser-to-gateway
feasibility result or request deferred capabilities. No edits, builds/tests or
delegation by the reviewer.

## readiness-v4-r3-1 — Major — Accepted-ID invitation refresh lacks a public operation

Affected: `spec/browser-hub-v1.md:490,521–528,554–563,595–598`;
M2 `docs/dashboard-remote-access.md:426–431`.

Accepted-ID link handling requires ordinary authentication and valid hello before
committing new relay hints, then offering invite redemption if access is rejected.
Only ProfileOwner.pair accepts an invitation, but its instructions always describe
temporary pairing/enrollment/session restoration. connectSelected/retryConnection
cannot receive new invitation hints, and retry explicitly cannot change relays.
Nothing distinguishes authenticated refresh from user-selected redemption.

An implementer must invent an overloaded state machine, new API, or automatic
redemption. Calling the same method again after denial has no discriminator to
prevent another ordinary refresh attempt.

Repair: define the public accepted-record branch. Specify authentication through
temporary invite hints, hello-gated hint persistence and restoration on failure.
Add explicit user-selected redemption invocation, retained invite lifetime,
public error/state and reconstruction. Test successful refresh without pairing,
failure preserving accepted data, and explicit redemption after NotAuthorized.

## readiness-v4-r3-2 — Major — Valid enrollment inputs can exceed relay-union admission

Affected: `spec/browser-hub-v1.md:429,453–454,461–464,521–526`;
M2 `docs/dashboard-remote-access.md:426–429`.

Two selected gateways may each have four relays and an invitation independently
has four. The endpoint nevertheless must use selected-plus-pending union <=8.
Eight selected distinct URLs plus 1–4 disjoint invitation URLs gives 9–12. Pairing
stops connections but does not remove selected records from that prescribed union.
There is no reject/truncate/selection-change/temporary-configuration admission rule.

Repair: separate temporary enrollment/refresh relay sets from ordinary live sets.
Since old endpoint already stops, use only the invitation's validated 1–4 URLs
for temporary endpoint, close it, then restore selected-record union (naturally
<=8). Reconcile union/restart text. Test two four-relay selected gateways plus
four disjoint invitation relays, including restoration on failure.

## Prior twelve findings

Original severities preserved; resolved means specified, not runtime verified.

| Prior ID | Severity | Assessment |
| --- | --- | --- |
| readiness-v3-r2-1 / V3-01 | Major | Resolved: authoritative sink/result, ready ordering, observer isolation and service cleanup (hub 70–114,195–207) |
| readiness-v3-r2-2 / V3-02 | Major | Substantially resolved, partial workflow completeness: all records, Recovery/save retry/connect/reset specified (466–573); accepted refresh API remains finding 1 |
| readiness-v3-r2-3 / V3-03 | Major | Resolved: Retry lifetime/admission/cleanup/generations/counters/coalescing/isolation (338–354,558–563) |
| readiness-v3-r2-4 / V3-04 | Major | Resolved: first/open/partial frame deadlines, idle/backoff/disposal (286–291,392–412) |
| readiness-v3-r2-5 / V3-05 | Major | Resolved: M1 independent entry/build/gallery/enrollment/prerequisites (main 331–332,355–420; hub 799–820) |
| codebase-v3-r2-1 / V3-06 | Major | Resolved: exact release selector, guards, regressions and release:check (778–784) |
| codebase-v3-r2-2 / V3-07 | Major | Resolved: public core/native order/type graphs/packed consumer/checks (771–797,822–831) |
| codebase-v3-r2-3 / V3-08 | Major | Resolved: pure derivation/validation/zero-seed/mismatch/no-endpoint rules (258–266) |
| codebase-v3-r2-4 / V3-09 | Major | Resolved: full bounded snapshot API/sizing/preflight/reservation/rollback/delivery/semantics/tests contract |
| readiness-v3-r2-6 | Nit | Resolved: distinct second reference and non-overlap in duplicate-ID fixture (231–242) |
| style-v3-r2-1 | Nit | Resolved: explicit board-ID exclusion, panel/reference retention (main 220–221,471; hub 706–709) |
| style-v3-r2-2 | Nit | Resolved: facade reference to section 3 (hub 125) |

Historical identity/profile group is not wholly cleared while finding 1 remains.
Relay admission is an additional concrete decision. Other personas' assessments
may identify source issues beyond this readiness assessment.

## Reads, integrity and verdict

Read fully: readiness persona/standard; main 504 lines; hub 928; snapshot 182;
schema 64; fixtures 361; v4 state/resolution; v3 synthesis/readiness; README;
gateway-v4; experiment README/RESULTS/recorded result. All five frozen hashes and
line counts matched [manifest](review-state.md). Also inspected core snapshots/
routing, renderer client/images, hub-ui contracts/services/configs, manifests,
release/native build, gateway authorization/commands and native pairing.
Missing/inaccessible required reads: none. Future Create paths need not exist.

Clean requires no unsettled Blocker/Major. This round is not clean due to the
two Majors. No runtime implementation, semantic codec execution, acceptance pass
or test success is claimed by the audit.
