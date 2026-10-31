# Omochi native services

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

Online development uses n0 public discovery and relays. For a project relay, use
the same --relay https://your-relay.example option on remote gateway, pair and hub
commands. This archive does not provision a relay. The source repository includes host
configuration and monitoring templates in deploy/relay.

status reports registered gateway connection attempts and failures from the running
hub (use --port if the hub has a non-default port). It does not initialize state.

trust list shows peers. trust forget NAME_OR_ENDPOINT_ID waits for live processes
to destroy revoked connections. An acknowledgement failure remains an error even
though the trust removal is already durable.

## Breaking upgrade

Stop old gateways/hubs first. SDK APIs and manifests are unchanged; gateway wire
protocol and NodeIds are new. Pair devices again and recreate saved dashboards.
Native state lives at $XDG_CONFIG_HOME/omochi/rust-v1 or
$HOME/.config/omochi/rust-v1. Legacy state is left untouched.

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
