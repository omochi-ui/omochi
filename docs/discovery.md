# Discovery and connections

[Overview](../README.md) · [Architecture](architecture.md) · [CLI reference](cli.md)

Native discovery is implemented in `crates/discovery`, `crates/gateway`, and
`crates/transport`. SDKs continue to publish their existing service manifest and
Unix JSONL socket under the user's private Omochi runtime directory. Rust watches
that directory, validates manifests and shares connections to discovered apps.
The former `@omochi-ui/discovery` library is no longer part of the workspace;
already-published 0.1.x packages remain available for old installations.

## Local use

```sh
bun run build:native
bun run omochi-ui hub
```

The hub starts a local gateway when needed, and the dashboard defaults to
`http://127.0.0.1:5555`. Local-only startup does not create an iroh endpoint.
SDKs and utilities discover the same `$XDG_RUNTIME_DIR/omochi` service namespace.
Runtime directory ownership/permissions and manifest validation are required;
local reachability is not an authorization boundary across users.

## Remote pairing

On the gateway machine:

```sh
omochi-ui gateway --remote --pair --name workstation
```

On the hub PC:

```sh
omochi-ui pair --name workstation
# Paste the invitation on stdin, then Ctrl+D.
omochi-ui hub
```

The invitation is a bearer credential valid for 180 seconds, admitting one hub.
After enrollment the hub reconnects to registered gateway IDs automatically.
Users select names; ordinary connections do not require entering IP addresses.
The NodeId is the full 64-character lowercase iroh EndpointId.

Online development uses n0 lookup/relays by default. `--relay` selects project
relays. `--offline` disables public lookup and relays while keeping direct paths
and LAN mDNS. mDNS only locates known identities and grants no access.
A registered gateway that is offline does not block local services or other peers.

`omochi-ui status` reports connection attempts; `trust list` lists peers;
`trust forget NAME_OR_ENDPOINT_ID` revokes a peer and waits for live authorization
acknowledgements. Failures remain errors even when the durable trust removal has
already succeeded. Old TLS trust records are not migrated; re-pair devices and
recreate saved dashboards after the native breaking update.

See [the wire contract](../spec/gateway-v4.md) and
[relay deployment](../deploy/relay/README.md) for protocol and operations details.
For explicit LAN multicast validation:

```sh
cargo test --manifest-path crates/Cargo.toml mdns_resolves -- --ignored --nocapture
```
