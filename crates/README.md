# Native Rust workspace

This Cargo workspace implements the CLI, Gateway, Hub, utilities and native app
bridge. Cargo owns native versions and behavior; npm CLI/utils packages are thin
launchers. TypeScript, Python and Rust SDKs keep their own application-facing
packages. See [architecture](../docs/architecture.md), the
[specification index](../spec/README.md) and [distribution guide](DISTRIBUTION.md).

## Crates

| Directory | Package | Responsibility |
| --- | --- | --- |
| `protocol` | `omochi-protocol` | Gateway v4 framing, bounded snapshots, queues and generations |
| `downstream-core` | `omochi-downstream-core` | Transport-independent owner/share contracts and apply/ACK state |
| `mdns` | `omochi-mdns` | Platform discovery backends, aggregation and cancellation |
| `transport` | `omochi-transport` | iroh endpoints, identity, trust, invitation enrollment and persistence |
| `discovery` | `omochi-local-discovery` | Validate and watch local SDK service manifests |
| `gateway` | `omochi-gateway` | Shared SDK connections, snapshots, actions and resources over Unix/iroh |
| `hub` | `omochi-hub` | Aggregate Gateways, dashboards, local HTTP/WS/SSE, remote owner/share access and image policy |
| `cli` | `omochi-cli` | `omochi-ui` executable, compositions, stdio preview and catalog generation |
| `utils` | `omochi-utils` | Command surfaces, watches, processes, streams, logs, HTTP monitors and MCP |
| `native-bridge` | `omochi-native-bridge` | C ABI v2 for Flutter's embedded Hub and external-Hub client |

`downstream-core` and `native-bridge` are private implementation crates.
The [native release guide](../docs/native-releasing.md) defines which crates and
npm distributions are published.

## Build and verify

From the repository root:

```sh
bun install --frozen-lockfile
bun run build:native
bun run omochi-ui --help
bun run test:server-rust
cargo fmt --manifest-path crates/Cargo.toml --check
cargo clippy --manifest-path crates/Cargo.toml --all-targets -- -D warnings
```

The native build includes the embedded dashboard. Hosted WASM and Flutter builds
are separate; see [hosted dashboard](../apps/dashboard/README.md) and
[mobile app](../apps/mobile/README.md). Installed Cargo packages contain built UI
assets and do not require Bun at runtime.

[Contributing](../CONTRIBUTING.md#verification) lists the integration checks.
Real multicast resolution is explicitly opt-in:

```sh
cargo test --manifest-path crates/Cargo.toml mdns_resolves -- --ignored --nocapture
```

That test verifies same-host discovery without public lookup/relays. It does not
establish cross-network reachability. Dated [sharing acceptance](../docs/dashboard-sharing-acceptance.md),
[mobile validation](../apps/mobile/docs/validation.md) and
[benchmarks](../docs/benchmarks/native-local.md) record their own tested scope;
follow the deployment guides for host/network acceptance.

## Connection and storage boundaries

Local service discovery validates manifests in private owned runtime directories.
SDK Unix publishers exclude competing owners with a persistent lock inode; see
[local service ownership](../spec/local-service-ownership.md).

Remote Gateway access is opt-in and requires an authenticated EndpointId with
registered Hub role. Discovery is not authorization. Enrollment has a separate
ALPN from ordinary traffic. Revocation waits for all registered live processes to
reload authorization and destroy revoked sessions; acknowledgement failure stays
an error after durable removal. Native state lives under `omochi/rust-v1`; legacy
keys, trust and saved dashboards are not migrated.

`omochi-ui hub` serves loopback HTTP/WS/SSE and starts a local Gateway if needed.
`hub --remote` additionally accepts owner/share clients over iroh. Those grants
have separate stores and enrollment from Gateway trust. The hosted client serves
static assets, while the native Hub owns dashboards and authorization. See
[remote access](../docs/dashboard-remote-access.md) and
[sharing](../docs/dashboard-sharing.md).

The Flutter app reuses the Hub without its browser HTTP server through
[`omochi_hub.h`](native-bridge/include/omochi_hub.h). Host storage and lifecycle
adapters remain responsible for durable protected identity and orderly shutdown.
The direct browser-to-Gateway Hub is retained separately as an
[opt-in experiment](../experiments/browser-dashboard/README.md).

## Native artifacts

`bun run build:native:release` builds the dashboard and optimized CLI/utilities,
then creates an archive and checksum under `artifacts/native`. The archive
includes an installer and per-file checksums:

```sh
python3 scripts/smoke-native.py artifacts/native/<archive>.tar.gz
```

Use the [distribution guide](DISTRIBUTION.md) for installation and the
[native release guide](../docs/native-releasing.md) for crates.io/npm packaging.
