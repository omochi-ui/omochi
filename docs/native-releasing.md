# Native npm and crates.io releases

Native services release independently as `native-vX.Y.Z`. SDK/browser packages
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
npm install -g @omochi-ui/cli@0.3.1 @omochi-ui/utils@0.3.1
# Or: bunx @omochi-ui/cli@0.3.1 hub
omochi-ui hub

# Build from the published Rust sources instead:
cargo install omochi-cli --version 0.3.1 --locked
cargo install omochi-utils --version 0.3.1 --locked
```

npm wrappers need Node.js 22+ and select exact-version optional packages:
`@omochi-ui/native-{linux,darwin}-{x64,arm64}`. Each contains both native binaries.
There is no postinstall script, runtime download, compilation or Bun requirement.
Keep optional dependencies enabled. Linux prebuilds target glibc (built on Ubuntu
22.04); musl prebuilds are not provided. Native services currently support Linux
and macOS; Windows source builds are also unsupported. Native archives can still be
installed directly without Node. npm registry integrity covers the binary package.

Cargo publishes the eight native crates, including protocol, mdns, transport, discovery,
gateway and hub, because registry builds must resolve their dependencies. Internal
path dependencies have exact release version requirements. The existing Rust SDK
version must already be published. Hub dashboard assets are embedded in the `.crate`,
so `cargo install` does not need Bun, Node, or a checkout of the monorepo.

## Verify before release

```sh
bun install --frozen-lockfile
bun run build:native:release
python3 scripts/package-crates.py          # packages and builds all eight crates
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

### Routine release

Native 0.3.1 and SDK/browser 0.2.1 have separate tags at the same source commit.
See their [native](releases/native-0.3.1.md) and
[SDK/browser](releases/sdk-0.2.1.md) notes, including the required v1 sharing-link
replacement.

1. Update public SDK/browser manifests, the Python workspace and both Rust SDK
   crates together. Update the native workspace version and exact path dependency
   requirements separately, then regenerate affected Bun, uv and Cargo lockfiles.
2. Run both release source checks and packaging regression checks, commit the
   changes, and push `main`. Create and push `v0.2.1` and `native-v0.3.1` at that
   commit. Validate each tag using `release:check` / `release:native:check`.
3. Publish the SDK/browser GitHub Release first. Wait for its crates.io job to
   publish the SDK and macro crates before starting native crate packaging:
   native crates pin the exact SDK version from the checkout.
4. Publish the native GitHub Release. Its event starts native registry packaging
   and the Docker workflow, which publishes the tested amd64/arm64 GHCR image.
   Alternatively, dispatch those workflows with the native tag and
   `dry_run: false`. Avoid triggering both paths for the same release.
5. Review the workflow results and approve npm stages with maintainer 2FA,
   dependencies/platform packages before dependents/launchers. Confirm the
   versioned GHCR manifest lists both Linux platforms. There is no `latest` image
   tag; use `ghcr.io/omochi-ui/omochi:0.3.1`.

### First publication of the new crates

The initial SDK/browser `v0.2.0` and native `native-v0.3.0` rollout introduced
the macro and mDNS crates. Publish these two new crates locally before configuring
their trusted publishers. Commit the version and lockfile changes first and use that
same commit for both release tags: Cargo archives include the source commit,
and CI rejects an existing version with different bytes. Then run from the
repository root:

```sh
cargo publish --manifest-path packages/sdk/rust/macros/Cargo.toml --locked
cargo publish --manifest-path crates/Cargo.toml -p omochi-mdns --locked
```

These commands publish only `omochi-ui-macros 0.2.0` and `omochi-mdns 0.3.0`.
Both depend only on existing third-party crates, so neither needs the other
Omochi releases to be published first. Use local Cargo credentials for this
initial publication; no CI bootstrap token is needed for these two crates.

After publication, configure each crate's trusted publisher with GitHub owner
`omochi-ui`, repository `omochi`, and environment `crates-io`:

| Crate | Workflow filename |
| --- | --- |
| `omochi-ui-macros` | `release.yml` |
| `omochi-mdns` | `native-release.yml` |

Then publish SDK `0.2.0` before native `0.3.0`, whose crates pin that SDK version.
The SDK workflow checks the macro checksum and skips an identical already
published macro crate; the native publisher does the same for the mDNS crate.

### Registry workflows

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
3. Configure crates.io trusted publishing for `native-release.yml` on all eight new
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
