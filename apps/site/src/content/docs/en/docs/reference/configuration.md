---
title: "Configuration & storage"
description: "Configuration & storage · omochi"
sidebar:
  order: 5
---

## Ports and paths

| Setting | Default |
| --- | --- |
| Hub HTTP/WebSocket | `127.0.0.1:5555` |
| Stdio development bridge | Port `5555` |
| Source-based Vite preview | Port `5173` |
| Runtime directory | `$XDG_RUNTIME_DIR/omochi/v2` |
| Native identity, trust, and dashboards | `$XDG_CONFIG_HOME/omochi/rust-v1`, or `$HOME/.config/omochi/rust-v1` |

`--config-dir` or `OMOCHI_CONFIG_DIR` changes native state storage. `--runtime-dir` or `OMOCHI_RUNTIME_DIR` selects the complete runtime directory. SDK transport options take precedence over the same environment variable. Without XDG, Unix uses `<system temporary directory>/omochi-<effective uid>/v2`. Windows defaults to LocalAppData KnownFolder `Omochi\Runtime\v2` for runtime and `Omochi\rust-v1` for native state.

SDK apps, Gateway and Hub must use the same directory. `up` exports `OMOCHI_RUNTIME_DIR` to its children; explicit paths can have any final component. `OMOCHI_SERVICE` is the service ID passed by `up`; bundled examples use it to select local discovery.

Keep Unix runtime directories owned by your user with mode 0700. Windows uses a protected current-user-only ACL. Shared writable directories are rejected. Dashboard files preserve layout, not live application state. See [Transports](../transports/) for the v2 upgrade procedure.

## Networking

| Option | Behavior |
| --- | --- |
| `hub --remote` | Enable remote owner and shared-dashboard clients |
| `gateway --remote` | Enable app access for registered remote Hubs |
| `--web-url HTTPS_BASE` | Hosted client base URL; defaults to `https://app.omochi.fyi/`; requires `--remote` |
| `--relay URL` | Replace the default relay set; repeatable |
| `--offline` | Disable public lookup and relays; keep direct connections and LAN mDNS |
| `--allow-origin ORIGIN` | Allow an additional exact Hub HTTP/WebSocket origin |

Distributed release builds use `https://relay.omochi.fyi`; source debug builds use n0 public relays. Use matching relay settings on Gateway, pairing, and Hub. Remote-enabled Hubs cannot use `--offline`.

`--web-url` accepts an absolute HTTPS URL of at most 2048 characters, without credentials, query, or fragment. Use it when hosting your own client.

HTTP/WebSocket listeners have no authentication or TLS. Keep loopback binding; remote clients connect without exposing the HTTP port. See [Connect machines](../../guides/remote/) for access management.

## Browser and app storage

Owner connections are saved in the browser. One tab at a time owns the browser identity. Clearing browser storage requires registering again. A sharing link can survive reloads in its current tab; reopen the original link after closing the tab or when browser storage is unavailable.

The mobile/desktop app saves connections in OS-protected storage. Saving a sharing link retains its access credential; temporary views do not save it. Deleting a browser/app connection does not revoke access at the Hub.
