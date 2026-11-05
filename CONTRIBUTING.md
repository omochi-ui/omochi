# Contributing

Start with the [documentation map](docs/README.md), [architecture](docs/architecture.md) and [CLI reference](docs/cli.md)
to find the component you want to change. Development commands run from the
repository root unless stated otherwise.

## Setup

Use Bun 1.4.3 or newer, with the version pinned in `.bun-version` and
`package.json`. npm packaging smoke tests need Node.js 22+. The default test suite and Python changes need Python 3.11+; Rust changes need
current stable Rust and Cargo. The Python example uses uv. Discovery tests use Unix domain sockets and a Unix-like environment.

```sh
bun install
bun run build
```

Follow the [quick start](README.md#quick-start) to run an app in the browser.
For hub frontend work, run `bun run omochi-ui hub` and, in another terminal,
`bun run --filter @omochi-ui/hub-ui dev`. The frontend proxies the hub on port 5555;
set `OMOCHI_HUB` to use another address.

## Verification

| Change | Verification |
| --- | --- |
| JavaScript/TypeScript workspace | `bun run test` (builds all packages, then runs tests) |
| CLI after building dependencies | `bun run test:server-rust` |
| Python SDK | `bun run test:python` |
| Generated Python catalog types | Build the native CLI, install `mypy` and `pyright`, then run `python scripts/test-catalog-types.py` |
| Rust SDK | `bun run test:rust` |
| Unix publisher ownership (Node, Bun, Python, Rust SDK, native utilities) | `bun run test:sdk-ownership` |
| Preview frontend | `bun run --filter @omochi-ui/dev-server lint` |
| Dashboard frontend | `bun run --filter @omochi-ui/hub-ui lint` |
| Hosted owner/share clients | `bun run test:dashboard-sharing` (requires the WASM toolchain and Chromium; see [setup](apps/dashboard/README.md)) |
| Documentation site | `bun run check:site`, `bun run build:site`, `bun run --filter @omochi-ui/site verify` |
| Flutter/native bridge | Follow the [mobile verification guide](apps/mobile/README.md#検証) and [external-Hub checks](apps/mobile/docs/remote-connections-validation.md) |
| Rust formatting | `cargo fmt --manifest-path packages/sdk/rust/Cargo.toml --check` |
| Cargo/npm packaging (also included in `bun run test`) | `bun run test:packaging` |
| Release versions and dependency graph | `bun run release:check` |
| npm distributions after building | `bun run release:pack && bun run release:smoke` (requires Node/npm) |

Run the checks affected by your change. Protocol messages, stable IDs,
reconciliation, and component catalogs must stay aligned across all three SDKs;
update their tests and shared fixtures in `spec/fixtures` together.

`bun run build` builds the JavaScript packages and native CLI/utilities.
`bun run build:native` builds just the browser dependencies and those native
binaries. Neither builds hosted WASM or Flutter. The default `bun run test` does
not replace the opt-in real-browser, multicast or mobile integration checks.

Native lifecycle tests must wait for observable readiness, command completion or
acknowledgement, rather than sleeping for an assumed processing time. Test timer
boundaries with Tokio's paused clock and controlled outcomes, without real child
processes. Verify suppression (no restart or no duplicate output) at the state
transition boundary; a short period without socket traffic is not proof. E2E
checks cover the wiring to real processes. Polling an explicit condition and
bounded timeouts to catch hangs are still appropriate.

After building, `bun packages/hub-ui/test/browser.mjs` runs the dashboard browser
smoke test with Chromium. Set `CHROMIUM_BIN` if necessary. Screenshots go to
`/tmp/omochi-hub-screenshots`, or `SCREENSHOT_DIR` when set.

Real multicast tests are opt-in because many environments cannot use multicast:

```sh
cargo test --manifest-path crates/Cargo.toml mdns_resolves -- --ignored --nocapture
```

Run those when changing mDNS integration or updating iroh/mDNS dependencies.

## Documentation changes

User guides live in `apps/site/src/content/docs/`: update Japanese and English
pages together. Keep command flags/defaults aligned with `crates/cli/src/main.rs`
and root/package scripts. Protocol changes belong in the relevant active contract
listed in [spec/README.md](spec/README.md).

Keep dated acceptance records and review baselines as historical evidence.
Describe their tested scope without presenting old results as fresh verification.
Link archived plans to current guides; current setup instructions must use active
package locations rather than pre-relocation experiment paths.

## Changes and pull requests

Keep changes focused and follow the surrounding language conventions. Do not
hand-edit generated `dist/` or `dist-test/` files. Put regression tests alongside
the affected package's existing tests. CLI changes should update command help
and the CLI reference; architecture or protocol changes should update the diagrams
and relevant guides.

Explain the problem, resulting behavior, affected SDKs, and verification in the
pull request. Include screenshots for renderer or dashboard changes and call out
intentional schema or fixture changes. Use short, imperative commit subjects.

Keep loopback defaults and origin checks intact. Never commit credentials, local
environment files, or pairing state. Runtime manifests are untrusted input; keep
service IDs, socket names, and icons validated. See [AGENTS.md](AGENTS.md) for
repository-specific implementation and security constraints.
