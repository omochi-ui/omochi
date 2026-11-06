# SDK and browser releases

Public SDK/browser packages share a stable `vX.Y.Z` tag and version. Native
CLI/utils use an independent `native-vX.Y.Z` release; see the
[native release procedure](native-releasing.md).

## Packages and versions

Update these version sources together:

- npm: `packages/{core,renderer,hub-ui}/package.json` and
  `packages/sdk/typescript/package.json`.
- Python: `packages/sdk/python/pyproject.toml` and root `pyproject.toml`.
- Rust: `packages/sdk/rust/Cargo.toml`, its exact macro dependency and
  `packages/sdk/rust/macros/Cargo.toml`.

Regenerate affected Bun, uv and Cargo lockfiles without updating unrelated
dependencies. Private workspaces and native npm launcher stubs keep their
independent versions. Native manifests that depend on the Rust SDK must pin the
new exact SDK version when released from the same checkout.

## Verify and publish

```sh
bun install --frozen-lockfile
bun run release:check
bun run test:packaging
bun run test
bun run test:python
bun run test:rust
```

Commit the version changes and release notes, push `main`, then create and push
the stable tag at that commit. `bun run release:check vX.Y.Z` verifies that the
tag matches the checkout and versions and belongs to `origin/main`.

Publishing a stable GitHub Release starts `.github/workflows/release.yml`.
It builds/tests all three languages, verifies the packaged distributions, stages
the four npm packages, and publishes the Python SDK to PyPI and the macro/SDK
crates to crates.io. For a manual run, dispatch `release.yml` with the existing
tag, `registry: all`, `dry_run: false` and `bootstrap: false`; a dry run uses
`dry_run: true`. Avoid dispatching a second publication for the same release
while its Release event is running. Never move a published tag or replace an
existing registry version with different contents.

Normal publishing uses the configured Trusted Publishers and protected GitHub
environments. If native crates pin this SDK version, wait for successful SDK
crate publication before starting the native release workflow. SDK releases do
not publish GHCR images.

## npm approval

A successful npm staging job does not make the version installable. Review the
exact stage IDs and provenance in the job summary, then approve on npmjs.com
with maintainer 2FA, in this order: core, SDK, renderer, hub-ui. For native npm
packages, approve platform packages before CLI/utils. Approval is performed by
the maintainer; do not add approval credentials or enable direct publishing in
Actions.

On retry, the workflow skips identical published artifacts and rejects conflicting
contents. If an npm version is already staged, inspect and approve the expected
stage before retrying instead of creating another upload. PyPI and crates.io
publication can already be complete while npm approval is pending.

See the [0.2.1 release notes](releases/sdk-0.2.1.md).
