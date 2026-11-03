# Browser hub implementation contract v1

Status: proposed implementation instructions, 2026-11-03. This specifies future
code. The only completed runtime work is the isolated
[experiment](../experiments/browser-hub/RESULTS.md). The existing
[gateway v4](gateway-v4.md) remains the wire contract.

## 1. Scope, packages, and compatibility

The first supported direct client is desktop Chromium 156 or later on Linux,
with a secure context, WebAssembly, IndexedDB, Web Locks, AbortController,
DecompressionStream and createImageBitmap. Linux Chromium is the release test
target; other browsers and mobile are explicitly unsupported pending tests.
Missing facilities disable direct mode with `UnsupportedBrowser`; local/native
mode remains available. Do not infer compatibility from the version string alone.

Run WASM on the page's main thread, as in the experiment. Do not introduce a
worker in v1. One origin has one default browser identity and one endpoint owner.
Direct access is gateway-wide trusted-Hub access, not per-Surface authorization.
The browser may keep local Surface leases to stop stale work; these are lifecycle
references, not security capabilities.

Create two private Bun workspace packages, version `0.0.0`:

| Package | Ownership and dependencies |
| --- | --- |
| `packages/browser-transport`, `@omochi-ui/browser-transport` | Standalone Rust WASM workspace, JS facade, stream disposal. No native hub, SDK runtime, mDNS, fs or Node runtime imports |
| `packages/browser-hub`, `@omochi-ui/browser-hub` | Pure browser TypeScript session manager, profile storage, bootstrap and PNG resource transport. Depends on core's browser subpath and browser-transport; no React dependency |

Renderer owns MessageProcessor, catalogs, bitmap decoding and React components.
Hub UI owns repositories, board workflows and adapter selection. Export pure
interfaces/helpers at `@omochi-ui/core/browser`; never import core's Node-bearing
root in the direct bundle. Browser packages remain private build inputs; hub-ui
references them as devDependencies, not public runtime dependencies. They are
bundled into the hosted assets. Native Cargo versions and npm launchers do not change.

The WASM crate is `packages/browser-transport/Cargo.toml` with its own `[workspace]`,
lockfile and target directory, `publish = false`, and `cdylib` output. Pin iroh
`=1.4.1` with `default-features = false, features = ["tls-ring"]`, wasm-bindgen
`=0.2.130`, wasm-bindgen-futures `=0.4.80`, and commit the resolved lockfile. Use
futures-util Abortable/mutexes and serde/serde_json in that crate. Do not depend
on `omochi-protocol`: its inherited full native Tokio graph is unsuitable here.
Instead pin framing/control/session conformance with shared v4 fixtures.

## 2. Public interfaces and ownership

Define these public TypeScript interfaces in core's browser subpath. The code
below is normative; `A2uiMessage`/`A2uiClientMessage` are imported as types from
`@a2ui/web_core/v0_9`. `Processor` remains a renderer-owned type.

```ts
type Unsubscribe = () => void;
type ServiceKey = string; // opaque locally; direct mode uses <64-hex>~<serviceId>
type LeaseId = string;    // random UUID allocated locally, never serialized upstream
type ImagePolicy = 'native' | 'sdk-png';
type ClientMessage = A2uiClientMessage & {
  metadata?: { a2uiClientDataModel?: unknown };
};
interface ServiceView {
  id: ServiceKey; name: string; node: { id: string; name: string; local: boolean };
  status: 'connected' | 'disconnected' | 'error'; detail?: string;
}
interface SurfaceLease {
  id: LeaseId; surfaceId: string; signal: AbortSignal;
}
type LiveEvent =
  | { kind: 'status'; sourceId: string; state: string; error?: SessionError }
  | { kind: 'images'; enabled: boolean }
  | { kind: 'services'; services: readonly ServiceView[] }
  | { kind: 'commit'; service: ServiceKey; epoch: number;
      messages: readonly A2uiMessage[]; leases: readonly SurfaceLease[] }
  | { kind: 'update'; service: ServiceKey; epoch: number; message: A2uiMessage;
      lease?: SurfaceLease }
  | { kind: 'remove'; service: ServiceKey; epoch: number; reason: string };
interface LiveSession {
  readonly imagePolicy: ImagePolicy;
  subscribe(listener: (event: LiveEvent) => void): Unsubscribe;
  start(): Promise<void>;
  setServices(ordered: readonly ServiceKey[]): void;
  action(lease: LeaseId, message: ClientMessage): Promise<void>;
  image(lease: LeaseId, uri: string, signal: AbortSignal): Promise<Response>;
  stop(): Promise<void>;
}
```

`SessionError` is `{code, message}`. Code is one of `UnsupportedBrowser`,
`StorageUnavailable`, `StorageCorrupt`, `StorageVersion`, `OwnerBusy`,
`InvalidInvitation`, `EnrollmentRejected`, `EnrollmentUnknown`,
`EnrollmentUnsaved`, `NotAuthorized`, `NetworkUnavailable`, `ProtocolError`,
`CapacityExceeded`, `SourceUnavailable`, `ScopeExpired`, `UnsupportedMedia`,
`InvalidImage`, `Timeout`, `Cancelled`. Messages are fixed UI text; never include
keys, invitation text, raw source frames or resource URIs.

