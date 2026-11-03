# Static browser preview

This static delivery setup belongs to the retained browser dashboard experiment.
See [the experiment README](../README.md) for its current scope.

Build with `bun run experiment:browser-dashboard build`. This requires the `wasm32-unknown-unknown`
target and the exact wasm-bindgen CLI version reported by the build script.
Serve `experiments/browser-dashboard/app/dist-hosted/` over HTTPS. No application backend is needed.
The build does not publish or provision a host or relay. This is a public-relay
preview without an uptime guarantee; invitations supply the gateway's TLS relays.

Apply [headers.json](headers.json) to all responses. Its CSP is exactly:

```text
default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self' https: wss:; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; worker-src 'none'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'
```

Also send `X-Content-Type-Options: nosniff` and
`Referrer-Policy: no-referrer`. Serve JavaScript as `text/javascript` and WASM as
`application/wasm`. HTML and stable `iroh/` JS/WASM assets use
`Cache-Control: no-cache`; only Vite-hashed `assets/` files use
`public, max-age=31536000, immutable`. Preserve MIME, cache and CSP headers when
applying gzip or Brotli compression. Copy `iroh/` intact alongside `index.html`.

The Vite base is relative, so a directory such as `/dashboard/` is supported.
Serve index.html only for that directory's root; asset requests must return the
requested file or 404, without redirects or an index fallback. This app uses
fragments, not path routing. It never proxies `/api`, `/images`, `/resources` or
WebSocket application endpoints through the static host.

The supported initial browser is secure-context Linux Chromium/Chrome 156+ with
IndexedDB, Web Locks, WebAssembly, AbortController, DecompressionStream and
createImageBitmap. Each origin has its own durable, exportable browser key and
requires independent enrollment. Only one tab may own the direct endpoint.
The host's JavaScript is trusted with keys and application data. Do not add
third-party scripts or telemetry. Treat both native and `#connect:` invitation
links as bearer secrets; issue a hosted link explicitly with
`omochi-ui gateway --remote --pair --web-url https://your-host.example/dashboard/`.

`bun run experiment:browser-dashboard test:profile` builds and tests this artifact with the same header
file over isolated loopback HTTPS and a controlled TLS relay. Set `CHROMIUM_BIN`
if Chromium 156+ is not on PATH. Set `BROWSER_HUB_RESULTS` to retain sanitized
reports/screenshots. The harness trusts only its one-run leaf SPKI, preserves no
user profile and removes temporary private state. Session-only tests use
`bun run experiment:browser-dashboard test:session` and a separate memory-only browser entry.
Separate-network and public-host operator acceptance remain rollout steps.

Run `bun run experiment:browser-dashboard test:hosted` to check nested `/dashboard/` URLs, every JS/CSS/
WASM response's prescribed headers with gzip, missing-file/API 404s, real SDK
actions and automatic recovery within 90 seconds after the controlled relay
resumes at its original TLS URL. No wildcard certificate exception is used.
The report records the browser version, artifact sizes and recovery duration.

`bun run experiment:browser-dashboard test:load` runs ten full minutes against two gateways/eight
services. The SDK emits 10000 incremental Text updates with exactly 1024 ASCII
payload bytes, distributed evenly across the services. Two PNG readers repeatedly
reload registered resources. The harness checks ordered authoritative apply,
latest DOM values, stable lease/scope counts, reservation high-water marks and
zero retained counters after joined stop. JS heap and Linux Chromium process-tree
RSS are diagnostics; neither is equated to encoded protocol budgets. Reports
contain counters and public fixture IDs, with no keys, invitations or raw frames.

`bun run experiment:browser-dashboard test` builds both entries and runs session, profile, media,
boards, hosted, faults, revocation and load modes in separate temporary fixture/browser profiles.
Set `BROWSER_HUB_RESULTS` for its aggregate `result.json` and per-mode screenshots;
the default for this complete run is `test-results/browser-hub/`. To repeat a
prepared mode without rebuilding, use `bun experiments/browser-dashboard/tests/browser-hub.mjs --mode
hosted` (or another mode). Add `--public-relay` for the external-relay smoke path;
that explicitly skips the controlled stop/restart assertion.

Before a public deployment claim, serve this same artifact on the intended HTTPS
host and test a recipient on another network: import a board, explicitly enroll
and connect both gateways, edit a form, display registered PNGs, interrupt and
restore relay access, reload, and revoke the recipient's exact browser ID. Check
headers, missing-file behavior and the absence of static-host app-data/API calls.
Record the two-host browser versions, artifact hashes and results separately.
Loopback acceptance does not establish separate-network behavior or an SLO.

Direct media accepts only SDK-registered `omochi://resources/<32 lowercase hex>`
PNG images. The initial format is non-interlaced 8-bit RGB/RGBA, with an 8 MiB
encoded limit, 8192 pixels per side and 4 Mi pixels total. CRCs, chunk structure
and exact inflated scanlines are checked before browser bitmap decoding. External
images, audio and video show unavailable; they are never fetched through the
static host or an unrestricted media element. Native mode retains its existing
media policy. Run `bun run experiment:browser-dashboard test:media` for actual canvas-pixel validation,
malformed-format cases, stat/read/decode cancellation and gateway revocation.

The hosted and native entries use the same dashboard editor. **Export board file**
shares a configuration-only JSON copy. **Import board file** previews advisory
source names and creates a new local ID/revision; it never pairs or selects a
gateway. Recipients use **Connections** to enroll and explicitly connect sources.
Deleting a board, forgetting a browser connection and revoking a gateway peer are
independent operations. Sender edits/deletion do not change exported copies.
Browser boards use origin-local IndexedDB; native boards keep their existing
HTTP-backed storage. No native file or identity is migrated automatically.

`bun run experiment:browser-dashboard test:boards` exercises both real repositories and the shared UI:
browser/native file round-trips, independent copies, CAS conflicts, quota/abort
failures, board-only tabs, scoped picker subscriptions and zero connection/trust
side effects before explicit enrollment/Connect.

`bun run experiment:browser-dashboard test:faults` drives partial headers/bodies, silent/interrupted/
stalled snapshots, invalid control frames, same-ID recreation, generation
replacement and maximum-u64 action/resource opens over real QUIC. A fixture-only
adversarial peer forwards real SDK snapshots and changes the wire at explicit
gates. It never replaces the production listener in authorization tests.
`bun run experiment:browser-dashboard test:revocation` uses the production listener and a bounded Unix
proxy that stalls an actual SDK write. It verifies delayed revocation ACKs,
already-dispatched work, removal of queued remote work, retained Local work and
an independently usable B gateway. No fault flag is added to the production CLI.

See [acceptance.md](acceptance.md) and [acceptance.json](acceptance.json) for the
recorded local run, artifact hashes, counter peaks and rollout limits.
