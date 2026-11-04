# omochi-mdns

Cross-platform mDNS advertisement and discovery for Iroh endpoints. This crate
owns backend selection, publication, record aggregation and invalidation, and
cancellation. It does not depend on Omochi transport, enrollment, or trust state.

`omochi_mdns::LanDiscovery::new(endpoint.id(), local_port).await?` creates an Iroh
`AddressLookup`. Attach it with `endpoint.address_lookup()?.add(discovery)`;
Iroh then publishes endpoint data and resolves peer IDs through it. `backend()`
returns the selected backend name for diagnostics. Bind IPv4 and IPv6 to the
same UDP port before attaching the lookup: native DNS-SD advertises a single
SRV port. Pass the actual bound port (for example from `endpoint.bound_sockets()`),
not a port inferred from `EndpointData`: published addresses may also include
NAT mappings with different ports. Those mappings do not change or withdraw the
native LAN advertisement. An empty IP address set still withdraws it.

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
addresses and a single SRV port; callers must use one UDP port for both IP families.
Incoming multi-SRV/multi-address records are aggregated, and link-local IPv6
addresses retain their interface scope. Discovery is only an address source:
QUIC authenticates the exact EndpointId, and application role/trust checks still
control access. TXT contains only public relay/user data.

Each endpoint owns its native registration, browsers, callback storage, and
bounded record/query queues. Dropping it cancels queries and withdraws its
registration. Removed services and backend resets invalidate record generations;
late callbacks cannot recreate an old service. Native backends watch address and
service changes; Windows also rebuilds discovery on interface/address changes.
Windows retains early address records in a bounded shared cache, but each service
receives only its own SRV/TXT records and the addresses of its SRV targets on the
same interface. Other peers therefore do not consume its record limit.

Record aggregation and lifecycle regression tests run without multicast:

```sh
cargo test --manifest-path crates/Cargo.toml -p omochi-mdns
```

The [transport probes](../transport/README.md) verify integration with endpoint
binding, authenticated QUIC, and real platform discovery on LAN-capable hosts.
