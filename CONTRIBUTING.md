# Contributing

Start with the [documentation map](docs/README.md), [architecture](docs/architecture.md) and [CLI reference](docs/cli.md)
to find the component you want to change. Development commands run from the
repository root unless stated otherwise.

## Setup

Use Bun 1.4.3 or newer, with the version pinned in `.bun-version` and
`package.json`. npm packaging smoke tests need Node.js 22+. The default test suite and Python changes need Python 3.11+; Rust changes need
current stable Rust and Cargo. Python checks and examples use uv. Discovery tests use Unix domain sockets and a Unix-like environment.

```sh
bun install
uv sync --locked
bun run build
```

Follow the [usage instructions](README.md#use-it) to open the dashboard.
For hub frontend work, run `bun run omochi-ui hub` and, in another terminal,
`bun run --filter @omochi-ui/hub-ui dev`. The frontend proxies the hub on port 5555;
set `OMOCHI_HUB` to use another address.

### Preview a stdio application

For SDK apps that use stdin/stdout, build the checkout and use two terminals at
its root. In terminal 1, start the browser frontend:

```sh
bun run --filter @omochi-ui/dev-server dev
```

In terminal 2, start the app and bridge:

```sh
bun run omochi-ui dev -- uv run --project examples/python examples/python/app.py
# Alternatives:
# bun run omochi-ui dev -- bun run --cwd examples/typescript start
# bun run omochi-ui dev -- cargo run --manifest-path examples/rust/Cargo.toml
```

Python examples require Python 3.11+ and uv. Open the frontend's printed URL,
normally `http://127.0.0.1:5173/`. The bridge defaults to port 5555; stop a Hub
using that port first or choose `dev --port 5557 -- COMMAND`. For a custom bridge
port, use its printed `?ws=` URL. The bridge does not serve the frontend.

To try the bundled development workspace, run `bun run quickstart`. It builds
and opens the preset in `examples/quickstart/omochi.toml`; start Vite from its
**Preview server** panel. Choose **Save as dashboard** to keep an editable layout.
Ctrl+C stops the processes started by the composition.

## Formatting

Run `bun run format` to format sources or `bun run format:check` to check them.
Select languages with `--only python --only web`; use `--list` to preview files.

| Language (`--only`) | Formatter | Setup |
| --- | --- | --- |
| Rust (`rust`) | `cargo fmt` | `rustup component add rustfmt` |
| Python (`python`) | Ruff | `uv sync --locked` |
| Shell (`shell`) | shfmt | `shfmt` on `PATH` |
| JS/TS, CSS, HTML, Astro (`web`) | Prettier | `bun install` |
| Kotlin, Gradle Kotlin (`kotlin`) | ktfmt | `ktfmt` on `PATH` or `KTFMT_JAR` |
| Java (`java`) | google-java-format | `google-java-format` on `PATH` or `GOOGLE_JAVA_FORMAT_JAR` |
| Dart (`dart`) | `dart format` | Dart SDK bundled with Flutter |
| Swift (`swift`) | swift-format | `swift-format` on `PATH` or `swift format` |
| C/C++/Objective-C (`c`) | clang-format | `clang-format` on `PATH` |

JAR variables point to executable JARs and require a compatible JDK. Git ignores
and `.formatignore` exclude vendored code, generated files and build output.

## Lint and type checks

```sh
bun run lint       # Linters
bun run typecheck  # Type checks
bun run check      # Formatting, lint and type checks
```

Use `lint:<scope>` or `typecheck:<scope>` for `web`, `python`, `rust`, or `mobile`.
Checks stop at the first failure. Run tests separately with `bun run test`.
Python type checks use the SDK contract suite with mypy and Pyright.

## Verification

| Change | Verification |
| --- | --- |
| JavaScript/TypeScript workspace | `bun run test` (builds all packages, then runs tests) |
| CLI after building dependencies | `bun run test:server-rust` |
| Python SDK | `bun run test:python` |
| SDK action/input types | `python scripts/test-sdk-types.py` with mypy/Pyright installed; TypeScript SDK tests; `cargo test --manifest-path packages/sdk/rust/Cargo.toml --doc` |
| Python renderer integration | `bun run test:python-renderer` |
| Generated Python catalog types | Build the native CLI, install `mypy` and `pyright`, then run `python scripts/test-catalog-types.py` |
| Rust SDK | `bun run test:rust` |
| Unix publisher ownership (Node, Bun, Python, Rust SDK, native utilities) | `bun run test:sdk-ownership` |
| Preview frontend | `bun run --filter @omochi-ui/dev-server lint` |
| Dashboard frontend | `bun run --filter @omochi-ui/hub-ui lint` |
| Hosted owner/share clients | `bun run test:dashboard-sharing` (requires the WASM toolchain and Chromium; see [setup](apps/dashboard/README.md)) |
| Documentation site | `bun run check:site`, `bun run build:site`, `bun run --filter @omochi-ui/site verify` |
| Flutter/native bridge | Follow the [mobile verification guide](apps/mobile/README.md#verification) and [external-Hub checks](apps/mobile/docs/remote-connections-validation.md) |
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

Write `README.md` in English. Put Japanese translations in a sibling
`README.ja.md`, link both versions to each other, and keep their content aligned.
Development documentation, including `spec/`, `docs/`, and application-local
`docs/` directories, is maintained in English only; do not add Japanese copies.

User guides live in `apps/site/src/content/docs/`: update Japanese and English
pages together and preserve the site's existing language and URL structure.
Public-facing site assets retain their intended language.

User documentation serves people running Omochi tools and developers building
applications with its SDKs. Focus on their current tasks and decisions; put
instructions for maintaining Omochi itself in development documentation. Apply
this distinction to the site's guides and to repository READMEs and guides linked
as usage instructions.

Keep guides focused on completing a task. Put advanced application-facing
behavior in dedicated public references, and keep test inventories and internal
module responsibilities in development documentation. Japanese and English user
documentation must cover the same features, procedures, and reference details;
do not send Japanese readers to English-only user references for missing details.

Keep the first-UI walkthrough centered on an installed Hub and utilities. SDK
guides should show language-package installation, publishing an application with
`UnixSocketServerTransport`, and displaying it in the Hub without cloning or
building Omochi. Document other transport choices in the transport reference.

Describe historical behavior only when readers need it to handle existing
configuration, applications, or saved data. Migration instructions must identify
the affected versions and required actions and address an actual migration need.
Omit obsolete, expired invitation formats and experiment-only historical
alternatives from current user guides. Do not add upgrade sections for changes
with no affected users.

Keep command flags/defaults aligned with `crates/cli/src/main.rs`
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
