# Native npm and crates.io releases

Native services release independently as `native-v0.2.0`. SDK/browser packages
retain their existing `vX.Y.Z` release train. The native version is owned by
`crates/Cargo.toml`; generated npm manifests use that same version.
No registry publication happens when running ordinary build/package/test commands.

## Existing package names

| Package | Native transition |
| --- | --- |
| npm `@omochi-ui/cli` | Keep the name; 0.2.0 is a native `omochi-ui` launcher |
| npm `@omochi-ui/utils` | Keep the name; 0.2.0 is a native `omochi-utils` launcher |
| npm `@omochi-ui/discovery` | Keep already-published 0.1.x; exclude from new releases |
| npm SDK/core/renderer/hub-ui | Continue the existing SDK/browser release workflow |
| crates.io `omochi-ui` | Keep as the Rust SDK; do not replace it with a CLI |
| crates.io `omochi-cli` | New package, installs the `omochi-ui` executable |
| crates.io `omochi-utils` | New package, installs the `omochi-utils` executable |

No old version is unpublished or automatically deprecated. Users pinned to 0.1.x
can keep using it. Native 0.2.0 is intentionally breaking: stop old services,
re-pair devices and recreate saved dashboards. Direct imports of old server-side
TypeScript modules are not part of the new CLI package contract.

Rust implementation and version metadata live in `crates/`. The private source
stubs in `packages/npm/cli` and `packages/npm/utils` contain only a manifest and
checkout launcher, with placeholder version `0.0.0`. Do not publish those folders
directly. `scripts/package-native-npm.py` reads their metadata, takes the release
version from Cargo, and bundles `packages/npm/launcher.cjs` into the public wrappers.
The old TypeScript services are retained in Git history, not the active workspace.

## Installation after publication

```sh
npm install -g @omochi-ui/cli@0.2.0 @omochi-ui/utils@0.2.0
# Or: bunx @omochi-ui/cli@0.2.0 hub
omochi-ui hub

# Build from the published Rust sources instead:
cargo install omochi-cli --version 0.2.0 --locked
cargo install omochi-utils --version 0.2.0 --locked
```

npm wrappers need Node.js 22+ and select exact-version optional packages:
`@omochi-ui/native-{linux,darwin}-{x64,arm64}`. Each contains both native binaries.
There is no postinstall script, runtime download, compilation or Bun requirement.
Keep optional dependencies enabled. Linux prebuilds target glibc (built on Ubuntu
22.04); musl prebuilds are not provided. Native services currently support Linux
and macOS; Windows source builds are also unsupported. Native archives can still be
installed directly without Node. npm registry integrity covers the binary package.

Cargo publishes the seven native crates, including protocol, transport, discovery,
gateway and hub, because registry builds must resolve their dependencies. Internal
path dependencies have exact release version requirements. The existing Rust SDK
version must already be published. Hub dashboard assets are embedded in the `.crate`,
so `cargo install` does not need Bun, Node, or a checkout of the monorepo.

## Verify before release

```sh
bun install --frozen-lockfile
bun run build:native:release
python3 scripts/package-crates.py          # packages and builds all seven crates
python3 scripts/package-native-npm.py artifacts/native/*.tar.gz
bun run release:native:smoke:npm
```

The npm pack command normally requires all four platform archives from the same
commit. For host-only testing, pass `--partial`; such a bundle cannot be staged.
`package-crates.py --allow-dirty` permits local verification; dirty bundles also
cannot be published. `--no-verify` is only for inspecting crate contents and is
rejected by the publisher. Both publishers verify artifact checksums and refuse
an already-published version with different bytes.

The crate packager and publisher check Git source cleanliness before staging the
dashboard. Cargo itself receives `--allow-dirty` because explicitly included
gitignored assets count as changes to Cargo. This does not allow source edits in
a release; the bundle manifest records source-tree cleanliness separately from
Cargo's generated-asset VCS dirty flag.

## GitHub Actions and registry setup

`.github/workflows/native-release.yml` builds and tests four platform archives,
installs the npm tarballs outside the checkout with install scripts disabled,
and packages/builds the actual Rust crate archives before either publisher runs.

Run `workflow_dispatch` with an existing `native-vX.Y.Z` tag on main and
`dry_run: true` first. A stable GitHub Release with that tag, or a manual run with
`dry_run: false`, enables publishing. `registry` selects npm, crates, or both.
The existing SDK release workflow ignores `native-v` release events.

Registry-side setup is required once (workflow files do not grant registry access):

1. Reuse the protected GitHub environments `npm` and `crates-io`. If their
   deployment rules restrict tags, allow `native-v*` as well as the existing `v*`
   and `main` rules. Use GitHub owner `omochi-ui`, repository `omochi`.
2. Configure npm trusted publishing for `native-release.yml` on cli/utils and all
   four platform packages. Those wrapper names previously used `release.yml`;
   move their trusted-publisher workflow to the native workflow, leaving SDK/core/
   renderer/hub-ui on the existing workflow. New platform names need first-publication
   credentials; use a narrowly scoped `NPM_BOOTSTRAP_TOKEN` environment secret and
   the workflow's `bootstrap` option, then remove it after OIDC setup.
3. Configure crates.io trusted publishing for `native-release.yml` on all seven new
   native crates. Keep the Rust SDK's publisher on `release.yml`. Use a temporary
   `CRATES_IO_BOOTSTRAP_TOKEN` in the `crates-io` environment for first publication.
4. npm uses the same staged publication policy as SDK releases. Review the exact
   stage IDs in the job summary and approve with maintainer 2FA: platform packages
   first, then cli/utils. CI does not approve stages. On retry, published identical
   tarballs are skipped; pending-stage conflicts must be reviewed, not bypassed.
5. Cargo publishes the verified dependency graph. Re-runs skip identical published
   versions and refuse mismatched checksums before making another registry write.

The workflow uploads archives as CI artifacts. It does not change existing registry
owners, delete old releases, or automatically deprecate `@omochi-ui/discovery`.
