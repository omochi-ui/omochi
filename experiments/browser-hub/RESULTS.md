# Browser-to-gateway experiment results

Result: **PASS**, 16 browser assertions, on 2026-11-02.
Recorded run: 14:49:55–14:50:06 UTC.

The experiment demonstrates a browser connecting directly to an unchanged native
gateway implementation, using the existing enrollment and gateway v4 contracts,
without a native hub process or an application-data HTTP proxy. The browser
renders a real SDK Surface, invokes its action, reads an SDK image, reconnects,
and loses access when the gateway revokes its identity.

This establishes feasibility for the tested path. It does not establish full
native-hub feature parity, production readiness, or support across browsers.

## Evidence

- [Machine-readable browser result](evidence/2026-11-02/result.json).
- [Actual rendered Surface after an action and reconnection](evidence/2026-11-02/browser.png).
- [Harness and assertions](run.mjs).
- [Browser gateway client and renderer adapter](web/main.tsx).
- [WASM iroh/framing wrapper](wasm/src/lib.rs).
- [Real gateway and SDK fixture](fixture/src/main.rs).
- [Reproduction and limitations](README.md).

The result includes sanitized network requests, authenticated public IDs,
browser version, session events, and assertion outcomes. It contains no private
keys or invitation secrets. Browser profiles and fixture state are disposable.

## Environment and topology

| Item | Recorded value |
| --- | --- |
| Browser | Chromium 156.0.8078.73, Linux, headless |
| Rust | rustc 1.99.0 |
| Bun | 1.4.3 |
| iroh | 1.4.1 in browser and native gateway |
| WASM binding generator | wasm-bindgen 0.2.130 |
| Gateway contract | `omochi/gateway/4` |
| Enrollment contract | `omochi/pair/1` |
| Relay | Public n0 `https://aps1-1.relay.n0.iroh.link./` |
| Browser page | Static files on loopback HTTP; no backend application endpoints |
| Source app | Real Rust SDK, discovered by a real gateway over Unix |
| Native hub | Not linked into either experiment binary and not started |

Browser and gateway run on one Linux host, with the application connection forced
by the browser transport through the public relay. iroh reports a relay path and
no IP path; Chromium observes WSS `/relay` traffic and HTTPS relay probes. There
are no certificate-verification bypass flags. The run is not evidence of a
separate-physical-network deployment or hosting on app.omochi.fyi itself.

## Verified behavior

| Check | Evidence |
| --- | --- |
| Reject unenrolled browser | Peer closes with `not authorized (code 1)` before control data |
| Accept invitation | Existing gateway pairing returns accepted |
| Authenticate intended gateway | iroh remote ID and control hello match the target |
| Use relay-only connectivity | iroh path inspection and Chromium WSS request |
| Remove invitation from URL | Fragment cleared before normal operation |
| Avoid persisting invitation | Storage check; only test identity and gateway addressing retained |
| Decode SDK resource image | Existing renderer canvas is visible at 1×1 pixels |
| Draw actual image bytes | Canvas contains nonzero pixels from the red PNG |
| Invoke real app action | Increment changes SDK counter from 0 to 1 and updates the UI |
| Maintain control stream | Gateway responds to browser heartbeat ping |
| Reconnect with fresh snapshot | New connection and renderer restore counter 1 and image |
| Reload without invitation | Same persisted browser ID receives a new snapshot |
| Enforce gateway revocation | Native forget acknowledgement followed by browser connection close |
| Avoid hub application HTTP | Static host receives no `/api`, `/ws`, `/images`, `/resources`, `/services`, or `/events` requests |
| Observe relay WebSocket | Chromium records real WSS connection to relay |
| Avoid unhandled browser exceptions | No exception events in the recorded run |

The image uses a resource stat and chunk read through separate existing gateway
resource streams. No image URL is resolved against the static web host. The
fixture's gateway peer table registers the browser in the existing Hub role.
No new dashboard-client role, downstream ALPN, or Surface-sharing grant was added.

## Costs and porting observations

- The release WASM is 4,808,265 bytes, or 1,317,730 bytes with gzip. The Vite
  browser bundle is about 524 kB before compression. No download-performance
  conclusion follows from a loopback static host.
- With iroh defaults disabled, this pinned version also needs `tls-ring` selected
  to use `presets::Minimal`. Disabling defaults alone leaves no chosen crypto
  provider. This was found during the first compile and fixed in the wrapper.
- The wrapper deliberately does not depend on the native `omochi-hub` crate or
  desktop SDK dependency graph. It implements the existing bounded JSON frame
  header and exposes iroh streams; the browser consumes the existing v4 messages.
- The renderer's existing Surface component, resource catalog, image decode, and
  pixel limits are reused with an experimental resource-read adapter.
- The first browser attempt reached live Surface rendering but exposed a bad PNG
  fixture CRC and an idle-stream timeout in the experiment. Both were corrected
  before the recorded passing run. These were experiment defects, not evidence
  of a required native hub intermediary.

## What remains unresolved

The experiment does not test multiple gateways, multiple tabs, service-generation
replacement, mobile/background operation, gateway/relay restart, long-running
loads, or production persistence/recovery of browser credentials. It uses an
exportable key in disposable localStorage. Snapshot/roster validation, aggregate
memory and stream limits, cancellation-safe timeouts, backoff, and lifecycle
cleanup require production work.

Only an SDK-backed PNG and a button with no form input are exercised. External
HTTPS image fetching still needs an explicit architecture because browser fetch
does not reproduce the native resolver's DNS-pinning and redirect policy. Other
media, generic HTTP resource adapters, and custom catalogs are not established.

No board storage or board-sharing format has been implemented. The gateway grant
is the existing coarse trusted-Hub grant, not read-only or per-Surface access.
Unenrollment and shared board configuration must remain distinct concerns.

## Architectural implication

A mandatory server hub is not needed for this browser display/action/resource
path. A browser gateway client is a supported direction to design next, backed by
an actual end-to-end result. Keep native hub functions that require Unix access,
offline/local WS, native consumers, or native media policy as separate deployment
choices until those requirements are decided. The interrupted review of the
earlier server-hub-sharing proposal is not automatically resumed by this result.