`subscribe` emits cached status/roster/image capability synchronously but no cached render events;
the renderer subscribes before `start`. Start is idempotent and resolves when
manager tasks have started, not when every source is online. Network errors are
status events. Missing facilities/storage/ownership reject start. Stop is
idempotent, cancels leases before notifications and awaits I/O disposal.
Calling start again after stop is an error; create another session object.
Listeners must be synchronous, are isolated from one another's exceptions, and
cannot perform async network work inside the render event callback.

`ClientMessage` is A2uiClientMessage with optional raw metadata
`{a2uiClientDataModel?: unknown}` preserved; action accepts that intersection,
not a schema-parsed object that has discarded metadata.

`createBrowserSession(profile, transport)` is exported by browser-hub. Its source
set is the profile's selected accepted gateways. ProfileSnapshot is exactly
`{identity:{secretHex,endpointId}, gateways: readonly AcceptedGateway[]}`;
AcceptedGateway has the gateways-store fields with enrollment fixed to accepted.
The array has at most two entries in explicit selection order. The transport
argument is the loaded facade with generateIdentity/openEndpoint methods above.
No storage or ownership acquisition happens inside this low-level constructor.
Its caller must own the profile lock; test harnesses use isolated in-memory records.
`createLocalHubSession(url)` is
exported by renderer and wraps existing WS/HTTP/SSE; it keeps native media policy.
Its setServices is a no-op because legacy WS subscribes to all sources. Do not
impose new direct-mode capacity limits on that existing local route.

Renderer exports `createA2uiSession(source): RendererSession`,
`useA2uiSource(source): A2uiSession`, and existing
`useA2uiSession(url, resourceBaseUrl?): A2uiSession`. The old hook wraps the local
source and retains its current return fields. The renderer-owned types are:

```ts
interface A2uiSession {
  processor: Processor | null;
  surfaceIds: readonly string[];
  services: readonly ServiceView[];
  status: string;
  error: string | null;
  images: BrowserImages | null;
  endpoint: string;
  resourceBaseUrl?: string;
}
interface RendererSession {
  getSnapshot(): Readonly<A2uiSession>;
  subscribe(changed: () => void): Unsubscribe;
  close(): Promise<void>;
}
```

RendererSession owns one MessageProcessor, service/lease maps and subscriptions.
It subscribes to source events before calling start. getSnapshot returns a cached
object, stable until a notification, suitable for useSyncExternalStore. The hook
owns close on cleanup; close unsubscribes then awaits source.stop. SurfaceView
accepts A2uiSession, not the controller or ReturnType of the WS hook. Direct mode
uses endpoint `browser:gateway` and no resourceBaseUrl; local mode retains its URL
and optional resourceBaseUrl. Aggregate direct status is `Connected` if any source
is online, `Connecting…` if none is online but one is connecting/backing off, and
`Disconnected` otherwise. Per-source errors remain visible in services/status
events; aggregate error is the first active error in profile selection order,
or null. Preserve local status text compatibility.
Catalog construction is renderer-owned `createResourceCatalog(imagePolicy)`;
resourceCatalog remains a native-policy compatibility export for existing code.
Refactor the old client into local-source.ts plus a compatibility
A2uiWebSocketClient wrapper around LocalHubSession/RendererSession. Put shared
message helpers in messages.ts to avoid a client/session import cycle. The native
source emits incremental updates as received and ACKs images/1 only after their
synchronous processor application, preserving its current applied-ACK contract.
It does not wait for a global snapshotEnd before ACKing. Legacy standalone/stdio
Surfaces retain bare IDs under opaque service key `__standalone__`; local actions
forward their original IDs without direct-mode route stripping.

Keep BrowserImages' update/scope/response/close public compatibility methods. Add
a constructor overload taking `(lease, uri, signal) => Promise<Response>` for
RendererSession. Move native image-reference protocol I/O into native-images.ts;
LocalHubSession owns it and maps local lease IDs to server handle tokens. Its
image method delegates there. BrowserImages owns scoped bitmap decoding/budgets,
not gateway routing. The existing URL/send constructor remains a compatibility
wrapper for existing tests/consumers. SurfaceView uses session.images as before.
The local source emits images:false initially and images:true only after the
existing images/1 hello negotiation; disconnect emits false. This preserves the
legacy HTTP fallback before/without negotiation. Direct mode emits true from
subscription onward and always uses the scoped loader, including before a source
is online. A capability false event closes and clears BrowserImages before the
renderer snapshot notification; true creates it if absent.

On commit, invalidate previous leases and remove previous Surfaces of that
service, then synchronously process the complete validated snapshot with one
processMessages call. No action callback may run until that call returns and
the manager marks the epoch ready. Notify React once afterwards. A processor
exception fails/retracts only that service. On remove, issue deleteSurface for
all its Surfaces and release their image scopes before notifying React.

