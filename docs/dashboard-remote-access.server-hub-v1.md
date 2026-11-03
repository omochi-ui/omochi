# Dashboard remote access and sharing

Status: proposal for EGM review. No remote dashboard implementation is claimed.
Date: 2026-11-02.

## Problem

A dashboard currently connects to the hub that serves its web assets. Users need
to open the same interface on another computer or phone, from
`https://app.omochi.fyi/#connect:...` or a separately hosted copy, and connect to
their own hub. They also need to share one dashboard without giving the recipient
access to every app aggregated by that hub.

The hub remains the authority for live app state, saved dashboard configuration,
and access permissions. A hosted dashboard instance distributes browser assets;
it does not require a cloud hub or a copy of the user's application data.

## Goals and scope

- Keep local dashboard access on the existing loopback HTTP and WebSocket routes.
- Support remote dashboard access through iroh from an ordinary browser.
- Use the same dashboard UI and A2UI message processing for both connections.
- Support personal remote access and access limited to one shared dashboard.
- Separate permission to view, interact with an app, and edit dashboard layout.
- Allow enrollment, later reconnection, expiry, and explicit revocation.
- Preserve existing gateway v4, SDK, stdio preview, and local browser contracts.

Cloud data synchronization, anonymous public dashboards, reusable public access
links, account login, simultaneous collaborative layout editing, and browser UDP
hole punching are outside this proposal. Existing revision conflicts remain the
mechanism for concurrent dashboard edits. Audio and video transport are not added.

## Current system and reading list

Reviewers should read these files as the implementation baseline. No additional
conversation context is needed to understand this proposal.

- [Architecture](architecture.md): the Rust hub aggregates local and remote gateways.
- [Browser boundary](../crates/hub/src/http.rs): `/ws`, `/events`, `/services`,
  dashboard CRUD, presets, connection diagnostics, resources, and static assets.
- [Dashboard app](../packages/hub-ui/src/App.tsx) and
  [service subscription](../packages/hub-ui/src/services.ts): same-origin fetch,
  WebSocket, and EventSource are currently selected inside the UI.
- [Renderer client](../packages/renderer/src/client.ts) and
  [browser image client](../packages/renderer/src/images.ts): WebSocket session,
  image handles, acknowledgements, and subsequent HTTP image reads.
- [Browser image contract](../spec/hub-browser-images.md): Surface lifetimes,
  one-use image references, fetch policy, cancellation, and byte/pixel limits.
- [Gateway v4](../spec/gateway-v4.md), [transport setup](../crates/transport/src/lib.rs),
  [pairing](../crates/transport/src/pairing.rs), and
  [trust roles](../crates/transport/src/trust.rs): authenticated EndpointIds and
  registered Gateway/Hub roles; these do not authorize dashboard clients today.
- [Hub supervision](../crates/hub/src/remote.rs) and
  [CLI startup](../crates/cli/src/main.rs): the CLI waits for a registered gateway
  before starting its iroh endpoint; dashboard-only remote access needs a listener
  even when no remote gateway has been registered.
- [Dashboard store](../crates/hub/src/dashboards.rs): saved layouts and revisions.
- [Relay deployment](../deploy/relay/README.md): templates for pinned iroh-relay
  1.4.1 exist; those templates do not establish a deployed public relay.

