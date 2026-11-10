# Omochi UI for Rust

Attach browser UIs to Rust programs with the declarative A2UI runtime.

```sh
cargo add omochi-ui
```

Requires Rust 1.85 or newer. The CLI is distributed separately as
`@omochi-ui/cli` on npm (Node.js 22+ launcher), or as a native archive.

The [SDK guide](https://omochi.fyi/en/docs/guides/sdks/#rust) shows a complete app
published with `LocalServerTransport` and displayed in an installed Hub.
Use `ActionInput` derive for typed action expressions and component builders for
views. See [typed inputs](https://omochi.fyi/en/docs/reference/typed-inputs/)
for Serde projections and custom expression adapters.

The default `runtime` feature keeps the complete SDK API. Embedded renderers that
only materialize protocol messages can disable default features to use
`A2uiSnapshot` and `SnapshotCursor` without local-server or resource-file code.

Licensed under Apache-2.0.
