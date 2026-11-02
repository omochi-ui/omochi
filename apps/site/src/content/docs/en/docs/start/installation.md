---
title: "Installation"
description: "Installation · omochi"
sidebar:
  order: 1
---

## Build the current source

The source checkout is the reproducible path for these guides. Use Linux or macOS, Bun 1.4.3+, and current stable Rust/Cargo. Python examples require Python 3.11+ and uv.

```sh
git clone https://github.com/omochi-ui/omochi.git
cd omochi
bun install --frozen-lockfile
bun run build
```

The build compiles the SDK/browser packages and native CLI. Node.js is not required for this source workflow. Unix socket discovery requires a Unix-like system; Windows is not currently supported by native services.

## Native release archives

Download an OS/architecture-compatible archive and its `.sha256` file from [GitHub Releases](https://github.com/omochi-ui/omochi/releases). Verify the checksum, extract the archive, then run:

```sh
./install.sh  # defaults to $HOME/.local/bin
omochi-ui hub
```

The archive contains both `omochi-ui` and `omochi-utils`, including the built dashboard. The binaries need no Bun, Node.js, Python, or Rust runtime. Applications they launch still need their own runtime. Linux archives target glibc; macOS binaries are not codesigned or notarized.

## Registry distributions

Native npm/Cargo release packaging is implemented, but these docs do not assume a version has been published. Check the release notes and registry before choosing a package version. Do not publish or install the private `packages/npm` source stubs as distributions. npm launchers require Node.js 22+; native archives do not.

Continue with [your first UI](../first-ui/).