The workspace pins iroh 1.4.1. The
[official browser documentation](https://docs.iroh.computer/languages/wasm-browser)
states that browser endpoints compile to WASM, require relays, and retain
end-to-end encryption. It recommends an application-specific Rust wrapper using
wasm-bindgen. Native JavaScript bindings do not provide a browser replacement.
Compatibility with the pinned version and target browsers still needs a spike.

## Architecture

```text
Local browser ---------- HTTP / WS / SSE -----------+
                                                   |
Hosted or self-hosted UI                            v
  browser -> iroh WASM -> public relay ----------> Rust hub
                                                   |
                                        Unix / existing iroh gateway v4
                                                   |
                                                gateway -> SDK apps
```

The public web host serves HTML, JavaScript, CSS, and WASM. The relay forwards
encrypted traffic. Neither service owns dashboard state or grants app access.
The hub must be running and reachable through a relay for remote browser access.
Local access continues to work when a relay or remote gateway is unavailable.

The browser uses one `HubClient` interface with two implementations. The local
implementation adapts existing HTTP, WS, and SSE. The remote implementation uses
typed requests and subscriptions over iroh. The interface covers dashboard CRUD,
presets, available services, connection state, Surface subscriptions, app actions,
and image/resource reads. Image bytes and resource response metadata travel over
the selected connection rather than being fetched from the UI host.

The renderer's A2UI processor stays independent of transport. A separate adapter
owns connection lifecycle, framing, acknowledgement, and binary transfers. The
remote protocol is versioned separately, provisionally `omochi/dashboard/1`.
Enrollment uses a separate protocol from normal dashboard traffic. Existing
`omochi/gateway/4` and gateway pairing messages are unchanged.

Remote requests name a fixed set of operations. They do not expose arbitrary
loopback URLs, files, HTTP routes, or a general TCP tunnel. Large binary reads use
separate streams so an image cannot occupy the dashboard control stream.

## Access model

The hub registers dashboard-client EndpointIds separately from gateway peers.
An authenticated connection establishes which client is present; a stored grant
establishes which operations and data it can access.

| Grant | Visible data | Allowed changes |
| --- | --- | --- |
| Personal remote access | Hub dashboards and app views | View, app interaction, and layout editing |
| Shared dashboard | One named saved dashboard and its permitted views | Independently granted app interaction and layout editing |

Viewing includes the images and resources required by permitted views. App
interaction allows actions to those views. Layout editing changes the shared
dashboard's name and panel geometry; it does not permit adding arbitrary app
views, browsing other dashboards, changing grants, or enrolling other clients.
Personal access in this proposal does not include remote trust administration.
The owner administers grants locally.

A shared grant follows the current saved revision of its dashboard. The owner
controls which references belong to that dashboard. A reference added by the
owner becomes visible to its recipients; removing one withdraws access to it.
Deleting the dashboard withdraws all its shared access. Shared preset access is
deferred because presets have a different owner and lifetime.

Filtering happens in the hub before sending snapshots, updates, service names,
dashboard lists, diagnostics, images, and resources. Actions and layout writes
also check the grant. Client-supplied surface IDs, resource IDs, or dashboard IDs
cannot widen it. A panel grants access to its entire Surface, including its data
model; it is not a way to hide selected fields inside a Surface.

Service generations, Surface replacement, subscription lifetimes, and grant
revocation constrain every operation. Existing image handles help enforce image
lifetimes; they do not already provide shared-dashboard action authorization.
Grant-aware action dispatch is new work.

## Enrollment and connection links

Remote dashboard listening is opt-in and independent of remote gateway setup.
Enabling it does not expose the loopback HTTP server to the internet or broaden
its Origin allowlist. The owner creates a personal or shared grant locally and
issues an enrollment link for it.

The proposed link is `https://app.omochi.fyi/#connect:<base64url-json>`. The same
fragment works on a configured self-hosted UI origin. The payload contains a
version, the hub EndpointId, a relay address, a short-lived invitation secret,
an expiry, and an optional dashboard selection. Selection is navigation data;
the hub's grant determines access. No hub private key is included.

The browser reads the fragment before loading third-party code, captures the
invitation in memory, and removes the secret from browser history immediately.
URL fragments are not sent in the page's HTTP request, but scripts executing on
that origin can read them. The web host and its deployed JavaScript are therefore
part of the trusted client software. End-to-end encryption does not remove this
trust dependency. The hosted client should avoid third-party scripts and avoid
logging invitation payloads.

The browser creates an iroh identity and connects to the exact hub EndpointId in
the link. The owner intends the link's bearer to redeem the invitation; users
must receive the link through a channel they trust. The hub atomically consumes
the invitation and binds its preselected grant to the authenticated client ID.
The proposal uses the existing 180-second invitation lifetime as a starting point.
One identity wins redemption; the same identity can retry a lost success reply
within the invitation lifetime. A second identity cannot reuse the secret.

Enrollment persists before reporting success. Reconnection uses the registered
identity without resending the secret. Clearing browser storage or changing the
UI origin requires enrollment of a new identity; identities are not transferred
between app.omochi.fyi and self-hosted origins.

Persistent grants expire only when the owner chooses an expiry or revokes them.
Invitation expiry does not expire a grant already issued. Each recipient gets a
separate invitation so the owner can revoke one recipient without removing others.
The UI reports invalid links, expiry, redemption by another identity, relay
failure, unsupported protocol, unavailable hub, and revoked access separately
where the authenticated protocol provides enough information.

## Lifecycle, failures, and limits

Each connection moves through loading, connecting, enrollment if needed,
authorized, reconnecting, and closed states. A failed connection must not fall
back to the web host's `/api` routes or silently select another hub.
Reconnect obtains fresh snapshots and current grants. It never replays app
actions or uncertain layout writes. A lost write reply triggers a revision read
before the user retries. An offline app appears unavailable within the dashboard.

Revocation removes durable permission and cancels all affected subscriptions,
queued actions, binary streams, and image references. Success requires all live
hub processes to acknowledge enforcement, following the existing store rules.
Failure to acknowledge remains an error; it does not restore the removed grant.
Already delivered bytes cannot be withdrawn, and an app action already executed
by its source cannot be undone by revocation.

The remote protocol must preserve bounded frames, snapshots, queues, stream
counts, transfer deadlines, and cancellation. Existing browser image byte/pixel
budgets and fetch restrictions apply to remote delivery too. Resource operations
preserve GET/HEAD, Range, and ETag semantics where they are already supported.
Unlike generic resources, image reads retain their existing no-range policy.

The relay carries all browser traffic, so relay capacity and egress cost grow
with recipients and transferred bytes. Each client has its own subscription;
clients that stop acknowledging updates are disconnected rather than allowed to
grow an unbounded queue. Production needs a working public relay and explicit
capacity settings. This document does not claim that one has been provisioned.

## Alternatives and trade-offs

| Alternative | Benefit | Reason for the proposed choice |
| --- | --- | --- |
| Publish hub HTTPS/WSS through a reverse tunnel | Reuses browser APIs and much of the current UI | Requires an internet-facing HTTP authentication boundary and tunnel infrastructure; iroh is the requested connection model |
| Cloud hub stores and serves dashboards | Access can continue while the user's hub is down | Adds cloud data ownership, synchronization, and backend operation outside the intended scope |
| Browser extension or local native proxy | Uses native iroh and may support direct paths | Requires installation on each viewing device |
| WebRTC data channels | Browser networking with possible direct paths | Introduces a different transport and signaling path alongside the existing native iroh system |
| Generic HTTP tunnel over iroh | Can reuse more existing HTTP handlers | Makes operation-level authorization and resource exposure harder to audit than typed dashboard operations |

The main counterargument is that a reverse tunnel avoids a browser WASM build,
new framing, and transport abstractions. If the browser spike fails or relay cost
is unacceptable, that option should be reconsidered before implementing grants
on an unproven browser transport.

## Open decisions and release gates

These are unresolved implementation inputs, not completed design decisions.

1. Browser identity storage: choose the persistence format, recovery behavior,
   and handling of storage denial. A WASM-exportable key should not be described
   as a non-exportable browser credential.
2. Specify typed operation schemas, binary framing, protocol negotiation, exact
   queue/connection/stream limits, and timeouts in a new wire contract.
3. Specify grant persistence and the store-version behavior for older native
   binaries sharing state with a binary that understands dashboard clients.
4. Specify how dashboard-revision changes atomically withdraw old Surface access
   while a subscription, image read, or layout write is in progress.
5. Select target browser versions and validate iroh 1.4.1 WASM builds, lifecycle
   suspension, CSP requirements, and relay compatibility on them.
6. Choose the first production relay, operating limits, and host deployment path.
7. Choose exact local enrollment and grant-management command/UI names.

## Detailed Implementation Plan

### 1. Prove browser connectivity

Create a small application-specific WASM wrapper pinned to iroh 1.4.1 with
default features disabled. Keep it separate from desktop mDNS and filesystem
trust code in `crates/transport`. Demonstrate a browser connecting to a Rust hub
test endpoint through a real TLS relay and authenticating its exact EndpointId.
Test both app.omochi.fyi-style hosting and a second UI origin. Record browser
versions, build flags, bundle cost, relay path, disconnect behavior, and results.
Resolve open decisions 1 and 5 before committing to the remaining transport work.

### 2. Extract the local client boundary

Introduce `HubClient` in the browser packages and adapt the current fetch,
EventSource, renderer WebSocket, and image paths to it. Decide the shared package
location from the dependency graph so renderer does not depend on hub-ui.
Keep existing local server routes and stdio preview behavior unchanged.
Run hub-ui and renderer tests and the existing local browser integration script.
The acceptance condition is unchanged dashboard operations and image decoding
through the real local hub, including revision conflict and reconnection cases.

### 3. Define grants and the remote wire contract

Resolve open decisions 2 through 4 and 7 before exposing a remote listener.
Add the separately versioned dashboard and enrollment contracts under `spec/`.
Define grants, redemption retries, revocation acknowledgement, malformed requests,
operation errors, snapshot boundaries, resource metadata, and byte-stream bounds.
Document explicit compatibility behavior with existing native state and readers.
Do not change SDK A2UI messages or gateway v4 to carry dashboard control messages.

### 4. Add personal remote access

Implement an opt-in hub listener and protocol dispatch independent of gateway
registration. Connect it to shared hub operations through an authenticated client
context. Add local invitation creation, grant listing, and revocation operations.
Implement browser link import, identity persistence, enrollment, and reconnection.
Load the WASM module only when an iroh connection is selected.
Personal access may ship as a separately named milestone; it does not establish
that dashboard-limited sharing is available.

### 5. Add dashboard-limited sharing

Enforce grants in list APIs, snapshots, incremental updates, action routing,
dashboard writes, and binary reads. Authorize captured subscriptions against
current dashboard references and the current grant generation. Scope every queued
operation so later revocation cancels pending work. Expose permission-aware UI,
but verify authorization with a client that sends forbidden operations directly.
Add deletion, reference removal, app generation replacement, recipient expiry,
and simultaneous grant-change tests. Verify the rights of a shared layout editor
cannot be expanded by submitting references outside its allowed set.

### 6. Verify hosted delivery and rollout

Build the same dashboard app for embedded local assets and static hosted assets.
Resolve open decision 6; use a relay compatible with the pinned transport and
verify certificates, reconnects, capacity limits, and metrics on the actual host.
Do not treat the deployment templates as operational verification.
Add acceptance runs across separate networks, including no registered remote
gateway, local-only apps, multiple recipients, sleeping browser tabs, relay
restart, hub restart, unavailable SDK apps, and revoked identities.

### Validation and completion criteria

- Local: existing HTTP Origin checks, WS, SSE, dashboard CRUD, image fetches,
  stdio preview, and offline operation retain their current behavior.
- Remote: an HTTPS page reaches a hub through a TLS relay, displays live views,
  forwards authorized actions, saves with revision checks, and transfers images
  and generic resources without requests to local hub HTTP addresses.
- Isolation: a one-dashboard grant reveals no other dashboard, service metadata,
  Surface state, resource bytes, or diagnostics through any operation.
- Lifecycle: expiry, revocation, dashboard deletion, and reference removal end
  pending subscriptions and reads; stale generations cannot dispatch actions.
- Enrollment: concurrent redemption has one winning identity, lost success replies
  are retryable by it, and consumed/expired secrets do not create new grants.
- Capacity: stopped readers, large images, malformed frames, and excess streams
  remain bounded and leave local dashboard access available.
- Recovery: a browser reconnect starts from a fresh snapshot and cannot replay
  an action or silently duplicate an uncertain layout write.

Planned commands include `bun run --filter @omochi-ui/hub-ui test`,
`bun run --filter @omochi-ui/hub-ui lint`,
`bun run --filter @omochi-ui/renderer test`,
`cargo test --manifest-path crates/Cargo.toml`, and the existing
`bun packages/hub-ui/test/browser.mjs`, plus new WASM/browser and grant tests.
These are planned verification steps, not results from this proposal-writing task.

Review reports belong in `docs/reviews/dashboard-remote-access/`. Findings must
distinguish proposal gaps, baseline implementation behavior, and verified test
results. The open release gates must be resolved before implementation is called
ready or sharing is advertised as available.
