---
title: "Configuration & storage"
description: "Configuration & storage · omochi"
sidebar:
  order: 5
---

| Setting | Default / behavior |
| --- | --- |
| Hub HTTP | `127.0.0.1:5555` |
| Development bridge | Port `5555` |
| Vite preview | Port `5173` |
| `OMOCHI_SERVICE` | Bundled examples select discoverable Unix sockets when set |
| Runtime namespace | `$XDG_RUNTIME_DIR/omochi`, private owned runtime directory |
| Native identity/trust | `$XDG_CONFIG_HOME/omochi/rust-v1` or `$HOME/.config/omochi/rust-v1` |
| Remote access | Off until explicitly enabled |
| `--allow-origin` | Widens hub HTTP/WebSocket origin permissions |
| Default online relay | Release: `https://relay.omochi.fyi` with built-in shared-token admission; debug: n0 |
| `--relay` | Replaces the default relay set; explicitly selecting production also authenticates automatically |
| `--offline` | No public lookup/relays; direct paths and LAN mDNS remain |

## Ownership and security

Runtime directory ownership and mode 0700 protect local sockets. Treat manifests as untrusted input: service IDs, sockets, and icons are validated before use. Do not place runtime directories on shared writable paths.

The development server has no authentication or TLS. The hub checks origins for both HTTP and WebSocket upgrades. Keep loopback binding unless external exposure is intended.

Native identities and trust are separate from legacy state. Do not copy old trust into the new store. Dashboard layouts are not application-state backups. See [remote setup](../../guides/remote/) before connecting machines.

## Connection and storage scopes

`--config-dir` or `OMOCHI_CONFIG_DIR` overrides native state. `--runtime-dir` or
`OMOCHI_RUNTIME_ROOT` selects the SDK registry. With `XDG_RUNTIME_DIR` unset or empty, the
runtime root defaults to `<system temporary directory>/omochi-<uid>/omochi`.
Unsafe runtime directories are rejected.

`hub --remote` enables owner/share clients over iroh; `gateway --remote` enables
app access for trusted Hubs. These permissions are separate. `--web-url HTTPS_BASE`
defaults to `https://app.omochi.fyi/` and requires `--remote`. A remote-enabled Hub
cannot use `--offline`.

The hosted browser keeps identity and registered connections in IndexedDB, with
Web Locks enforcing one owning tab. Public-share reload context lives in that
tab's sessionStorage. Owner and identity-share enrollment secrets are not saved.
Flutter uses OS-protected storage; explicitly saved public shares also retain
the bearer credential. See the [connection guide](../../guides/remote/).
