# Native client integration tests

These tests exercise the Rust binaries through external client protocols. MCP uses
the TypeScript MCP client; renderer integration stays in `packages/renderer/test`.
They do not import any former TypeScript server implementation.

Run `bun run test` for the complete suite, or `bun run test:native-mcp` for the MCP
path. `OMOCHI_NATIVE_PROFILE=release` selects release binaries for CI. Client test
dependencies are root development dependencies and are never published in stubs.
