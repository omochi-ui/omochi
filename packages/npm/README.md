# Native npm stubs

The Rust implementation is in `crates/`. This directory contains only npm entry
points and distribution metadata:

- `cli/`: `@omochi-ui/cli`, exposing `omochi-ui`.
- `utils/`: `@omochi-ui/utils`, exposing `omochi-utils`.
- `launcher.cjs`: shared published launcher, copied into each npm tarball.

Checkout bin scripts call `scripts/native-run.sh`. They do not compile or download
anything implicitly. Build with `bun run build:native` first.

Source manifests are private and have version `0.0.0`, a workspace placeholder.
`package-native-npm.py` reads these stubs, derives the public version from
`crates/Cargo.toml`, and adds exact-version platform optional dependencies.
Do not add server logic, TypeScript transports, or independent release versions here.
Native MCP integration tests live in `tests/native/`; Rust tests live beside their
implementations. SDK/browser npm packages remain under their existing directories.