Action callbacks construct createActionMessage(action, processor.getClientDataModel()),
then call source.action with the current lease. Validate the client message but
retain raw metadata: the upstream schema strips metadata. routeClientMessage
narrows it to the captured service and strips the gateway/service prefix once.
Never copy the spike's null-model shortcut or replay an action on error.
An action promise resolves after the frame is written, not after app execution.

## 3. Identifier and wire bridge rules

Native v4 node IDs are exactly 64 lowercase hex characters; service IDs match
`[A-Za-z0-9._-]{1,64}` except `.` and `..`. Direct channel IDs are a valid node,
one `~`, and a valid service, at most 129 characters. Bare service IDs remain
valid for SDK/dev transports. Surface IDs are nonblank, at most 4096 UTF-16 code
units and may contain `/`; split only the first `/` after a channel.

Modify existing core service-id helpers for v4; do not retain legacy 16-character
nodes in new validation. prefix helpers keep target-service metadata narrowing.
Update their existing fixtures rather than adding a second conflicting parser.
Do not use readdressServerMessage's old gateway namespace assumption for native
v4: the chosen service stream's payload has bare SDK Surface IDs. Prefix it with
the captured authenticated gateway/service route directly.

The private WASM facade exposes these operations (all handles owned by one endpoint):

```ts
generateIdentity(): Promise<{ secretHex: string; endpointId: string }>;
openEndpoint(secretHex: string, relays: readonly string[]): Promise<Endpoint>;
interface Endpoint {
  connect(addressJson: string, alpn: 'omochi/pair/1' | 'omochi/gateway/4',
          signal: AbortSignal): Promise<Connection>;
  close(): Promise<void>;
}
interface Connection {
  readonly remoteId: string;
  open(header: { kind: 'control'; protocol: 4 } |
               { kind: 'service' | 'resource'; service: string; generation: string } |
               { kind: 'pair' }, signal: AbortSignal): Promise<Stream>;
  closed(): Promise<{ code: string; reason: string }>;
  close(): Promise<void>;
}
interface Stream {
  send(json: string, signal: AbortSignal): Promise<void>;
  receive(signal: AbortSignal): Promise<{ json: string; wireBytes: number } | null>;
  finish(): Promise<void>;
  cancel(): Promise<void>;
}
```

Pair is a bridge discriminator only: it writes no OpenStream header on the pair
ALPN. Other opens generate the exact native header before resolving. Generation
is a canonical nonzero decimal u64 string in the JS bridge. Rust parses it and
writes a JSON integer upstream. Control receive normalizes generation and pong
ID integers to decimal strings before returning JSON to JS. The native wire
never gains string generations. This avoids JSON.parse precision loss for u64
identity fields. Browser ping IDs cycle 1 through 2^32-1 with one outstanding.
All other byte/offset/count fields must be safe JS integers within their bounds.

No two receive or send operations may overlap on one direction; a second call
rejects ProtocolError. A send/receive registers an AbortHandle before awaiting.
Cancellation marks the entire stream dead, aborts in-flight futures, waits for
their guards to drop, then stops/resets both QUIC directions. A partial read or
write is never resumed. Cancel is idempotent and all future calls reject
Cancelled. Closing a connection cancels/joins its streams before resolving;
closing the endpoint cancels all connections. Do not use an uncancelled
Promise.race as an operation deadline.

The frame reader reserves its declared byte length before allocation; zero,
over-4-MiB, malformed UTF-8 or truncated bodies are errors. A receive returns
actual native payload bytes, excluding the four-byte header, for accounting.
Rust's typed outer envelopes reject unknown fields. A2UI bodies remain data and
cannot impersonate controls. JS validates the one-message v0.9 schema and
exactly one of the four server operations before applying state. Validate model
paths before sparse allocation: at most 128 slash-separated segments, no decoded
prototype/constructor/__proto__ segment, combined numeric indices <=100000.
Reject those prototype keys in JSON objects before merging and reject nesting
over 128. CatalogId must be the built-in basic v0.9 catalog in direct mode.

## 4. Session state, admission, and budgets

Profile selects at most two accepted gateways. Extra connect requests return
CapacityExceeded without disconnecting an admitted source. Source state is
`idle -> connecting -> control -> online`, or `backoff`, `terminal`, `stopping`,
`stopped`. Authenticate remoteId and hello.nodeId against the profile before
accepting a roster. A control/version/roster protocol error or application close
code 1 is terminal until explicit Retry/re-enrollment. Transport loss/code 0
enters backoff; a later authenticated code-1 rejection becomes NotAuthorized.
Do not label every network close as revocation.

Complete rosters are validated atomically: <=256 unique valid services, positive
u64 generations, nonblank names, optional string icons, no unknown fields. Keep
at most 1 MiB encoded roster metadata per gateway, including icons; larger valid
metadata is CapacityExceeded, not an authentication failure. Never fetch service
icons in this release. Roster loss immediately retracts that gateway's services.

