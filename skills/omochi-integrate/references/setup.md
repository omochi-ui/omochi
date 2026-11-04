# Setup and browser connections

## Dependencies

The SDK and native CLI are separate distributions. Use an already installed
compatible SDK when available. Package names are `omochi-ui` on PyPI (import
`omochi_ui`), `@omochi-ui/sdk` on npm, and `omochi-ui` on crates.io (import
`omochi_ui`). Verify availability and API compatibility before selecting a
registry version: source examples may be ahead of published releases. The native
CLI's Cargo package is `omochi-cli`, while its executable is `omochi-ui`.

For a source dependency, obtain an omochi checkout separately from the user's
program. Build from its root:

```sh
git clone https://github.com/omochi-ui/omochi.git
cd omochi
bun install --frozen-lockfile
bun run build
```

This needs Bun 1.4.3+ and current stable Rust/Cargo. Python needs 3.11+; the Rust
SDK needs Rust 1.85+. Native services and Unix sockets require Linux/macOS.
See [installation](https://omochi.fyi/en/docs/start/installation/) for supported
distributions. Native archives include the dashboard and need no JS runtime.
The Vite development preview is separate and is not included in native archives.
`packages/npm/cli` and `packages/npm/utils` in a checkout are private launcher
stubs; use the built CLI or a verified distribution.

For local SDK dependencies in a separate target project (replace the absolute
checkout path):

- Python with uv: `uv add --editable /absolute/path/to/omochi/packages/sdk/python`.
- TypeScript: add `@omochi-ui/sdk` using a local link/file dependency on
  `/absolute/path/to/omochi/packages/sdk/typescript`, following the project's
  package manager. Build that SDK before consuming its `dist` exports.
- Rust: `omochi-ui = { path = "/absolute/path/to/omochi/packages/sdk/rust" }`.
  The example also needs `serde = { version = "1", features = ["derive"] }`,
  `serde_json = "1"` (used by the `ActionInput` derive), and
  `tokio = { version = "1", features = ["macros", "rt-multi-thread"] }`.

## Choose one connection

### Local dashboard: discoverable Unix socket

Start the program with `UnixSocketServerTransport` as shown in the language
reference. Those examples explicitly read `OMOCHI_SERVICE`; setting that
variable alone does not change the SDK's default transport. Choose a stable,
unique service ID such as `my-tool`.

Start `omochi-ui hub` and open `http://127.0.0.1:5555/`, create a dashboard, add
the app's surface, and save it. From a source checkout, the CLI equivalent is
`bun run omochi-ui hub`. The hub discovers running apps; it does not start the
program. A project composition can launch services with `omochi-ui up`; see
[dashboards](https://omochi.fyi/en/docs/guides/dashboards/).

### Single-app development preview: stdio

`app.start()` defaults to protocol JSONL over stdin/stdout. Reserve stdout for
protocol messages and send application logs, including libraries' output, to
stderr. Programs that require interactive stdin need a different transport or
an adapted input path.

In terminal 1, from a built omochi checkout:

```sh
bun run --filter @omochi-ui/dev-server dev
```

In terminal 2, run a command that starts the target program under the bridge,
for example (replace the paths):

```sh
bun run omochi-ui dev -- uv run --project /absolute/path/to/project /absolute/path/to/project/ui_app.py
```

For TS/Rust replace the command after `--` with the project's Bun/Node or Cargo
entry point. Leave `OMOCHI_SERVICE` unset for the dual-mode examples. Open
`http://127.0.0.1:5173/`. Stop both processes with Ctrl+C. The bridge uses port
5555, so stop a default-port hub first or select a different port and configure
the preview accordingly. `dev` starts the app and bridge, not the frontend.

### Existing server or custom transport

When integrating into an existing web/event-loop lifecycle, use `app.build()`
and the SDK's transport/runtime APIs instead of replacing that lifecycle.
TypeScript's `WebSocketServerTransport` is exported from `@omochi-ui/sdk/node`;
it serves the transport at `/ws`, not a browser frontend. See the
[SDK transport guide](https://omochi.fyi/en/docs/guides/sdks/#transports) before
implementing an adapter. Rust uses `start_with(transport)`; TS/Python use
`start(transport)`.

Keep local listeners on `127.0.0.1`; these HTTP/WebSocket services have no
authentication or TLS. Remote connectivity is a separate opt-in setup using
[pairing](https://omochi.fyi/en/docs/guides/remote/), only when requested.
