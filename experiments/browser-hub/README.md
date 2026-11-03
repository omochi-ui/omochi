# Browser-to-gateway experiment

This isolated experiment asks whether a browser can perform the gateway-client
part of a hub without running `omochi-hub`. It uses the unchanged gateway v4 and
pairing contracts, the real Rust SDK and gateway crates, iroh 1.4.1 compiled to
WASM, and the existing React A2UI renderer.

```text
Chromium + WASM -> public TLS iroh relay -> native gateway -> Unix -> Rust SDK app
```

The HTTP server only serves built static assets. There is no native hub process,
no `/api`, resource, image, or application WebSocket proxy. The fixture process
hosts a gateway and a real SDK app whose button increments a counter and whose
image is an SDK-registered 1×1 PNG. Browser assertions use real Chromium through
its DevTools protocol; neither iroh nor renderer messages are mocked.

## Reproduce

From the repository root, install workspace JS dependencies using `bun install`
if needed. Requirements: Rust with `wasm32-unknown-unknown`, Bun, Chromium, network
access to n0 public relays, and wasm-bindgen CLI **0.2.130**, matching the lockfile.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
bun experiments/browser-hub/build.mjs
bun experiments/browser-hub/run.mjs
```

`WASM_BINDGEN` selects an existing CLI binary for the build script.
`CHROMIUM_BIN` selects the browser executable for the harness. Generated Rust,
WASM binding, static build, and latest-run artifacts are ignored. This is a
separate Cargo workspace with its own lockfile; it does not change the production
Cargo workspace, JS workspace dependencies, or native services.

The harness creates private temporary native state and a disposable browser
profile. The enrollment secret is passed in a fragment, removed by the page,
and omitted from evidence. It kills its child processes and removes the profile
on completion. A fresh run uses new identities rather than touching user trust.

## Checks

- Gateway rejects a browser identity before enrollment; the rejection must be an
  actual connection close, not a test timeout.
- Existing short-lived invitation enrolls that browser as a Hub peer.
- The authenticated gateway ID matches the intended target.
- Browser connection paths are relayed with no direct IP path; WSS traffic is
  observed by Chromium without certificate-verification bypass flags.
- The control stream supplies the service list and responds to heartbeat pings.
- A real Surface snapshot renders using the existing renderer.
- Clicking Increment updates the actual SDK state and sends a resulting update.
- Resource stat/read frames deliver PNG bytes; the existing renderer decodes and
  draws them, and the harness checks the canvas dimensions and nonempty pixels.
- Explicit reconnect replaces the renderer state from a fresh snapshot.
- Page reload restores the browser identity and reconnects without an invitation.
- Gateway `trust forget` acknowledgement is followed by browser connection loss.
- No application-data HTTP requests are made to the static host; no unhandled
  browser exceptions occur.

See [recorded results](RESULTS.md). Each run writes sanitized `results/result.json`
and `results/browser.png`. Recorded evidence is kept separately in `evidence/`.

## Limits

This is a feasibility experiment, not a browser SDK or a replacement for the
native hub. It tests one gateway, one service, one Surface, one PNG, and one
Chromium version. Native gateway and browser run on the same host, but their
application connection crosses a real public TLS relay. Separate physical
networks, mobile browsers, many gateways/tabs, suspension, relay outages, and
long-duration operation are not established by this run.

The browser key is exported to JavaScript and stored in disposable localStorage
for the identity-persistence test. This does not establish a production credential
storage or recovery design. Production code also needs strict message/roster
validation, bounded aggregate memory/stream concurrency, cancellation-safe timeout
handling, operation cleanup, backoff, and service-generation replacement tests.

The image adapter supports only SDK-backed PNG resources and reuses the renderer
decode/pixel bounds. It does not implement the native hub's public-HTTPS DNS
pinning/redirect fetch policy, other image formats, video/audio, generic HTTP
Range/HEAD delivery, or arbitrary custom catalogs. Form draft metadata is not
forwarded in this fixture's action; the action deliberately has no form inputs.

The board-document storage/share feature is not implemented. Existing local
hub/WS/stdio paths remain unchanged. A successful result establishes that a native
hub is not required for this tested browser-to-gateway path, not full feature
parity with every native hub function.
