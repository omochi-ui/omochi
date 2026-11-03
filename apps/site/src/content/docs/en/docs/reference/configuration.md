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
