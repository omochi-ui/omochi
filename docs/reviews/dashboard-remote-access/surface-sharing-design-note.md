# Proposed implementation of Surface-scoped sharing

Historical note, superseded by the [gateway-sharing plan](../../dashboard-remote-access.md)
on 2026-11-03. The manager below is outside the current initial scope.

Status: design explanation in response to the author's question. This note is
not an approved change to the proposal, a resolution of ZO-001, or a completed
review stage. Stage 5 remains at its author checkpoint.

## Recommended boundary

Use the hub as the enforcement point. A client receives only the Surfaces allowed
by its grant and operates on them through opaque handles issued by that hub.
Extend the existing scoped subscription and Surface handle mechanisms rather than
implementing a second rendering or action-routing system.

The proposed resource rule is: a Surface publishes a resource for its viewers by
including its canonical URI in that Surface's authoritative state. Publication
grants read access to the entire immutable resource, subject to its media policy
and the viewer's current grant. This is a new authorization rule, not a property
already implemented by the SDK resource registry.

It is a publication model, not proof that one Surface owns a resource. One
resource can be deliberately published by multiple Surfaces. If strict ownership
independent of published state is required, use explicit SDK publication instead,
as described below.

## 1. Resolve a grant into an allowed Surface set

For an authenticated dashboard-client EndpointId, load its current grant and
the saved dashboard controlled by the owner. Resolve panel references into
`(nodeId, serviceId, surfaceId)` tuples. Permissions to view, interact, and edit
layout remain separate.

The client may request a subset of this set, but it cannot add a Surface to it.
Shared layout edits can only rearrange permitted references. Owner changes to
membership take effect at an explicit authorization generation boundary.

## 2. Filter delivery and issue opaque handles

Extend `Hub::scoped_subscription` to apply an authorization predicate to frozen
snapshot replay and incremental updates before serialization. Skip disallowed
messages before assigning the outbound sequence number. Snapshot completion and
acknowledgement must still progress when all messages are filtered out.

Filter service lists, dashboard metadata, and diagnostics separately; hiding a
panel in the UI cannot enforce those endpoints. Share only metadata needed for
the allowed Surfaces.

For an allowed Surface, issue an unpredictable token kept in a per-session table.
Its server-side entry binds the grant generation, session lifetime, service
instance, Surface generation, and allowed operations. Resolve the token only
inside that authenticated session. Reuse the internal `SurfaceHandle` behind
this table; do not trust a client to serialize its own internal scope.

## 3. Route actions through the authorized handle

A remote action supplies a handle and an A2UI action body. The hub checks the
interaction permission and current grant, resolves the captured internal handle,
and calls `SurfaceHandle::action`. The existing method replaces the supplied
destination and narrows action metadata to that Surface.

The remote adapter must not call the unrestricted `Hub::action` entry point used
by today's local browser adapter. A viewing grant cannot send actions. Pending
actions retain cancellation linked to the grant as well as the subscription and
Surface, and are checked before dispatch.

This authorizes an action's destination, not its business effect. An allowed
button might affect shared app state. Apps must implement any finer user or
object permissions; sharing a Surface does not sandbox its action handler.

## 4. Establish resource publication from trusted state

Maintain a per-Surface index of published resource URIs derived from the hub's
materialized upstream A2UI snapshot. Do not derive authorization from client
requests, client draft data, action metadata, renderer acknowledgements, or a
client-provided list of supposedly referenced resources.

The initial supported publication grammar should be deliberately narrow:

- A canonical complete `omochi://resources/<32-lowercase-hex>` string in the
  Surface's component properties or data model publishes that resource.
- Markdown resource destinations require a defined server-side parser and shared
  extraction fixtures. Unsupported encodings and constructed destinations are
  denied until that support exists. A broad substring search is insufficient.
- A URI assembled from separate pieces by a custom client component does not
  automatically publish anything. The app must also supply the complete URI in
  authoritative Surface state, or use explicit SDK publication in a later version.

All authoritative data-model values delivered for the Surface are within its
sharing boundary, including values not currently rendered on screen. Publishing
a URI in such a value authorizes bytes, not just disclosure of the URI itself.
Apps must not echo an arbitrary user-supplied resource URI into this state unless
they intend to publish it. This assumption must be accepted explicitly; an opaque
ID appearing in UI state is not, by itself, an existing permission declaration.

