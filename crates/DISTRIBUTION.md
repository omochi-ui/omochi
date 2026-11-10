# Omochi native services

## Start without a checkout

On Linux, macOS or Windows 11 with Node.js 22+:

```sh
npx @omochi-ui/cli hub
```

Open http://127.0.0.1:5555/. The hub includes the dashboard UI and starts a local
gateway automatically. Add your running SDK app or utility with **Add view**.
SDK apps must use the discoverable `LocalServerTransport`.

For regular use, install `npm install -g @omochi-ui/cli`, or build the published
Rust package with `cargo install omochi-cli --locked`, then run `omochi-ui hub`.
The CLI includes both hub and gateway commands. The `omochi-ui` Cargo crate is
the separate SDK. Utilities run with `npx @omochi-ui/utils`, or install them with
`npm install -g @omochi-ui/utils` or `cargo install omochi-utils --locked`.

## Native archive

The archive contains omochi-ui and omochi-utils for the target in manifest.json.
The production dashboard is embedded. Neither binary requires Bun, Node.js,
Python or Rust at runtime. Applications launched by them need their own runtime.

Verify the archive against its .sha256 file, extract it, and install both binaries:

    ./install.sh                       # $HOME/.local/bin
    ./install.sh /your/preferred/bin   # optional destination
    omochi-ui hub

The default dashboard is http://127.0.0.1:5555. Keep this loopback default.
Use omochi-ui up omochi.toml for compositions, and omochi-utils --help for utility
modes. dev bridges application stdio to the separate developer preview frontend;
the Vite developer preview is not included in this archive.

## Pairing

On the gateway machine:

    omochi-ui gateway --remote --pair --name workstation

Import its invitation on the hub PC:

    omochi-ui pair --name workstation
    # Paste the invitation on stdin, then Ctrl+D.
    omochi-ui hub

Possession of an invitation grants access. It expires after 180 seconds and admits
one hub; keep it private. Registered gateways connect automatically without IP
input. --offline disables public discovery and relays while retaining LAN mDNS
and direct connections.

Release builds use https://relay.omochi.fyi with built-in shared-token admission.
Debug builds use n0 public relays for development. Both use n0 public address
discovery when online. To override the relay set, use the same --relay URL option
on remote gateway, pair and hub commands. The built-in token is sent only to the
production relay, including when selected explicitly in a debug build.
The source repository includes host configuration and monitoring templates in
deploy/relay.

status reports registered gateway connection attempts and failures from the running
hub (use --port if the hub has a non-default port). It does not initialize state.

trust list shows peers. trust forget NAME_OR_ENDPOINT_ID waits for live processes
to destroy revoked connections. An acknowledgement failure remains an error even
though the trust removal is already durable.

## Breaking upgrade

Stop publishers, gateways and hubs together before the local IPC v2 upgrade.
SDKs now use `LocalServerTransport` and runtime format 2. Explicit old runtime
paths require a new empty directory; legacy manifests are not read or migrated.
Service IDs are lowercase and stored without encoding. Existing native identities
and trust are retained; this local IPC change does not require pairing again.
See `spec/local-service-ownership.md` in the repository for API migration.

Unix native state lives at $XDG_CONFIG_HOME/omochi/rust-v1 or
$HOME/.config/omochi/rust-v1; Windows uses the LocalAppData known folder's
Omochi\rust-v1. Windows archives include `install.ps1`; run
`./install.ps1` to install in `$env:LOCALAPPDATA\Programs\Omochi\bin`, or
`./install.ps1 -Prefix "$HOME/bin"` in PowerShell to select an install directory.

Linux binaries use their build runner's libc; choose an OS/architecture-compatible
archive and libc, or build from source. This script does not codesign or notarize
macOS binaries.

## Registry distributions

Native releases also prepare npm `@omochi-ui/cli` and `@omochi-ui/utils` under
the existing package names, using exact-version platform binary dependencies.
The npm launcher needs Node.js 22+; the native archives above do not. Published
Cargo sources can be installed with `cargo install omochi-cli --locked` and
`cargo install omochi-utils --locked`. The existing `omochi-ui` crate remains the SDK.
See docs/native-releasing.md in the repository for release setup and migration.
