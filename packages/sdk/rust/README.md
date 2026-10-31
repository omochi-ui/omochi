# Omochi UI for Rust

Attach browser UIs to Rust programs with the declarative A2UI runtime.

```sh
cargo add omochi-ui
```

Requires Rust 1.85 or newer. The CLI is distributed separately as
`@omochi-ui/cli` on npm (Node.js 22+ launcher), or as a native archive.

See the [SDK guide](https://github.com/omochi-ui/omochi/blob/main/docs/sdk.md)
and [project README](https://github.com/omochi-ui/omochi) for examples.

The next minor release introduces `ActionInput` derive and typed component
builders. The SDK depends on the exact matching `omochi-ui-macros` version.
See the SDK guide for migration and custom Serde expression adapters.

Licensed under Apache-2.0.