setServices takes desired service keys in first-panel order, duplicates removed,
followed by a picker service if one is open. Reserve the first eight desired
slots, including temporarily unavailable sources; later keys show Service limit
without displacing earlier entries. Invalid routes are rejected synchronously.
The picker first lists services; selecting one subscribes to discover its views.
It does not subscribe to every source just to populate the picker.

Service state is `absent -> awaitingSnapshot -> ready`, or `backoff`, `terminal`.
Capture `{gatewayId, gatewayEpoch, serviceId, generation, serviceInstance}` on
each task. A new connection or service generation creates a fresh instance.
Stage from snapshotStart to exactly one snapshotEnd in a private A2uiSnapshot;
do not render or permit actions while staging. At snapshotEnd emit commit and
mark ready only after its synchronous application succeeds. Incremental updates
are valid only in ready. EOF/lag closes that service and resubscribes with a fresh
snapshot. Malformed service data/catalog errors are terminal for that service;
another service or gateway continues. A roster removal cancels it immediately.

Every createSurface allocates a new local Surface generation, including reuse of
the same ID after deletion. A lease captures the immutable task/Surface generation
and its AbortController. Lease lookup validates all generations before enqueue,
before upstream writes, and after resource responses. Delete, replacement,
unsubscribe, gateway loss, or shutdown aborts it before publishing removal.
Resource loops never read mutable manager.currentConnection/currentService.

| Budget (direct profile only) | Value and accounting |
| --- | --- |
| Active gateways / desired services | 2 / 8; explicit admission as above |
| Wire frame | 4 MiB including native envelope, excluding length header |
| Simultaneous frame-body allocations | 8 MiB across all Rust streams, reserved before Vec allocation; no hidden waiter queue |
| Staging snapshot per service / all retained staging+committed snapshots | 16 MiB / 32 MiB, encoded native message charges; frozen views keep their charge |
| Outstanding render application | One per profile; a commit is one admitted snapshot (<=16 MiB), individual messages <=4 MiB; commit retains its snapshot charge until applied |
| Pending incremental render queue | 32 messages and 16 MiB, whichever first; keep charge until synchronous processor application finishes |
| Action queue | 8 messages and 8 MiB, including in-flight writes until completion |
| Live Surface leases / retained Surface ID text | 4096 / 8 MiB per profile |
| Resource reads | 2 active across the profile, one stat/chunk exchange at a time per read; no implicit request queue |
| Encoded PNG | 8 MiB each, 16 MiB profile reservation before allocating bytes |
| Image dimensions / pixels / retained decoded accounting | 8192 per side / 4 Mi pixels / 32 MiB, eight bytes reserved per pixel |

Snapshot charges use merged replay messages, not just delta sizes. Compute size
incrementally during apply and reject before copying or sparse expansion; new
staging and previous committed state are both charged until commit/release.
On snapshot/frame/lease budget failure, retract that service and mark capacity
terminal until Retry or a desired-source change frees capacity. Render queue
overflow retracts its affected service and permits one fresh snapshot attempt;
a repeat in the same gateway epoch is terminal. Action/resource admission returns
busy CapacityExceeded and never adds an unbounded waiter. These are wire/storage
budgets, not a total JS/Rust/decoder RSS claim.

| Deadline | Policy |
| --- | --- |
| Connect / hello+first roster | 10s each; cancel connection on expiry |
| Pair exchange | 10s after connect; existing invite expiry still applies |
| Write | 5s, then discard its stream |
| Initial service snapshot | 30s from snapshotStart; close only that service |
| Heartbeat | Ping 10s after previous matching exchange; matching pong within 10s; failure closes gateway |
| Ready service read | No idle deadline; gateway heartbeat detects loss |
| Resource exchange / whole PNG | 15s / 20s; cancel captured streams on expiry |
| Reconnect | 500ms, 1s, 2s, 4s, 8s, 16s, 30s cap, multiplied by random [0.8,1.2]; reset after 30s continuously online |

visibility hidden does not revoke or replay work. On visibility/online return,
send an immediate ping if none is outstanding; an elapsed pong deadline closes
the connection and resnapshots. pagehide starts stop. pageshow starts a new
manager after ownership is acquired, never revives an old scope. Shutdown aborts
new work and leases first, disposes streams, closes endpoint, then releases the
owner lock. After 2s show Closing pending, but do not release the lock while an
endpoint is live; retain it until close completes or the document is destroyed.

## 5. Identity, bootstrap, and IndexedDB transactions

Use DB `omochi.browser.v1`, version 1. Stores have no indexes:

| Store/key | Exact record fields |
| --- | --- |
| `identity`, keyPath `id` | `{id:"default", secretHex:<64 hex>, endpointId:<64 hex>, createdAt:<safe ms>}` |
| `gateways`, keyPath `nodeId` | `{nodeId:<64 hex>, name:<trimmed 1..256 UTF-16>, relayUrls:<1..4 HTTPS URLs>, enrollment:"pending"|"accepted", createdAt:<safe ms>}` |
| `boards`, keyPath `id` | `{id:<UUID>, revision:<safe positive integer>, name, panels, createdAt:<safe ms>, updatedAt:<safe ms>}` |
| `preferences`, keyPath `id` | `{id:"ui", selectedGateways:<0..2 unique node IDs>, selectedBoardId:<UUID|null>}` |

