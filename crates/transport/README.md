# omochi-transport

Iroh transport and device enrollment for Omochi.

See [Omochi](https://github.com/omochi-ui/omochi) for usage and migration instructions.

`bind()` constructs endpoints for Gateway, Hub, and the mobile native bridge,
then attaches `omochi_mdns::LanDiscovery`. The [omochi-mdns crate](../mdns/README.md)
owns advertisement, discovery, record aggregation/invalidation, cancellation,
and OS backend selection. Applications do not need a separate implementation.

Native DNS-SD advertises one SRV port, so `bind()` uses one UDP port for both IP
families. Discovery only supplies addresses; QUIC authenticates EndpointIds and
application role/trust checks control access.

For a real LAN check, run `cargo run --manifest-path crates/Cargo.toml -p omochi-transport --example lan_probe -- serve` on one host, then `... -- connect
<EndpointId>` on the other. The probe disables relays, uses an ephemeral identity,
prints the selected backend, resolves only the public ID, and verifies a QUIC
ping/pong with that authenticated ID. On Linux, `--rust` explicitly exercises the
legacy Rust peer for interoperability. These are opt-in network checks, not
ordinary unit tests; permit the probe's inbound UDP traffic in the host firewall
when testing incoming connections.

`lan_probe lifecycle` checks native advertisement updates, withdrawal, and reuse
of the same EndpointId after a restart. It publishes public test addresses only;
it does not enroll peers or create trust records.
