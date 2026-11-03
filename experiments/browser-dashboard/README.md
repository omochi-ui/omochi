# Browser dashboard experiment

The browser-resident hub implementation is retained here while the dashboard's
hub-client and sharing design is reconsidered. The ordinary dashboard remains in
`packages/hub-ui` and connects to the native hub. This experiment connects directly
to gateways using browser-held identities and gateway-wide trusted-Hub access;
it does not implement readonly or board-scoped sharing.

## Layout

- `hub/`: browser sessions, bounded snapshots, profiles, IndexedDB, PNG resources.
- `transport/`: standalone Rust/WASM workspace and TypeScript transport facade.
- `app/`: hosted entry, enrollment UI, browser board repository, test gallery.
- `tests/`: real-browser two-gateway integration harness.
- `hosted/`: static header contract, operator notes and historical acceptance.
- `build-transport.ts`: explicit WASM generation with pinned tooling.

The app reuses dashboard source from `packages/hub-ui` and the shared renderer.
These source imports are internal experiment wiring, not new public SDK exports.
Shared session interfaces, board codecs and native gateway authorization fixes
remain in the main workspace. The native fixture stays at
`crates/gateway/examples/browser_fixture.rs`.

The three private experiment packages remain in Bun's workspace for dependency
resolution. Normal `build:web`, `build:native` and `test` scripts select only
`packages/`; they do not build or run this experiment. Public SDK and native
packaging exclude its private packages and WASM assets.

## Commands

Run from the repository root after `bun install`:

```sh
bun run experiment:browser-dashboard build
bun run experiment:browser-dashboard test:unit
bun run experiment:browser-dashboard test:session
bun run experiment:browser-dashboard test:boards
bun run experiment:browser-dashboard test
```

`build` builds shared packages, both experiment entries and WASM. WASM generation
requires `wasm32-unknown-unknown` and wasm-bindgen CLI 0.2.130; `WASM_BINDGEN` can
select that binary. `test:unit` builds TypeScript and runs transport Rust/TS,
session/profile/media and browser repository unit tests without WASM generation.
`test` runs unit tests and every browser integration mode, including the ten-minute
load test. Individual modes are session, profile, media, boards, hosted, faults,
revocation and load. `CHROMIUM_BIN` selects Chromium 156+; `BROWSER_HUB_RESULTS`
preserves reports/screenshots. `--public-relay` selects the public-relay smoke path.

Build output is `app/dist-hosted/` and `app/dist-browser-test/`. To rerun a prepared
mode without rebuilding:

```sh
bun experiments/browser-dashboard/tests/browser-hub.mjs --mode session
```

This move preserves the IndexedDB name and Web Lock name; no identities, accepted
peers or boards are migrated or reset. Browser data still belongs to its original
origin; serving the app from another origin requires independent enrollment.

The [original connectivity spike](../browser-hub/README.md) remains separate and
unchanged. [Historical acceptance](hosted/acceptance.md) describes the earlier
implementation run, not a new acceptance of this relocation. The earlier design
and file paths are retained in [the design document](../../docs/dashboard-remote-access.md).