The index keys include the service instance and Surface generation. Resource IDs
are resolved only against that Surface's owning service. Recompute affected
membership from merged state after patches, including removals; do not retain
the union of all references ever seen. Bound the index's count and text bytes.
An initial implementation can reuse snapshot replay for inspection, but should
not clone every service snapshot on every resource request.

## 5. Issue read capabilities rather than accepting arbitrary IDs

An image request can continue to supply a Surface token and URI. Before creating
any transfer, the hub checks both the current viewing grant and current URI
publication. Only then does it invoke the existing image resolver. Apply the
same membership check to resource-backed images; their current handle validation
alone does not establish publication.

For generic resource reads, resolve the URI once into an opaque resource token
bound to this session, grant, Surface, publication lifetime, and resource ID.
Subsequent stat/HEAD/read/Range operations use that token, never a client-selected
service route. Resource tokens cannot be used as image tokens to bypass image
validation, or as handles to another Surface. Existing media policy and resource
GET/HEAD, Range, and ETag semantics remain separate from authorization.

In the remote adapter, bytes return over an authenticated iroh stream. Do not
fall back to the existing loopback `/resources/{route}/{resource}` endpoint.
The local browser contract can remain unchanged.

External HTTPS image reads must retain the existing fetch policy. An image
publication rule for those URLs needs the same defined extraction behavior;
an allowed Surface token must not become an arbitrary URL-fetch permission.

## 6. Make removal and revocation terminate capabilities

Update the resource index and its cancellation tokens under the same ordering
boundary as the authoritative Surface state update, before publishing that
update. When a URI is removed, cancel its active resource tokens and pending
reads. If it is later added again, issue new tokens so old ones remain invalid.
Unrelated data changes need not invalidate an unchanged resource publication.

Grant revocation, removal of a panel reference, Surface deletion/replacement,
service restart, and session closure invalidate their dependent tokens. Check
permission before opening an upstream read and before forwarding each response
chunk; race cancellation against stalled reads and writes. After the revocation
barrier, no newly authorized delivery is permitted. Bytes already delivered or
buffered downstream cannot be recalled.

## Example and acceptance tests

One service publishes two Surfaces:

```text
public:  image = omochi://resources/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
private: image = omochi://resources/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
```

A grant for `public` receives only its snapshot, updates, and handle. Reading
resource A succeeds. Supplying resource B with the public handle fails even when
the recipient knows B's ID. A forged private Surface ID fails. An opaque resource
token copied to another session fails. A viewer cannot dispatch an action.

After A is removed from public state, its existing tokens cannot start or continue
reads. Reintroducing A creates a new publication lifetime. If the app deliberately
publishes A in both Surfaces, both sets of viewers can read it. These tests must
also run through the resource-backed image path and during grant revocation.

## Alternative: explicit SDK publication

If publishing complete URIs in authoritative Surface state is an unacceptable
permission contract, add an explicit resource publication API to all three SDKs.
The app declares which resources a Surface publishes, and the SDK sends separate
publication/retraction control messages tied to the Surface and service lifetime.
The gateway and hub validate and retain those declarations. The client never
creates publication declarations.

This model supports constructed URIs and custom components without parsing their
rendering expressions, and separates disclosure of an ID from permission to read
its bytes. It requires protocol specification, cross-language SDK work, gateway
replay/lifetime support, and compatibility decisions. Existing SDKs cannot be
assumed to implement it. Until a verifiable publication rule is available, a
shared grant must fail closed for those resources.

## Existing implementation anchors

- [Scoped subscriptions, handles, actions, and resource routing](../../../crates/hub/src/scoped.rs).
- [Current local browser image adapter](../../../crates/hub/src/browser_images.rs).
- [Image resolver and SDK-resource image reads](../../../crates/hub/src/images.rs).
- [Materialized snapshot](../../../packages/sdk/rust/src/snapshot.rs).
- [Existing service-wide resource registry](../../../packages/sdk/rust/src/resources.rs).

These are implementation proposals verified against source structure, not
implemented behavior or passing test results. The main design must select and
specify its publication model before ZO-001 can be closed.