Validate all reads against these shapes. Unknown DB versions return StorageVersion;
corrupt records return StorageCorrupt without replacing identity or deleting DB.
Native files and the experiment's localStorage keys are never migrated. Fresh
initialization creates all stores in the versionchange transaction. Browser keys
are exportable strings handled by trusted JS, not hardware/non-exportable keys.

Acquire `navigator.locks.request("omochi.browser.v1/default/endpoint", {
mode:"exclusive", ifAvailable:true}, callback)` before identity mutation or
endpoint binding. A null lock yields OwnerBusy; do not wait invisibly or steal.
Keep callback pending on a private release promise until endpoint.close resolves.
Board-only operations may run in other tabs. Identity reset also needs this lock.
If Web Locks/IndexedDB is missing or denied, direct connections fail closed;
do not silently substitute localStorage or an ephemeral new identity.
See [lock semantics](https://developer.mozilla.org/en-US/docs/Web/API/LockManager/request).

The hosted fragment is exactly `#connect:` plus unpadded base64url UTF-8 JSON
`{"version":1,"invitation":"omochi://invite/<hex-json>"}`; reject unknown fields.
The complete fragment is <=32768 ASCII bytes before base64 decoding. The decoded
JSON is <=24576 UTF-8 bytes and the native invitation is <=16384 ASCII bytes.
Apply existing native version/secret/expiry validation. Ignore direct-IP address
hints in browser mode; require 1..4 relay URLs in the invitation, each <=2048
characters, HTTPS, without credentials/query/fragment. HTTP relays are unsupported
in release mode. Remove the fragment immediately after memory capture, including
on parse failure, and never log or persist the wrapper/secret.

No public lookup is added in this release. Store gateway identity and relay hints;
normal connection tries saved relays in order. A failed/stale hint offers a new
invitation from the same gateway to refresh addresses, not another identity or
automatic trust from board data. Endpoint relay configuration is the union of
selected accepted gateways and the pending invitation, up to eight unique URLs.
A user adding/removing that set restarts the endpoint and resnapshots selected
sources; an ordinary failure of one gateway does not restart the other.

ProfileOwner in profile.ts is the orchestration boundary. Export
`openBrowserProfile(transport): Promise<ProfileOwner>` with this interface:

```ts
interface ProfileOwner {
  snapshot(): ProfileSnapshot;
  session(): LiveSession | null;
  pair(invitation: string): Promise<void>;
  selectGateway(nodeId: string, selected: boolean): Promise<void>;
  forgetLocalGateway(nodeId: string): Promise<void>;
  subscribe(listener: () => void): Unsubscribe;
  close(): Promise<void>;
}
```

Open acquires ownership and DB, validates/creates identity (no sockets), and commits
it before returning. A new gateway's stored name defaults to its first twelve
hex ID characters; LiveSession's node name uses the authenticated hello name.
Selection is
persisted to preferences, capped at two, then old session.stop is awaited before
creating a session for the new immutable snapshot. Only explicit user selection
or restoration of previously selected profiles dials sources; board imports do not.

Pairing always stops the current live session, uses a temporary same-key endpoint,
commits enrollment, closes that endpoint, then reconstructs the live session for
the selected accepted records. A newly paired gateway is not selected until the
user presses Connect. This avoids overlapping endpoints or dynamic relay-map
mutation. On pairing failure, close that endpoint and restore the previous
selected session. Keep the owner lock through all these transitions. ProfileOwner
serializes these commands on one promise chain, rejects new commands after close,
and never performs database transactions across awaited network work.

Enrollment transitions:

1. Under ownership, load and verify the existing secret derives the stored ID.
   If absent, generate identity without binding network sockets, write identity
   plus a pending gateway record in one RW transaction, and wait for completion.
2. Bind the endpoint only after that commit. Dial the invitation's exact ID on
   pair ALPN and send `{version:4,secret,name}`. The name is `Omochi browser`.
3. On accepted reply, update the gateway to accepted in another completed RW
   transaction. Only then may ordinary streams open. Invite expiry does not
   expire an already accepted peer record.
4. On connect failure/rejection leave pending and present retry/new invitation.
   On reply loss/timeout show EnrollmentUnknown. Retry in the same page with the
   same identity/secret only while it remains valid.
5. On remote success but local commit failure show EnrollmentUnsaved, retaining
   the accepted reply/address in memory. Retry just the local transaction. Do
   not open ordinary streams until it succeeds.
6. On reload, a pending record has no secret. Show Finish pairing with a new
   invitation; do not probe normal traffic to infer enrollment. A fresh invite
   can re-enroll the same durable identity. After expiry, require that new invite.

Opening a link for an accepted ID is an explicit reconnect/refresh workflow.
Commit new relay hints only after ordinary connection authenticates that ID and
validates hello. If normal access is rejected, offer pair redemption using the
in-memory invite. Never replace an accepted record with a pending one on parse
or network failure. Clearing identity is an explicit user action that lists the
old public ID and tells the user gateway enrollment is not remotely revoked.
Under the owner lock, stop sockets, clear identity/gateways/selectedGateways in
one transaction, retain boards, then require new enrollment.

Prepare data before opening an IDB transaction. Issue request chains inside IDB
request callbacks; do not await network, WASM, or unrelated promises while the
transaction is open. Resolve writes on transaction complete, reject on abort/
error, normalize QuotaExceededError as StorageUnavailable and preserve unsaved
UI input. Use BroadcastChannel `omochi.browser.v1/boards` to notify other tabs
after commits; messages carry only `{kind:"changed"}`, never keys/board data.
See [commit completion](https://developer.mozilla.org/en-US/docs/Web/API/IDBTransaction/complete_event).

## 6. PNG adapter and direct catalog policy

Direct images accept only complete `omochi://resources/<32 lowercase hex>` URIs
through a current lease. Return a Response with image/png and validated width/
height headers for the existing renderer decode path. Metadata must have status
200, image/png, safe size 33..8 MiB and a quoted resource-ID ETag. Each read uses
one independent resource stream with the captured service/generation, at most
24576 bytes, sequential offsets. Require canonical padded base64 and the exact
requested decoded length. A response ID need not equal the browser request ID:
the existing gateway replaces it; validate its ASCII shape but correlate using
the captured one-request stream. Cancel and dispose each stream after response.

Initial PNG acceptance is deliberately restricted: valid signature, first IHDR
length 13, 8-bit RGB or RGBA, compression/filter method 0, no interlace, legal
dimensions/pixel budget. Check chunk lengths/CRC32, exactly one IHDR, >=1 IDAT,
final zero-length IEND, no trailing data, no unknown critical chunks, and reject
APNG acTL/fcTL/fdAT. At most 4096 chunks. Inflate concatenated IDAT with
DecompressionStream('deflate') without retaining inflated bytes, enforcing exactly
`height * (1 + width * channels)` bytes and a filter byte 0..4 per scanline.
Cancel on excess/deficit/checksum errors before createImageBitmap. This prevents
an encoded-size-only check from accepting an excessive decompressed payload.
Unsupported PNG variants return UnsupportedMedia, malformed ones InvalidImage.

Reserve encoded bytes and validate dimensions before decoding. The renderer
retains its bitmap/canvas accounting and cancels late decoding results on lease
loss. Abort cancels the currently active stream and checks the lease before/after
every await. An upstream chunk already forwarded may finish; it cannot populate
an expired Surface or cause further reads.

`createResourceCatalog('sdk-png')` overrides Image, Markdown image loading,
Video, and AudioPlayer. Unsupported media produces accessible unavailable text
without assigning any network src. Do not call useResource/resolveResourceUrl
for direct-mode Image/Video/Audio. Text/Markdown still renders ordinary safe text
and user-activated links; image tokens use the scoped loader. Native catalog
behavior and native fetch policy stay unchanged. Never add an HTTP fallback.

## 7. Board codec and repositories

The normative exchange shape is [board-export.schema.json](board-export.schema.json).
Top level is exactly `{format:"omochi.board",version:1,board:{name,panels},sources}`.
sources is an array of `{nodeId,name?}` with one entry per referenced gateway,
sorted by nodeId; name is optional advisory text, never addressing/trust. Empty
boards have sources `[]`. Import requires strictly increasing node IDs; reject
duplicate/missing/extra or out-of-order source IDs. Export
preserves panel IDs; import creates a new board UUID and revision 1, so IDs in
different boards cannot collide. Source labels are only import-preview metadata;
native/current board storage need not persist them. Later labels come from the
authenticated source profile, not from an import.

All object levels reject unknown properties. Additionally enforce UTF-16 text
limits: name/title 200, panel ID 100, Surface ID 4096, source label 256; trim only
board name on save, reject blank text. Keep Rust's exact geometry rules: x 0..11,
y 0..100000, w 1..12, h 1..1000, x+w<=12; duplicate panel IDs/references and
rectangle overlap are errors. Do not add a y+h constraint absent from native
validation. Validate before DB writes or network work. Limit raw UTF-8 input to
1 MiB, 200 panels and 200 source entries. JSON nesting over 128 is rejected.

Create `packages/hub-ui/src/board-codec.ts` as the one strict browser exchange
validator/serializer, using manual field allowlists and current layout algorithms,
without a new runtime validation dependency. JSON Schema and fixtures are the
contract oracle; semantic checks above supplement JSON Schema. Native HTTP input
is still its existing Input shape; shared fixtures also verify that those valid
geometries agree with native validation. Do not globally change native serde's
unknown-field behavior just to implement file import.

```ts
interface BoardRepository {
  list(): Promise<readonly Dashboard[]>;
  get(id: string): Promise<Dashboard>;
  presets(): Promise<readonly DashboardPreset[]>;
  create(input: DashboardInput): Promise<Dashboard>;
  update(id: string, revision: number, input: DashboardInput): Promise<Dashboard>;
  delete(id: string, revision: number): Promise<void>;
  subscribe(changed: () => void): Unsubscribe;
}
parseBoard(bytes: Uint8Array): { input: DashboardInput; sources: SourceLabel[] };
exportBoard(board: Dashboard, labels: readonly SourceLabel[]): Uint8Array;
```

Repository errors are `{code,message}` with code `NotFound`, `Conflict`,
`InvalidBoard`, `StorageUnavailable`, or `StorageCorrupt`. Browser create allocates
crypto.randomUUID, revision 1 and safe millisecond timestamps. Update/delete uses
one RW boards transaction: get, check existence and exact revision, validate,
increment/put or delete, wait complete. Revision above Number.MAX_SAFE_INTEGER
is Conflict. get/list omit timestamp fields; list sorts by createdAt then ID.
presets returns [] in browser mode. Browser subscribe uses the post-commit channel.

HTTP repository uses existing `/api/dashboards`, `/{id}`, `/api/presets`, JSON
Content-Type and DELETE body `{revision}`. Keep server-generated IDs/revisions
and response order. Map 404/409/400/413/503 to the corresponding errors; other
failures are StorageUnavailable. Its subscribe fires after its own successful
writes; do not invent a dashboard SSE protocol. Existing local tabs still rely
on revision conflicts/reload for concurrent external changes.

Both UI modes use the same import/export functions. Import previews source labels,
then repository.create(parsed.input), never update an existing ID. Export excludes
IDs/revisions/timestamps, live data, bytes, keys, addresses and invitations. Display
missing profiles using gateway IDs; importing or selecting a board cannot pair,
dial or send actions. A user explicitly selects Connect for accepted profiles;
only already connected profiles are considered by setServices. Board deletion,
profile forgetting and remote trust revocation are independent UI operations.

## 8. Gateway queue provenance and revocation barrier

This is a source change preserving gateway v4 envelopes. Add `dispatch.rs` to
the gateway. An authorization epoch contains a CancellationToken and an async
RwLock dispatch barrier. Remote command origin contains that epoch plus its
connection's cancellation token; Local origin bypasses remote checks. Every
Command::Action and Command::Resource retains origin and its existing byte charge.
Remote serve_stream receives its origin; existing Unix serve_stream is a Local
wrapper. Resource reply receivers already closed are discarded before dispatch.

Immediately before SDK send, remote commands check both tokens, acquire a read
guard racing cancellation, then check again. Hold the guard through the bounded
five-second SDK write. Do not interrupt a partially written shared SDK frame on
session cancellation; that write is already dispatched and is irreversible.
A write timeout discards the SDK connection through existing error handling,
which also changes its service generation. Pending cancelled commands are skipped.

On revocation-generation change, cancel the old epoch and all remote sessions,
join their tasks, acquire its exclusive dispatch guard to wait for already-started
writes, then acknowledge store generation. No old-epoch SDK write may start after
that barrier. Create a new epoch for remaining authorized peers afterwards.
This retains existing broad session teardown on authorization changes rather
than introducing a new peer-filtered revocation protocol. Ordinary connection
loss cancels its session token even when a task is aborted (drop guard).

Trust forget can time out while a bounded write/barrier is still completing;
that remains an error and never rolls durable removal back. Queued commands keep
their cancelled epoch after the queue outlives the remote task. Never clear a
service-wide queue to revoke one remote caller: that would drop unrelated Local
commands. Tests must cover skipped remote work, preserved Local work, and an
in-progress write completing before acknowledgement.

## 9. Build, entry points, and delivery

Add `--web-url` to the gateway command, requiring its remote mode. It defaults
to unset. Accept an absolute HTTPS UI base of at most 2048 characters, without
credentials, query or fragment. Append the section 5 fragment to that exact base
when explicitly printing a newly created invitation. Preserve the existing native
invitation output as well. Do not emit either bearer form through service discovery,
normal connection logs or board export; no default public UI address is inferred.

Browser-transport's default `build` is TypeScript-only; it does not run Rust.
`build:wasm` runs a new `scripts/build-browser-transport.ts`: locked release cargo
build for wasm32-unknown-unknown, exact-version bindgen --target web, output named
`iroh` under package generated/. The JS facade dynamically imports that generated
module by an explicit assetBaseUrl; its public TS FFI types are handwritten in
src/ffi.ts, so ordinary type builds do not require generated files. It initializes
WASM with an explicit module_or_path URL. Generated files are ignored, never edited.

The two private packages join Bun workspaces and ordinary type builds. Add root
`build:hosted`: existing build:web, browser-transport build:wasm, then the hosted
hub-ui build. Native build:native/build:web never invoke build:wasm. Keep separate
hosted/local HTML entries and a shared App injected with repository/session.
Existing main.tsx constructs local adapters. New main.hosted.tsx constructs direct
adapters; no static hosted import appears in the local runtime dependency graph.

`packages/hub-ui/hosted/index.html` is the hosted Vite root and imports
`../src/main.hosted.tsx`. `vite.hosted.config.ts` uses base `./` and output
`packages/hub-ui/dist-hosted`. After Vite, copy browser-transport/generated intact
to dist-hosted/iroh. The facade gets `new URL('./iroh/', document.baseURI)`.
Do not bundle Rust generated code through Node import paths. Files have stable
iroh names; use Cache-Control no-cache for iroh JS/WASM and HTML, and immutable
one-year cache only for Vite-hashed assets. The native crate continues embedding
dist, which has no direct-mode WASM requirement; no native WASM MIME addition is
needed for this initial local-only entry.

Static host serves JS as text/javascript, WASM as application/wasm, no redirects
for asset requests, and index.html only for `/` (the app uses fragments, not SPA
path routing). Emit `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`
and CSP exactly:

```text
default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self' https: wss:; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; worker-src 'none'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'
```

The connect list permits operator-supplied HTTPS relay origins; renderer code
still enforces the PNG-only policy. There are no third-party scripts/telemetry.
See [WASM CSP](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/script-src).
Put this exact asset/header contract in deploy/hosted/headers.json and README;
the HTTPS integration host reads that file. The release artifact is the static
directory, not a requirement to provision a particular cloud vendor.

Initial distribution is a public-relay preview with no uptime guarantee. Gateway
invitations select their configured TLS relay, including existing n0 defaults;
the browser does not provision or operate another relay. Dedicated production
relay/SLO and public publication are operator rollout decisions, not unresolved
implementation choices. No deployment happens as part of implementing this plan.
Browser diagnostics are in-memory fixed error codes, source IDs, epochs and
budget/retry counters; no raw frames, URI secrets, keys or persisted telemetry.

## 10. Deterministic checks and release acceptance

Use `spec/fixtures/browser-gateway-v4.json` for positive/max-u64/unknown-field/
malformed-frame/sparse-path/cancellation cases in Rust bridge and TS tests.
Create `spec/fixtures/board-export.json` with schema/semantic rejection cases.
The production direct integration harness is `tests/browser/browser-hub.mjs`;
its fixture is `crates/gateway/examples/browser_fixture.rs`, using real SDK apps
and two gateways with separate stores. Test flags/fault injection live only in
that fixture, never production CLI. Preserve experiment evidence unchanged.

Run modes `--mode session|profile|media|boards|hosted|all` with child fixture,
Chromium CDP, per-run private state/profile, sanitized result JSON and screenshots,
finally cleanup. The fixture has explicit gates for partial frames, roster/
generation changes, snapshots, and stalled resource/SDK writes. Session mode
accepts test invitations supplied to an ephemeral in-memory ProfileStore; it is
available before M2. Profile mode tests real IndexedDB and a second tab. Media
mode checks actual canvas pixels and forbidden HTTP requests. Boards mode tests
browser repository and the real native HTTP repository. No mocked rendering.

Hosted mode serves actual dist-hosted over loopback HTTPS with a one-run test
certificate and the prescribed CSP/MIME/cache headers. Trust only that leaf SPKI
in the isolated Chromium profile; never disable all certificate verification.
Use a locally controlled TLS relay for deterministic fault injection, trusting
its explicit CA in native iroh. Also retain a public-relay smoke mode for the
existing source path. Separate-network operator acceptance runs the same hosted
artifact on two hosts before claiming public deployment support.

Required session cases: 64-hex nodes, service IDs of length 1/16/17/64, maximum
u64 generations, same service/Surface IDs on two gateways, form drafts narrowed
to target service, interrupted snapshots, delete/recreate, generation replacement,
control loss/pong mismatch, late images, partial reads, malformed rosters, no action
replay and A failing while B remains usable. Use fake clocks/randomness in unit
tests to assert exact timers; real smoke reconnect deadline is 90s after a controlled
relay resumes, including bounded retry and snapshot time.

Required profile/board cases: denied storage, corrupt identity, DB upgrade errors,
owner contention/crash, lost acceptance, local-commit failure and reload, expired
invite and new-invite same-key recovery; unknown nested fields, UTF-16 edges,
duplicate sources/panels/references, geometry/overlap, 1 MiB input, import-as-copy,
CAS conflict/delete, quota/abort failure and no network/trust mutation on import.

Required media/revocation cases: bad PNG signatures/CRCs/IHDR/IDAT, decompression
excess/deficit, APNG, encoded/pixel/concurrency budgets, unsupported Image/Markdown/
Audio/Video with zero external/HTTP resource loads; abort during stat/read/decode;
blocked SDK commands, revoke, wait for ack, release gate, and verify only Local
and already-dispatched work can arrive.

Load acceptance is ten minutes of two gateways/eight services, with 10000 1-KiB
incremental updates distributed across them and repeated two-reader PNG transfers.
Assert encoded-budget counters never exceed the table and return to zero on stop;
no rejected promises, stale renders or monotonic retained-scope growth. Parsed
objects/decoder RSS are measured diagnostically, not asserted equal to wire budgets.
Direct build JS/WASM must import no Node builtin and fetch no static-host app-data
API. Keep native local browser integration and packaging checks passing at every
milestone; M5 only verifies these specified contracts, not chooses new ones.
