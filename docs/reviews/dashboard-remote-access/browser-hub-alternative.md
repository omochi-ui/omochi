# Browser-resident hub and gateway sharing

Historical experiment proposal. The [experiment passed](../../../experiments/browser-hub/RESULTS.md)
and its result is reflected in the [current plan](../../dashboard-remote-access.md)
as of 2026-11-03. Statements below about untested feasibility describe the earlier
decision point, not the current experiment status.

Status: alternative under discussion after the author reconsidered the trust and
deployment boundary. This supersedes the assumption that a native hub server is
required for remote viewing; it is not an implemented or validated replacement.

## The question to answer first

Can the browser perform the gateway-consuming functions currently assigned to
the hub: enroll its identity, discover the connected gateway's services, receive
snapshots and updates, send actions, read resources, and combine multiple gateways
for rendering? If so, the hosted dashboard needs no separate native hub process
for that remote viewing path.

```text
Browser
  dashboard documents + renderer + gateway-session aggregation
          | iroh WASM via relay
          +----------------> gateway A -> local SDK apps
          +----------------> gateway B -> local SDK apps
```

The web host serves static assets. The relay forwards encrypted packets. Native
gateways still run near the SDK applications to discover them and reach their
Unix connections. Eliminating the native hub does not eliminate gateways or the
relay.

## Separate two things users share

1. Gateway access: the gateway owner enrolls a recipient's browser identity and
   can later forget that identity. The current grant is coarse: a registered hub
   can access that gateway's exposed services, including app actions. Do not
   present this as a read-only or selected-Surface grant.
2. A board document: titles, layout, gateway/source references, and selected
   Surfaces. Sharing the document does not grant access to any referenced source.
   A recipient enrolls separately with each required gateway.

A board can initially be a versioned JSON export/import. A static URL encoding is
possible for small documents, with explicit size limits. Persistent short links
or synchronized editable boards require a separately chosen storage service;
they are not required to prove browser gateway connectivity.

Board documents contain no browser private keys or reusable invitation secrets.
An enrollment link can direct a browser to the same static app, but it remains
short-lived and single-recipient under the existing invitation rules. A board
can continue to be useful after its creator's enrollment link expires.

The browser is already trusted with everything its gateway enrollment exposes.
Filtering which permitted Surface is currently displayed is then a layout or
subscription choice, rather than a security promise to a less-trusted recipient.
The cross-user Surface publication manager proposed in the earlier design is
not needed for that coarse gateway-sharing model. Resource lifetime handles,
generation checks, validation, bounded queues, and cancellation remain needed.

## Existing contracts are a starting point

[Gateway v4](../../../spec/gateway-v4.md) already defines the control stream,
service generations, service snapshots/updates, reverse actions, and independent
resource requests. [Gateway enrollment](../../../crates/transport/src/pairing.rs)
authenticates the remote EndpointId and registers its Hub role. A browser can
occupy that protocol role; it does not imply running a native daemon.

The official [iroh browser documentation](https://docs.iroh.computer/languages/wasm-browser)
supports compiling iroh to WASM, with an application-specific wasm-bindgen wrapper
and relay-only encrypted connectivity. These facts support a connectivity
hypothesis, not proof that this repository's gateway client works in a browser.
The existing relay-only transport test uses native endpoints; it is not a WASM
or browser end-to-end test.

## What must change or be proven

- Extract or implement the gateway client behind browser-compatible interfaces.
  The current hub unconditionally includes UnixStream, native spawning, native
  image fetching and desktop dependencies. Its `browser` Cargo feature enables
  the native HTTP/WS server; it does not mean a browser/WASM target.
- Avoid carrying the SDK's Unix resource registry and multithreaded native Tokio
  configuration into a browser transport wrapper. Reuse framing and message
  contracts where feasible, without claiming the full dependency graph compiles.
- Persist the browser identity and accepted gateway records with durable-write
  semantics around enrollment. A reload, denied storage, cleared storage, and
  overlapping tabs must have specified behavior. The current desktop Store is
  not browser storage.
- Restore correct snapshots after reconnect and gateway/service replacement.
  Background suspension can interrupt heartbeats; reconnect with fresh state and
  never replay app actions.
- Bound browser memory and transfer concurrency. A per-tab hub does not inherit
  the native hub's implemented capacity controls automatically.
- Preserve resource framing and range behavior when reading SDK-registered media
  over iroh; deliver image bytes directly to the renderer without native HTTP
  image-reference URLs.

## The image boundary prevents a blind port

SDK-registered resources already have a gateway protocol and can be part of the
first proof. External HTTPS images are different. The current hub checks public
destinations with native DNS resolution/pinning, redirects, encoded/decompressed
size limits, and image decoding policy. A browser fetch is subject to CORS and
does not expose equivalent socket/DNS controls.

Do not silently replace that resolver with unrestricted browser image tags or
assume feature parity. Decide whether apps register media as SDK resources,
whether a source-side constrained image resolver is added, or whether an optional
native adapter supplies that operation. A source-side resolver is new server
work and potentially a new protocol operation; it is not part of today's v4
contract. The browser-only proof may limit itself to SDK-backed images, but that
limit must be explicit rather than presented as full existing image support.

## When a native hub still helps

Keep the existing local hub/bridge path while evaluating the alternative. It
provides a localhost page and WS connection to local Unix sources, works without
public relays, and supports native image fetching. Native/headless consumers can
also use its gateway-aggregation logic. Browsers cannot directly perform Unix
socket discovery, and ordinary browser iroh connections currently need a relay.

A native hub can remain an optional deployment for those requirements. Making it
the mandatory intermediary for every remote browser introduces another listener,
enrollment boundary, and downstream protocol that direct gateway connectivity
could avoid. Supporting web-to-native-hub iroh as an optional adapter is possible,
but should be justified separately after the direct path is evaluated.

## Proposed next experiment

Use a minimal WASM gateway client against an unchanged native gateway. Prove:

1. Import a gateway invitation and durably register the browser identity.
2. Connect with `omochi/gateway/4` and consume the control service list.
3. Receive one real SDK Surface's snapshot and updates and render it.
4. Send a real button action and observe the SDK's resulting update.
5. Display an SDK-resource image by pulling resource bytes over iroh.
6. Recover from gateway/browser reconnect with fresh snapshots; revoking the
   browser identity at the gateway closes access.

Run through an actual browser and a TLS relay; record supported browser versions
and failures. If this succeeds, rewrite the main design around browser gateway
clients and board documents. Reassess the optional native hub and external-image
contract before completing architecture review. If it fails, record the blocker
and compare the original native-hub path using the actual evidence.

This note does not authorize deployment or claim any of these tests passed. The
existing Stage 5–7 findings concern the earlier native-hub-sharing design and
must not be treated as validated requirements for this alternative.
