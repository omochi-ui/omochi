# @omochi-ui/cli

Thin npm entry point for the Rust `omochi-ui` executable.
The implementation and release version live in `crates/`. This private checkout
manifest uses a placeholder version; the public manifest is generated from Cargo.

From a checkout, run `bun run build:native`, then `bun run omochi-ui`.
Published npm packages select an exact-version native platform package.
See [the command reference](../../../docs/cli.md) and
[registry releases](../../../docs/native-releasing.md).
