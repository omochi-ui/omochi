# Contributing

Start with the [architecture](docs/architecture.md) and [CLI reference](docs/cli.md)
to find the component you want to change. Development commands run from the
repository root unless stated otherwise.

## Setup

Use Bun 1.4.3 or newer, with the version pinned in `.bun-version` and
`package.json`. npm packaging smoke tests need Node.js 22+. The default test suite and Python changes need Python 3.11+; Rust changes need
current stable Rust and Cargo. The Python example uses uv. Discovery tests use Unix domain sockets and a Unix-like environment.

```sh
bun install
bun run build
bun run build:native
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
| Rust formatting | `cargo fmt --manifest-path packages/sdk/rust/Cargo.toml --check` |
| Cargo/npm packaging (also included in `bun run test`) | `bun run test:packaging` |
| Release versions and dependency graph | `bun run release:check` |
| npm distributions after building | `bun run release:pack && bun run release:smoke` (requires Node/npm) |

Run the checks affected by your change. Protocol messages, stable IDs,
reconciliation, and component catalogs must stay aligned across all three SDKs;
update their tests and shared fixtures in `spec/fixtures` together.

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
