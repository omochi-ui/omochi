# Applied v3 finding resolutions

Status: applied edit batch; re-audit complete with five open findings.
User authorization: “修正してレビューして”. Date: 2026-11-03.
Accepted PROPOSE FIX: nine Major + three Nit. Contested/escalated/declined: zero.
No original severity is changed. Source findings and rationale are preserved in
[v3 combined report](../browser-hub-v3/implementation_readiness_report.md) and its
linked raw reports. Original targets are archived with matching frozen hashes.

| Finding | Severity | Applied replacement / implementation contract |
| --- | --- | --- |
| readiness-v3-r2-1 / V3-01 | Major | Hub §2 splits authoritative RenderEvent apply sink from observer LiveEvent notifications; explicit result/exception handling, readiness ordering and partial-state service-only cleanup |
| readiness-v3-r2-2 / V3-02 | Major | Hub §5 adds immutable all-record ProfileView, ProfileRecovery, local-only acceptance retry, connectSelected and reset preserving boards, including corrupt-open recovery |
| readiness-v3-r2-3 / V3-03 | Major | LiveSession.retry/ProfileOwner.retryConnection define service/gateway scopes, new instances/epochs, counter resets, admission, coalescing and unaffected gateway preservation |
| readiness-v3-r2-4 / V3-04 | Major | Hub §§3–4 adds 10s service-open/first-start and 5s first-byte frame deadline; idle stays unlimited, abort/join/releases and service-only retry explicit |
| readiness-v3-r2-5 / V3-05 | Major | M1 creates separate browser-test HTML/entry/Vite/TS config and SessionGallery; exact independent build/test aliases, memory enrollment and artifact prerequisites |
| codebase-v3-r2-1 / V3-06 | Major | Hub §9 and M1 add sdkWorkspaces public/non-native selector, release.ts/tests map, private/runtime restrictions and release:check checkpoint |
| codebase-v3-r2-2 / V3-07 | Major | Renderer public core dependency; core-first root/native-package builds; clean typecheck graph, isolated packed consumer and manifest/build-order tests |
| codebase-v3-r2-3 / V3-08 | Major | Pure identityFromSecret API with exact seed/ID/error rules and zero-endpoint verification tests |
| codebase-v3-r2-4 / V3-09 | Major | New browser-snapshot-v1 contract: browser-only snapshot/json-size APIs, virtual replay sizing, transient reserve/materialize/swap, rollback, detached delivery, sparse-array semantics, paths and boundary tests |
| readiness-v3-r2-6 | Nit | Duplicate panel-ID fixture now has a distinct second Surface reference and non-overlapping geometry |
| style-v3-r2-1 | Nit | Export excludes board IDs, revisions and timestamps; panel/reference IDs explicitly preserved in main and contract |
| style-v3-r2-2 | Nit | Transport facade reference points to section 3 |

Ancillary consistency: local/hosted/test typecheck graphs and direct M1 media stubs
are explicit; shared A2uiSnapshot/native semantics are unchanged. M1 static host
uses the exact header contract; M5 verifies/distributes rather than inventing it.

Validation: schema meta-validation and two positive/nine structural-negative
fixtures pass. Nine semantic-only negatives await the planned codec; duplicate-ID
fixture was separately checked to isolate ID from reference/overlap violations.
Document links/fences and preserved baseline hashes are checked. No production
code/build, semantic codec or runtime test is claimed. Review result is pending.

## Re-audit disposition

[Completed result](implementation_readiness_report.md): three Major and two Nit,
all PROPOSE FIX, zero contested/escalated/declined/accepted-risk findings. No new
target edits applied. Prior corrections are eleven resolved and one partial under
the combined conservative assessment; raw persona differences are preserved.
Current targets retain the dispatch hashes. No further review begins automatically.
