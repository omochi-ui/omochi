# Remote owner connection v1

A remote owner operates the native Hub through the same dashboard and sharing
operations as the local HTTP owner. Gateway trust and existing dashboard share
protocols are unchanged. Owner enrollment and revocation remain local-only.

## Enrollment and authority

The local `/api/owners` endpoint lists grants (GET) or issues a registration link
(POST). DELETE `/api/owners/{id}` revokes a grant and waits for live listener
teardown acknowledgements. Existing loopback defaults and Origin middleware
apply. Registration requires an enabled remote listener and available relay
address. `#owner:` carries base64url JSON with
`{version:1,hubId,relayUrls,grantId,mode:"owner",secret}`. It is removed from
history before loading UI/transport modules.

Enrollment uses `omochi/owner-enroll/1`; ordinary traffic uses `omochi/owner/1`.
The first framed JSON message is `{version:1,grantId,secret?}`. Enrollment binds
the first authenticated EndpointId presenting the secret within 180 seconds.
Ordinary access requires that exact ID; passing the invitation secret alone
never authorizes ordinary traffic. Bound access survives invitation expiry.
Owners are stored separately in `owners.json`, with a digest instead of the
invitation secret, locked transactions and atomic replacement. No share or
Gateway record is promoted to owner.

## Operations and live data

The ordinary welcome is `{kind:"ownerWelcome",services:[...]}`. Live Surface
messages, leases, snapshotEnd, ACKs, image validation and limits reuse dashboard
sharing's data plane, but cover all Hub Surfaces. Ping includes the current
service roster. Actions require a current connection-bound lease.

Owner control requests are `{op:"control",command:{method,...}}`; replies are
`{kind:"result",value}` or fixed error codes. Methods are `boardsList`,
`boardGet`, `boardCreate`, `boardUpdate`, `boardDelete`, `presets`, `connections`,
`sharing`, `sharesList`, `shareCreate`, and `shareRevoke`. Board commands carry
`id` where applicable and `value` for writes; updates/deletes retain revision
checks. Share creation carries `board_id`, `public`, and existing `permissions`.
There are no remote owner-administration commands. Share connections reject all
control requests. These methods use HubControl, also used by local board and
share handlers; no HTTP proxy or expanded CORS policy is involved.

One listener accepts all four browser ALPNs. Separate owner and share epochs
ensure revoking a share does not close the owner awaiting its response. Owner
revocation waits for all registered processes to join old sessions before
acknowledging. Failure remains an error after durable removal. Pending control
writes recheck owner authority under the owner lock. Actions are never replayed
on reconnect, and completed actions cannot be recalled.

## Browser lifecycle

Owner connections reuse the durable browser identity and pending/accepted
recovery flow. An optional `owner:true` distinguishes saved owner records from
legacy share records; absence always means share access. Secrets are not saved.
Several Hub records may be saved; only one identity connection is active at a
time. Switching joins the previous session and confirms discard of unsaved
edits. Local forget/reset does not revoke server-side authority.

Board adapters poll committed board state once per second for invalidation,
including cross-process changes; requests do not overlap, late results after
unsubscribe are ignored, and editor drafts are not replaced. Revisions detect
conflicting writes. Local HTTP/WS/SSE remains usable offline without enrollment.
