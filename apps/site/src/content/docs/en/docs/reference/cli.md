---
title: "CLI reference"
description: "CLI reference · omochi"
sidebar:
  order: 2
---

Examples use the [installed](../../start/installation/) `omochi-ui` command. You can also run `npx @omochi-ui/cli COMMAND`.

| Command | Purpose |
| --- | --- |
| `up [FILE]` | Start a project's services and dashboard preset |
| `dev -- COMMAND [ARGS...]` | Bridge a stdio app to a renderer |
| `hub` | Display and manage dashboards |
| `gateway` | Publish this machine's running apps to Hubs |
| `pair --name NAME` | Register a Gateway invitation |
| `status` | Inspect live Gateway connections |
| `id [--hub]` | Print the Gateway or Hub identity |
| `trust list` | List registered peers |
| `trust forget NAME_OR_ENDPOINT_ID` | Revoke a peer |
| `generate catalog` | Generate typed catalog bindings |

Use `--help`, `-h`, or `help COMMAND` to print usage without starting services. Invalid arguments exit with code 2.

## up

Start the services, Hub, and dashboard preset declared in TOML:

```sh
omochi-ui up --open
omochi-ui up --port 5557 ./omochi.toml
```

The default file is `./omochi.toml`. Commands run without a shell and inherit the environment. Relative working directories are resolved from the configuration file. Child services receive `OMOCHI_SERVICE`; apps must select their discoverable transport to use it. Utility shorthand supports `button`, `watch`, `process`, `tail`, and `http`. Run stdin-owned `stream` and agent-owned `mcp` separately.

```toml
version = 1
id = "local-development"

[services.health]
utility = ["http", "http://127.0.0.1:3000/health"]

[services.app]
command = ["python", "app.py"]
cwd = "./backend"

[dashboard]
name = "Local development"

[[dashboard.rows]]
panels = ["health", "app"]
height = "medium"

[dashboard.panels.health]
title = "Health"
service = "health"

[dashboard.panels.app]
title = "Application"
service = "app"
```

Rows divide twelve columns evenly. Heights are `small`, `medium`, or `large`; the default Surface is `main`. The preset is read-only until you choose **Save as dashboard** to make an editable copy. One service exiting leaves its view unavailable without stopping others. Ctrl+C stops the services and Gateway started by `up`, leaving previously running processes alone.

| Option | Meaning | Default |
| --- | --- | --- |
| `--host HOST` | Hub bind address | `127.0.0.1` |
| `--port PORT` | Hub port; `0` selects an available port | `5555` |
| `--allow-origin ORIGIN` | Add an exact browser origin; repeatable | Loopback origins |
| `--open` | Open the preset in the browser | Off |

## dev

`omochi-ui dev -- COMMAND [ARGS...]` launches an app and exposes its JSONL stdin/stdout through WebSocket. It needs a separately running renderer. See the [stdio preview setup](https://github.com/omochi-ui/omochi/blob/main/CONTRIBUTING.md#preview-a-stdio-application) for the source-based Vite frontend.

Arguments after `--`, including `--help`, belong to the app. Send app logs to stderr. `--host` defaults to `127.0.0.1`; `--port` defaults to `5555` and accepts `0` for an available port. Use the printed `?ws=` URL with the renderer. The bridge has no authentication or TLS; keep loopback binding.

## hub

```sh
omochi-ui hub
```

Open **http://127.0.0.1:5555/** and add running [SDK apps](../../guides/sdks/) or [utilities](../../guides/utils/). The Hub includes its screen and starts a local Gateway if needed. It reuses an existing Gateway without taking ownership of it.

| Option | Meaning | Default |
| --- | --- | --- |
| `--host HOST` | HTTP/WebSocket bind address | `127.0.0.1` |
| `--port PORT` | HTTP/WebSocket port; `0` selects an available port | `5555` |
| `--no-auto-gateway` | Do not start a missing local Gateway | Auto-start enabled |
| `--allow-origin ORIGIN` | Add an exact allowed browser origin; repeatable | Loopback origins |
| `--remote` | Accept remote owner/shared clients; conflicts with `--offline` | Off |
| `--web-url HTTPS_BASE` | Hosted client URL; requires `--remote` | `https://app.omochi.fyi/` |
| `--offline` | Disable public lookup and relays; retain direct connections and LAN mDNS | Off |
| `--relay URL` | Replace relay URLs; repeatable | See [configuration](../configuration/) |
| `--assets DIR` | Override the dashboard assets | Embedded screen |

Registered Gateways reconnect independently in the background. Use [remote connections](../../guides/remote/) for owner access and [dashboard sharing](../../guides/dashboards/#share-a-saved-dashboard) for one-board access. Keep HTTP/WebSocket on loopback; remote clients do not need its port exposed.

## gateway

```sh
omochi-ui gateway
omochi-ui gateway --remote --pair --name workstation
```

Publishes running apps under the same user. Start the apps separately; view them through a Hub.

| Option | Meaning | Default |
| --- | --- | --- |
| `--remote` | Accept registered remote Hubs | Off |
| `--pair` | Issue a three-minute invitation; requires `--remote` | Off |
| `--web-url HTTPS_BASE` | Hosted client URL for invitations; requires `--remote` | `https://app.omochi.fyi/` |
| `--name NAME` | Display name | `gateway` |
| `--offline` | Direct connections and LAN mDNS without public lookup or relays | Off |
| `--relay URL` | Replace relay URLs; repeatable | See [configuration](../configuration/) |

`--pair` prints native and HTTPS invitation links and a QR. Either link can be imported by `pair`; share them privately. See [Connect machines](../../guides/remote/) for the browser and app paths.

## pair

On the machine running the Hub:

```sh
omochi-ui pair --name workstation
# Paste an invitation on stdin, then Ctrl+D.
omochi-ui hub
```

Accepts the printed native or HTTPS invitation, either on stdin or as a positional argument. Stdin avoids shell history. An invitation lasts three minutes and admits one Hub; the same identity may retry an interrupted exchange.

`--name` is the local Gateway alias. `--hub-name` defaults to `hub`. `--offline` selects LAN-only enrollment; `--relay URL` selects a custom relay. Use matching networking options on `gateway`, `pair`, and `hub`.

## status

`omochi-ui status` prints JSON with each registered Gateway's EndpointId, alias, connection status, or last error. Offline peers remain visible while reconnecting. It queries the running Hub on `127.0.0.1:5555`; use `--host` and `--port` to change that target. An empty array means no remote connection attempts are tracked.

## id and trust

```sh
omochi-ui id
omochi-ui id --hub
omochi-ui trust list
omochi-ui trust forget workstation
```

`id` prints the persistent Gateway identity; `--hub` selects the Hub identity. Identities are full 64-character lowercase hexadecimal EndpointIds. Names are local aliases.

`trust forget` accepts an EndpointId or a unique name. It removes the registration and waits for running local processes to close the revoked connections. An acknowledgement failure reports an error even if removal was saved; see [troubleshooting](../../understand/troubleshooting/#revocation-reports-an-error). Pair again to register a removed peer.

## generate catalog

```sh
omochi-ui generate catalog --catalog ./catalog.json --name ProductCatalog \
  --typescript ./src/product.ts --python ./src/product.py --rust ./src/product.rs
```

Create output directories first and select any subset of languages. `--check` reports missing or stale output without writing. See [Catalog generation](../catalog-generation/) for all flags and schema support.

See [configuration](../configuration/) for storage paths and [troubleshooting](../../understand/troubleshooting/) for common errors.
