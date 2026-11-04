# Native gateway protocol v4

This is the breaking Rust gateway/hub contract. SDK Unix JSONL, A2UI v0.9 and
browser HTTP/WebSocket contracts remain separate. The executable definitions are
in `crates/protocol/src/{lib,session}.rs`; enrollment is in
`crates/transport/src/pairing.rs`.

## Identity and connectivity

NodeId is the iroh EndpointId encoded as 64 lowercase hexadecimal characters.
Gateway and hub have separate durable private keys. The authenticated iroh peer
identity, together with its registered role, controls authorization. An advertised
name, address, mDNS record or application-supplied ID never grants access.

Remote access is opt-in. Online release builds use `https://relay.omochi.fyi` with
shared-token relay admission; debug builds default to n0 public relays. Both use
n0 public address lookup. Custom relay URLs replace the relay set; the built-in
admission token is attached only to the production relay, never to other URLs or
advertised endpoint addresses. Relay admission does not grant application access.
Offline mode uses direct paths and LAN mDNS without public lookup or relays.
An offline registered peer does not block the
hub's local dashboard. All registered gateways reconnect independently with
bounded backoff and jitter. IP addresses remain internal addressing information.

## Enrollment

The gateway emits `omochi://invite/<unpadded-base64url-JSON>` containing `version: 4`,
iroh `address`, a random bearer `secret`, and Unix-seconds `expires`. The link is
imported on the hub PC and is bounded to 16 KiB. Invitations last 180 seconds;
the first authenticated hub to redeem one wins. The same hub may retry, but a
second identity cannot redeem the consumed secret. Expiry still applies.

Enrollment uses ALPN `omochi/pair/1`, separate from ordinary gateway traffic.
The first bidirectional stream carries `{version, secret, name}` and receives
`{accepted}`. The gateway reads at most 4096 bytes for the request, with a
10-second exchange timeout. It records the iroh-authenticated remote identity,
not an ID from the request. The durable write happens before success is sent.
The hub persists the invitation's authenticated gateway identity after success.

## Framing and streams

Ordinary traffic uses ALPN `omochi/gateway/4`. Each frame is a nonempty UTF-8 JSON
body preceded by its unsigned 32-bit big-endian byte length, at most 4 MiB.
Partial headers/bodies are errors. Readers must not cancel a partial read unless
they discard that entire stream. The local Unix gateway uses the same framing.

The first frame on each bidirectional stream is one of:

- `{"kind":"control","protocol":4}`
- `{"kind":"service","service":"app","generation":1}`
- `{"kind":"resource","service":"app","generation":1}`

Unknown fields in these envelopes and unsupported protocol versions are rejected.
A generation identifies one live SDK service instance; stale-generation requests
are rejected. Reconnection requires a fresh snapshot, and actions are not replayed.

The control stream starts with `hello` (`protocol`, `nodeId`, `name`), followed by
`services` containing the complete list of `{id,name,generation,icon?}`. Lists are
limited to 256 unique valid service IDs. The hub sends `ping` with a numeric `id`
every 10 seconds after the preceding exchange; the gateway returns matching
`pong`. A blocked write or missing matching pong for 10 seconds ends the session.
`bye` carries a `reason`. Control loss retracts the associated services.

Discovery reserves space for the complete serialized roster within one 4 MiB
frame, including JSON escaping, UTF-8 and the maximum generation width. Unchanged
admitted manifests retain capacity first, followed by changed admitted manifests
and then new IDs (lexical ID order within each group). Manifests that exceed the
remaining byte or service-count capacity are omitted and reconsidered on later
scans. An oversized metadata update retracts only that service; unchanged services
remain connected. Removal or smaller metadata can release capacity for waiting IDs.

A service stream emits `snapshotStart`, zero or more `message` frames carrying an
A2UI `message`, then `snapshotEnd`, followed by incremental `message` frames.
The reverse direction carries A2UI client actions/errors. Hub browser surface IDs
are `<nodeId>~<serviceId>/<surfaceId>`; SDK-facing IDs remain unchanged. Client
model metadata is restricted to the target service before forwarding.

A resource stream carries one existing `resources/1` request and response. It is
separate from snapshot state. The browser boundary preserves resource GET/HEAD,
ETag and byte-range semantics; remote resources require a live authorized peer.

## Limits and lifecycle

Stream admission reserves separate budgets for control, service subscriptions,
resource requests and headers awaiting classification. An authorized remote
connection permits one control stream, 256 service streams, 64 resource streams
and 64 pending headers. Its QUIC bidirectional-stream credit covers all 385;
raising this credit happens only after gateway role authorization. The Unix
listener shares budgets across clients: eight control streams, 2048 service
streams (eight complete hub rosters), 64 resource streams and 64 pending headers.
Subscriptions cannot consume the resource or control reservations. Exhausting a
budget rejects that stream, while existing streams continue. Header reads still
expire after five seconds, and resource concurrency and request timeouts remain
bounded separately from long-lived subscriptions.

Snapshots have a 16 MiB encoded-data limit. Every materialized gateway replay
message must also fit the 4 MiB frame including its envelope; individually valid
patches that exceed this limit after merging cause the service to be retracted.
Data-model paths reject prototype keys, excessive depth and combined numeric
indices above 100,000 before applying sparse array patches.

Fanout buffers bound both message count and encoded bytes (16 MiB). A lagging
subscriber reconnects for a fresh snapshot. Action/publication queues retain an
8 MiB budget through processing. These are wire-data bounds, not total RSS caps.

Trust writes are serialized across processes, re-read under lock and committed
atomically. `trust forget` waits for every registered live process to reload the
authorization generation and destroy revoked sessions. Failure to acknowledge is
an error; it does not roll back the durable removal. A revocation watermark forces
fresh sessions even if the same peer is immediately re-enrolled. Native state is
isolated under `omochi/rust-v1`; old trust and dashboards are not migrated.

## Web invitation handoff

`gateway --remote --pair` also prints `https://app.omochi.fyi/#g:<payload>`
and a terminal QR containing that URL. `--web-url` overrides the HTTPS base.
Both invitation forms contain the same unpadded base64url UTF-8 JSON; there is
no nested encoding. CLI and native invitation import accept either form.
Old hex and `#connect:` invitations are not accepted. Existing trust is unchanged.

The hosted page clears the fragment before loading transport modules. It shows
the gateway ID, expiry and target owner hub, and requires **Add to this hub**.
A shared viewer cannot register gateways. A new tab hands the invitation to the
profile-owning tab using an ephemeral same-origin BroadcastChannel with an
acknowledgement. Bearer data is never persisted or included in status responses.

Owner control methods `gatewayPairStart` (`invitation`, `name`),
`gatewayPairStatus` (`id`) and `gatewayPairRetrySave` (`id`) return
`{id,status,gatewayId}`. One enrollment/save-pending job is allowed per hub.
Status is `pairing`, `registered`, `failed`, `outcomeUnknown` or `savePending`.
Jobs belong to their initiating owner grant and authenticated browser identity.
Completed results expire after five minutes. Accepted enrollment proofs stay in
hub memory for local save retry without redeeming the invitation again, until
saved, revoked or shutdown. Trust revocation invalidates stale writes.
