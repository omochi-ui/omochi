---
title: "Installation"
description: "Installation · omochi"
sidebar:
  order: 1
---

## Start the hub

To display an SDK app or utility, start a hub on the same computer. On Linux or macOS with Node.js 22+, run from any directory:

```sh
npx @omochi-ui/cli hub
```

Open **http://127.0.0.1:5555/** . The CLI includes the browser UI and automatically starts a local gateway. Keep the terminal running while using the hub.

You do not need a separate gateway for apps on the same computer. To connect apps on another machine, see [remote connections](../../guides/remote/).

## Install for regular use

Install the CLI once to use the command directly:

```sh
npm install -g @omochi-ui/cli
omochi-ui hub
```

Or install with current stable Rust/Cargo:

```sh
cargo install omochi-cli --locked
omochi-ui hub
```

The Cargo package is `omochi-cli`; the executable is `omochi-ui`. Both distributions include the hub, gateway, and dashboard UI. The `omochi-ui` Cargo crate is the separate SDK.

Installing an SDK or utility does not start a hub. Your application uses its language's SDK; start the hub with the command above. Run utilities in another terminal with `npx @omochi-ui/utils ...`, or install them with `npm install -g @omochi-ui/utils` or `cargo install omochi-utils --locked`.

npm downloads prebuilt binaries and needs no Rust compiler. Keep optional dependencies enabled. Native services support Linux (glibc) and macOS on x64/arm64; Windows is not supported. Check `hub --help` for available options.

Continue with [your first UI](../first-ui/) to display a surface.

## Native release archives

Download an OS/architecture-compatible archive and its `.sha256` file from [GitHub Releases](https://github.com/omochi-ui/omochi/releases). Verify the checksum, extract the archive, then run:

```sh
./install.sh  # defaults to $HOME/.local/bin
omochi-ui hub
```

The archive contains both `omochi-ui` and `omochi-utils`, including the built dashboard. The binaries need no Bun, Node.js, Python, or Rust runtime. Applications they launch still need their own runtime. Linux archives target glibc; macOS binaries are not codesigned or notarized.

To develop Omochi itself or run its bundled examples, see [Contributing](https://github.com/omochi-ui/omochi/blob/main/CONTRIBUTING.md).
