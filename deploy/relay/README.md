# Project relay deployment

See the [deployment overview](../README.md) for the `app.omochi.fyi` static client
and native hub setup. This relay is a separate, optional deployment; the hosted
dashboard can use the existing n0 relay defaults.

These templates target **iroh-relay 1.4.1**, matching the native application's
pinned iroh version. They prepare a public relay; no host has been provisioned.
The operator must supply a DNS hostname, machine and certificate contact.

Build the upstream binary with current stable Rust:

```sh
cargo install --locked iroh-relay --version '=1.4.1' --features server
```

Install that binary at `/usr/local/bin/iroh-relay`. Copy `relay.toml.example` to
`/etc/omochi-relay/relay.toml`, replace both `.invalid` placeholders, and install
`omochi-relay.service` under `/etc/systemd/system/`. Keep config owned by root.
Point the hostname's A record to the host. This template binds IPv4; do not
publish an AAAA record unless you configure and test IPv6 listeners too.

Open inbound TCP 80/443 and UDP 7842. The relay obtains a public certificate
using Let's Encrypt; TCP 443 must reach it directly for TLS-ALPN validation.
Do not place an ordinary HTTP reverse proxy in front of this configuration.
The unit sends SIGINT on stop, which upstream handles gracefully.
Metrics stay on loopback port 9090. The dynamic systemd user keeps certificate
state under `/var/lib/omochi-relay`; back up this private directory.

After configuring the host:

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now omochi-relay
sudo journalctl -u omochi-relay -f
curl --fail https://YOUR_RELAY_HOST/
curl --fail http://127.0.0.1:9090/metrics
```

Use `--relay https://YOUR_RELAY_HOST` on `gateway --remote`, `pair`, and `hub`.
These arguments select the project relay explicitly. Development defaults still
use n0 services, and online EndpointId lookup still uses n0 address discovery;
providing a relay does not replace that lookup service. `--offline` disables
public relay and address lookup entirely while preserving LAN discovery.

## Admission and capacity

`access = "everyone"` intentionally admits any iroh endpoint. Relay admission is
separate from Omochi authorization: gateway streams require registered peers,
and dashboard streams require a valid identity grant or public-link credential.
The upstream server rate-limits new connections to 10/s with a burst of 20 and
client ingress to 1 MiB/s with a 4 MiB burst. Adjust after measuring real workloads.
The systemd unit caps memory at 1 GiB, tasks at 256 and open files at 8192.
These are process protections, not a total bandwidth or billing cap. Configure
host/provider egress quotas and budget alerts before public distribution.

For a restricted rollout, replace `access = "everyone"` with an upstream
`access.allowlist = ["<64-hex-endpoint-id>", ...]` list containing participating hub,
gateway and browser identities. Public dashboard links use ephemeral browser
identities, so a fixed allowlist does not support arbitrary public recipients.
Restart the relay after policy edits; config is not hot-reloaded.
Do not store invitation bearer secrets in relay config or logs.

## Monitoring and acceptance

Merge the scrape job from `prometheus.yml.example` into the host's Prometheus
configuration and install `alerts.yml` at its listed path. Set an Alertmanager
receiver for the supplied critical/warning alerts. The example traffic alert is
8 MiB/s sustained for five minutes; choose a value matching the hosting budget.
Monitor relay availability, transmitted bytes, dropped packets, rate-limit events,
certificate renewal errors and systemd restarts. Metrics are not public endpoints.

The repository's `transport/tests/relay.rs` test creates a loopback relay,
disables both endpoints' IP transports and public address lookup, then enrolls a
hub and exchanges a 128 KiB framed payload. It asserts relay paths and absence of
IP paths. Run it with:

```sh
cargo test --release --manifest-path crates/Cargo.toml -p omochi-transport --test relay
```

This validates protocol behavior through a relay, not the deployed host's DNS,
certificate, firewall or network capacity. Before distributing the public URL,
verify pairing and service/action/resource traffic between two separate networks,
then verify reconnect after a relay restart and the expected traffic/rate limits.

The config smoke test runs the actual pinned relay with the same admission/rate
settings, loopback listeners and a temporary locally trusted certificate. It
verifies HTTPS without skipping certificate checks, checks every metric name used
by the alert rules, and exercises graceful shutdown:

```sh
python3 scripts/smoke-relay.py /path/to/iroh-relay
```

This test uses manual local certificates; public ACME issuance must be verified
on the deployed hostname. OpenSSL and Python 3.11+ are test prerequisites.
