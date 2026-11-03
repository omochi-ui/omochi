# Applied v4 finding resolutions

Status: resolved edit batch; fresh review pending.
Authorization: user explicitly requested fix and review. Date 2026-11-03.
Accepted PROPOSE FIX: three Major/two Nit. Contested/escalated/declined: zero.
Original severities preserved in [v4 report](../browser-hub-v4/implementation_readiness_report.md).

| Prior finding | Severity | Applied instructions |
| --- | --- | --- |
| readiness-v4-r3-1 / V4-01 | Major | Separate refreshInvitation, retryInvitationRefresh and redeemInvitation; explicit bootstrap dispatch, memory-only capture/public summary, auth/hello/roster-gated hint commit, user-selected pair fallback, expiry, unsaved proof and reconstruction rules |
| readiness-v4-r3-2 / V4-02 | Major | Temporary endpoints use only invite's 1–4 URLs, ordinary endpoint uses selected-record union <=8; stop/close ordering and disjoint-URL cases explicit, no truncation |
| codebase-v4-r3-1 / V4-03 | Major | Shared public core data-patch/json-size planner, direct renderer current-live-root normalization and root processMessages call; preserved unrelated drafts, authoritative root/reconnect policy, RenderBudget scratch admission, installed-processor parity/draft/native tests |
| style-v4-r3-1 | Nit | M2 package-root HTML/Vite/TS paths fully qualified |
| style-v4-r3-2 | Nit | Section 10 consumes existing board fixture |

The patch planner is shared across retained snapshots and direct rendering, not
imported from a private browser package by renderer. Source snapshots remain
server-only; partial renderer patches use current live state. No raw nested data
increment reaches the direct processor. Scratch + incoming delivery must fit the
existing single 16 MiB slot before cloning; no hidden second queue/pool.

Parent mechanism probe used the installed MessageProcessor, existing A2uiSnapshot
as patch oracle and a detached root update. Four assertions passed: array removal
shifts, primitive-parent replacement, unrelated unfinished draft retention and
client action-model metadata retention. This is evidence for the root-update
mechanism only; the new planner/adapter is not implemented/tested, and the existing
16-assertion browser experiment was not repeated or broadened.

Document links/fences and exact v4 archive hashes checked. Schema/fixture format
unchanged this round. No production build/runtime acceptance claim.
