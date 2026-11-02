# Browser image transport

The Rust Hub supports opt-in `images/1` on `/ws?images=1` (also `/?images=1` for a stdio preview). The renderer requests this version. A server without the extension keeps the legacy image behavior; it is outside the new image policy. There is no per-image fallback once the server announces the extension.

The new transport uses the common `SurfaceHandle::image` resolver for Image components and Markdown images in Text. Service icons are currently not rendered. Video, audio, custom catalogs, application assets and explicitly opened links are separate paths. Browser actions continue to use the existing route-based API; image handles do not upgrade their guarantees. Existing `/ws` messages, dashboard APIs and `/resources` GET/HEAD, Range and ETag behavior are unchanged.

## Subscription

The server first sends `{"omochi":"images/1","kind":"hello"}`. Every A2UI message is wrapped by the adapter:

```json
{"omochi":"images/1","kind":"update","sequence":1,"snapshotEnd":false,"handle":"opaque-surface-token","message":{"version":"v0.9","createSurface":{"surfaceId":"node~service/main","catalogId":"..."}}}
```

Sequence numbers start at 1. `message` and `handle` may be null; a null message with `snapshotEnd: true` ends the frozen snapshot. The renderer applies the message and sends `{"omochi":"images/1","kind":"ack","sequence":1}` before the server delivers another batch. The snapshot and following updates share an atomic cursor. A subscription has only one batch in flight and no separate unbounded socket event queue. Each browser has an independent subscription.

The adapter creates the outer envelope. Service-supplied fields inside `message` are never interpreted as transport controls. A handle is scoped to the subscription, service/connection generation, authorization period and Surface lifetime. Deletion, replacement, authorization revocation and fanout overflow invalidate it immediately, even while the browser waits to acknowledge an earlier update.

ACK timeout is 30 seconds, writes are limited to 5 seconds. Queue overflow closes with 1013; the renderer makes at most one automatic resnapshot attempt before asking for a reload. A capacity failure closes with 1009 and requires a reload. The adapter retains at most 4096 Surface handles with 8 MiB of Surface ID text per browser.

## Image requests

The renderer sends the resolved URI and its captured handle over this same socket:

```json
{"omochi":"images/1","kind":"image","id":1,"handle":"opaque-surface-token","uri":"omochi://resources/0123456789abcdef0123456789abcdef"}
```

The server checks the handle and returns either an opaque reference or a sanitized error:

```json
{"omochi":"images/1","kind":"image","id":1,"reference":"32-lowercase-hex-characters"}
```

A reference permits one GET to `/images/<reference>` within 60 seconds. The endpoint accepts no URL query, route or alternate destination. HEAD does not consume it. References cannot be issued through HTTP and cannot outlive their subscription or Surface. All routes retain the Hub's Origin policy. The browser fetches without credentials, referrer, redirects or caching. On completion or cancellation it sends `{"omochi":"images/1","kind":"release","id":1}`; closing the socket releases all its references and cancels active fetches.

References are limited to 64 per socket, 512 globally and 4 MiB of retained URI text, with 16 KiB per URI. The HTTP adapter has two delivery slots, held until the response body is released. The core also limits image reads to two. Responses include the validated MIME, `X-Omochi-Image-Width` / `X-Omochi-Image-Height`, `Cache-Control: no-store` and `X-Content-Type-Options: nosniff`. They do not support ranges or persistent caching. Reuse/expiry returns 410, busy returns 429, policy/format failures return 422, and external transfer/timeout returns 502/504. Error text does not contain the source URL.

The common resolver enforces public HTTPS:443, DNS pinning, redirect revalidation, no ambient proxy/cookies/authorization, encoded and decompressed 8 MiB limits, a 20 second fetch deadline and raster MIME/signature/dimension checks. SDK resources are read from the handle's service. The renderer checks the response bound again, decodes the first frame to an ImageBitmap and draws a canvas. It retains no object URL, network image element, animated decoder or persistent image cache. Its trial bounds are two concurrent fetch/decode operations, 4 M pixels per image and 32 MiB for bitmap plus canvas storage (eight bytes per pixel reserved conservatively). Pixel leases and canvases are released on replacement/unmount; a late decode cannot populate a new Surface.

## Verification

`cargo test --manifest-path crates/Cargo.toml -p omochi-hub --lib browser_images` exercises the actual HTTP/WS server and stdio resource stream, including independent subscriptions, untrusted control-like fields, applied ACK, origin rejection, a valid PNG, one-use references, forbidden HTTPS destinations and expiry before a pending UI ACK. The existing Hub resource/CRUD tests cover legacy compatibility.

`bun run --filter @omochi-ui/renderer test` checks credential-free opaque fetches, late-result cancellation, encoded/decoded budgets, Markdown's lack of direct image URLs and transport acknowledgements. `OMOCHI_NATIVE_TEST=1 bun test tests/native packages/renderer/test/native.test.ts` uses the real native binaries for MCP and renderer/actions/restarts. Browser layout and real browser decoding need a browser-enabled environment; the current automated renderer tests do not establish those results.
