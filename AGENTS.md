# Repository Guidelines

## Project Structure & Module Organization

This workspace implements the same declarative A2UI runtime in three languages. TypeScript lives in `packages/sdk/typescript/src`, Python in `packages/sdk/python/src/omochi_ui`, and Rust in `packages/sdk/rust/src`. The shared browser renderer is in `packages/renderer`, the development preview is in `packages/dev-server`, and the dashboard frontend is in `packages/hub-ui`; TypeScript SDK/browser transport helpers are in `packages/core`; native server implementations are the Cargo workspace in `crates/` (`protocol`, `transport`, `discovery`, `gateway`, `hub`, `cli`, `utils`); `packages/npm/cli` and `packages/npm/utils` are thin launcher stubs with no server implementation; runnable integrations are under `examples/`. Shared protocol contracts and conformance data belong in `spec/`, especially `spec/ir.schema.json` and `spec/fixtures/basic.json`. Keep cross-language behavior aligned, and do not hand-edit generated `dist/` or `dist-test/` output.

## Build, Test, and Development Commands

- `bun install`: install workspace JavaScript dependencies (Bun 1.4.3+).
- `bun run build`: build JavaScript packages and the native Rust binaries.
- `bun run test`: run SDK/browser, native Rust, native client integration and sample composition tests.
- `bun run test:server-rust`: run the Rust server workspace tests.
- `bun run test:native-mcp`: verify the MCP client against native services.
- `bun run test:python`: run Python `unittest` tests with the local package on `PYTHONPATH`.
- `bun run test:rust`: run the Rust crate tests.
- `cargo test --manifest-path crates/Cargo.toml mdns_resolves -- --ignored --nocapture`: explicitly run real-multicast discovery verification.
- `bun run --filter @omochi-ui/dev-server dev`: start the Vite frontend during renderer work.

For an end-to-end example, build first, then run `bun run omochi-ui dev -- uv run --project examples/python examples/python/app.py`.

## Coding Style & Naming Conventions

Follow the existing language idioms: two-space indentation, semicolons, and `camelCase` in TypeScript; four spaces and `snake_case` in Python; standard `rustfmt` layout and `snake_case` in Rust. Use `PascalCase` for exported types and classes. Name modules after responsibilities (`transport`, `reconcile`, `catalog`) and preserve equivalent concepts across SDKs. Run `bun run --filter @omochi-ui/dev-server lint` for frontend linting and `cargo fmt --manifest-path packages/sdk/rust/Cargo.toml` for Rust formatting.

## Testing Guidelines

TypeScript uses `bun:test` through the disposal helper in `@omochi-ui/core/testing`, Python uses `unittest`, and Rust uses built-in `#[test]`/`#[tokio::test]`. Put tests in each package's `test` or `tests` directory and follow existing names such as `runtime.test.ts`, `test_runtime.py`, and descriptive Rust function names. Changes to protocol messages, IDs, reconciliation, or component catalogs should update all affected SDK tests and the shared fixture. No numeric coverage threshold is configured; prioritize exact message-sequence and regression assertions.

## Commit & Pull Request Guidelines

Use short, imperative commit subjects such as `align rust reconciliation fixtures`, keeping each commit focused. Pull requests should explain behavior and affected SDKs, list verification commands, and link relevant issues. Include screenshots for renderer/UI changes and call out intentional schema or fixture changes.

## Security & Configuration

The dev server is unauthenticated and has no TLS. Keep its default `127.0.0.1` binding unless external exposure is explicitly required, and never commit credentials or local environment files.

Local service discovery trusts `$XDG_RUNTIME_DIR` being mode 0700: reaching a service's `a2ui.sock`, or the gateway's `gateway.sock`, is the only authentication. Because WebSocket upgrades are exempt from CORS, the hub restricts both upgrades and its HTTP endpoints to loopback origins unless `--allow-origin` widens the list — keep that check when touching `crates/hub`. A manifest is written by an arbitrary local process, so keep validating `serviceId`, `socket`, and `icon` before acting on one.

Remote access is opt-in. iroh authenticates exact EndpointIds; application authorization also requires a registered peer role. Discovery is not authorization. Keep invitation enrollment separate from ordinary gateway traffic, and never advertise bearer secrets. Native identities and trust are separate from legacy state; require re-pairing rather than migrating trust.

Store writes are serialized across processes and re-read under the lock. `trust forget` succeeds only after all registered live processes acknowledge updated authorization and destruction of revoked connections; acknowledgement failure must remain an error. Do not restore deleted records from stale caches. Keep generation guards and bounded frame/queue sizes. See `spec/gateway-v4.md` for the active wire contract.

The Rust workspace is the source of truth for native versions and behavior. npm stub manifests use private placeholder versions; public manifests and exact platform dependencies are generated from Cargo. Keep SDK/browser releases separate from native releases. Do not add service implementations or legacy TypeScript dependencies to npm stubs.
