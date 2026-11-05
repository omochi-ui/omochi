# Hub dashboard sharing v2

This is the downstream hub contract. It does not change gateway v4 or SDK
protocols. Local HTTP/WS/SSE remains loopback by default and keeps the common
Origin policy. The browser-resident gateway aggregator remains an experiment.

## Authority

Each share names one saved board and authenticates a reusable bearer credential.
Anyone holding a forwarded link has the same authority. No recipient identity is
enrolled. `interact` and `editLayout` are independent permissions, both false by
default. Layout editing permits names, titles, positions and sizes, but never
adding/removing panels or changing their IDs or source references. All saves use
native revision conflicts and validation. Permissions are authoritative Hub
records, never client URL assertions. To change permissions, revoke and reissue.

The saved board determines the current Surface set. Owner membership edits take
effect on existing viewers through a fresh subscription. Board deletion denies
access. Share records, credential digests and process acknowledgements are
separate from gateway/native trust. No gateway enrollment is delegated.

`shares.json` is version 2, with a monotonic generation and at most 1000 records /
1 MiB. Mutations re-read under `shares.lock` and atomically replace mode-0600
storage. Only a SHA-256 digest of a link credential is persisted. A live listener
registers a flock-backed `share-live-*.lease` before accepting connections. Owner
revocation succeeds only after registered live listeners acknowledge teardown;
timeout is an error, without rolling back removal.

## Transport

The sharing ALPN is `omochi/dashboard/2`; there is no sharing enrollment ALPN.
It uses nonempty UTF-8 JSON, four-byte big-endian length framing and a 4 MiB
frame limit. The first client-initiated bidirectional stream starts with exactly
`{version:2,grantId,secret}`, limited to 16 KiB. Authentication/stream-open and
partial-frame deadlines are 10 seconds. The secret must match the stored digest
on every connection, regardless of the client's EndpointId.

Authenticated ordinary connections receive `{kind:"welcome",board,permissions}`
on the first stream. Only that board is returned. The stream then delivers
`{kind:"message",message,lease,sequence}` or `{kind:"snapshotEnd",sequence}`.
Sequence is a canonical decimal u64 string. Every delivered frame requires
`{ack:sequence}` after authoritative renderer application before the next frame.
Filtering occurs before serialization; sequence gaps caused by filtered frames
are intentional. Leases are opaque, connection-bound and Surface-lifetime-bound.
A snapshot is frozen at the subscription cursor. Snapshot completion still occurs
when no source is available.

Other bidirectional streams accept one request, followed by a response and FIN:

- `{op:"ping"}` replies `{kind:"ok"}`. No ping for 45 seconds closes the session.
- `{op:"action",lease,message}` requires `interact` and a live permitted lease.
  The hub overwrites the destination and narrows metadata using SurfaceHandle.
- `{op:"layout",value}` requires `editLayout`. Value is native board input with
  revision; the response is `{kind:"board",board}`. Conflicts return
  `{kind:"error",code:"Conflict"}`; denied operations return `NotAuthorized`. Invalid layout input returns `InvalidBoard`, missing boards return `NotFound`, and storage failures return `StorageUnavailable`.
- `{op:"image",lease,uri}` requires a current publication in that Surface's
  authoritative source state. Canonical complete URI strings publish images;
  constructed/embedded Markdown destinations do not imply publication. Native
  image fetch/validation policy is retained. Replies contain `imageChunk` frames
  with MIME, width, height and at most 24576 bytes encoded as standard base64,
  ending with `{kind:"imageEnd"}`. Publication removal cancels active fetching.
  Arbitrary client-provided resource IDs/URLs are never permission declarations.

There are at most 16 connections per listener and 8 in-flight request streams per
connection. Native image concurrency/byte/pixel limits and scoped snapshot queue
limits continue to apply. Requests and actions are never replayed after loss.
Connection loss or source replacement invalidates leases. Changes to the board
revision close the connection for fresh authorization and snapshots. Application
close code 1 means authorization denied; 2 requests a fresh connection. A grant
generation change retires old sessions before the listener acknowledges it.
Already delivered bytes and dispatched app actions cannot be recalled.

## Owner and browser entries

`omochi-ui hub --remote` opts in to
the downstream listener, including on hubs without registered remote gateways.
`--remote` conflicts with `--offline`; local WS remains usable without relays.
The web base defaults to `https://app.omochi.fyi/`; `--web-url` overrides it.
The web base contains no credentials/query/fragment. It is not inferred from the
local HTTP origin. Static UI hosting is separate from hub operation.

Local owner endpoints (under the existing Origin middleware):

- GET `/api/sharing`: configured hub ID, relay hints and static UI base, or
  `{enabled:false}`. No credentials.
- GET `/api/shares`: grant metadata, with no credential or digest.
- POST `/api/shares`: `{boardId,permissions:{interact,editLayout}}`; returns
  `{grant,secret,url}`. A usable URL requires relay hints to be available.
- DELETE `/api/shares/{id}`: durable removal plus live teardown acknowledgement.

Links use `#share:<base64url-json>` with
`{version:2,hubId,relayUrls,grantId,mode:"share",secret}`. These are hub
sharing links, not gateway invitations. The intended authenticated hub is always
the exact `hubId`. The browser must not fall back to a static-host API or WS when
an iroh connection fails.

## Upgrade and client storage

All v1 sharing links and registered identity grants are retired. Under the share
lock, a recognized v1 store is replaced with an empty v2 store and an incremented
generation. Unknown or malformed storage remains an error. Reissue all links.
Owner v1 links, owner enrollment and Gateway trust are unaffected.
Native profile loading validates and removes recognized v1 `invite`/`public`
records with compare-and-exchange, preserving the existing identity, owner
records and v2 shares. Unrecognized or malformed profile data remains an error.

Browsers keep bearer context in tab-scoped sessionStorage, without enrolling or
saving an identity grant. Native clients retain bearer credentials in protected
storage only when explicitly saved. Bearer secrets are removed from URL history
before loading the application; they are never returned in share listings.

## Verification

Native tests cover permission combinations, authentication, layout revision and
scope restrictions, credential persistence, revocation and the v1 store reset.
Browser acceptance exercises readonly and enabled links, forwarded access,
reload, images, relay recovery, layout editing and independent owner enrollment.
See [sharing guide](../docs/dashboard-sharing.md).
