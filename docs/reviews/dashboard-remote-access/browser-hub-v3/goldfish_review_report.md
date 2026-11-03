# Applied specification resolutions

Status: prior resolution batch applied; re-audit completed with new findings open.
Date: 2026-11-03. Authorization: user requested “不足する仕様を具体化して再レビューして”.
Accepted: 8 groups / 17 raw findings. Contested: 0. Escalated: 0. Declined: 0.
Unaddressed prior groups: 0 at editing stage; fresh review determines adequacy.

Baseline is the [v2 report](../browser-hub-v2/implementation_readiness_report.md)
and [archived target](../../../dashboard-remote-access.browser-v2.md), SHA-256
`4f989994ec084d39d0e70393c01a36a47890fc5fd75bf168073d03b025cedcc6`.
Original severities and raw identifiers remain in that report.

| Group | Severity | Decision | Applied changes |
| --- | --- | --- | --- |
| V2-01 | Blocker | PROPOSE FIX, applied | Contract §§1–2/9: private packages, version pins, pure core export, exact live/renderer/transport APIs, native-compatible wrappers and build graph; main file map and stable M1–M5 checkpoints |
| V2-02 | Blocker | PROPOSE FIX, applied | §§3–4/10: strict wire/u64 bridge, atomic snapshot commits, generation/lease capture, abort-and-join, source-local failures, numerical budgets/deadlines/retries and deterministic checks |
| V2-03 | Major | PROPOSE FIX, applied | §3 and M1: replace old TS node/channel bounds, retain bare SDK channels, explicit routing/metadata regression cases |
| V2-04 | Blocker | PROPOSE FIX, applied | §5 and M2: DB v1 exact records, transaction completion, owner lock, serialized ProfileOwner, bootstrap limits, uncertain/unsaved/reload enrollment recovery |
| V2-05 | Major | PROPOSE FIX, applied | §6 and M3: captured resource streams, strict PNG/chunk/inflation validation, budgets, scoped decoding, catalog overrides with no unsupported network src |
| V2-06 | Blocker | PROPOSE FIX, applied | §7, JSON Schema, 20 fixture cases and M4: exact exchange format/semantics, both repository APIs/CAS/errors, native/browser import/export |
| V2-07 | Major | PROPOSE FIX, applied | §§9–10, file map/checkpoints: static artifact/headers/TLS/CSP/cache, browser matrix, real two-source harness, fixed load/recovery criteria, explicit commands |
| V2-08 | Major | PROPOSE FIX, applied | §8 and M1: gateway queue command origins, cancellation checks, bounded dispatch barrier before revocation ACK, Local behavior and blocked-SDK regression |

Specification validation: JSON Schema draft 2020-12 meta-schema accepted; 2 valid
fixtures pass; 9 structural negatives reject. The 9 semantic-only negatives pass
the structural schema as intended and require the future codec. No semantic codec,
production build/test, or new browser experiment is claimed by this check.

Read the [main plan](../../../dashboard-remote-access.browser-v3.md) and
[contract](../../../../spec/browser-hub-v1.review-v3.md) together. Re-audit reports record
whether these edits fully resolve the earlier gaps and identify any new issues.

## Post-resolution re-audit

The fresh [re-audit](implementation_readiness_report.md) is complete. Four prior
groups resolve at specification level; four remain partial. Nine Major and three
Nit findings remain. All are PROPOSE FIX; none contested/escalated/declined.
Their IDs, locations and repair directions are in the report and raw reviewer
artifacts. No further target edits have been applied. Original finding severities
are retained; this report's earlier resolved status described the applied edit
batch, not a successful readiness verdict.
