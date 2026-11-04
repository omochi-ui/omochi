# omochi-transport

Iroh transport and device enrollment for Omochi.

See [Omochi](https://github.com/omochi-ui/omochi) for usage and migration instructions.

LAN discovery is selected in `bind()` for Gateway, Hub, and the mobile native
bridge. Applications do not need a separate discovery implementation.

| OS | Production backend |
| --- | --- |
| macOS / iOS | System Bonjour (`DNSServiceBrowse`, `DNSServiceQueryRecord`, `DNSServiceRegister`) |
| Windows 10+ | System DNS-SD (`DnsServiceBrowse`, `DnsServiceRegister`, `DnsServiceDeRegister`) |
| Linux with Avahi Server2 on the system D-Bus | Avahi D-Bus; no Avahi development libraries required |
| Linux without Avahi / system D-Bus, Android, other targets | Rust `iroh-mdns-address-lookup` / patched `swarm-discovery 0.6.5` |

Linux probes the existing Avahi daemon at endpoint creation without activating
or installing it. An active Avahi backend clears its cache and reconnects after
a daemon restart. Apple and Windows do not fall back to raw multicast sockets.
iOS still requires the app's local-network usage description and `_irohv1._udp`
in `NSBonjourServices`, but no multicast entitlement.

All backends use the existing `_irohv1._udp.local.` contract and lowercase
Base32 EndpointId instance names. Native DNS-SD publishes the system host's
addresses and a single SRV port; `bind()` uses one UDP port for both IP families.
Incoming multi-SRV/multi-address records are aggregated, and link-local IPv6
addresses retain their interface scope. Discovery is only an address source:
QUIC authenticates the exact EndpointId, and application role/trust checks still
control access. TXT contains only public relay/user data.

Each endpoint owns its native registration, browsers, callback storage, and
bounded record/query queues. Dropping it cancels queries and withdraws its
registration. Removed services and backend resets invalidate record generations;
late callbacks cannot recreate an old service. Native backends watch address and
service changes; Windows also rebuilds discovery on interface/address changes.

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
