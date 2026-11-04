---
title: "CLI reference"
description: "CLI reference · omochi"
sidebar:
  order: 2
---

Native npm releases retain this package name; Cargo releases use `omochi-cli`.
See [registry installation and migration](https://github.com/omochi-ui/omochi/blob/main/docs/native-releasing.md).

[Overview](https://github.com/omochi-ui/omochi/blob/main/README.md) · [Architecture](/en/docs/understand/architecture/) · [Discovery guide](/en/docs/guides/remote/)

Run the CLI from any directory with Node.js 22+ on Linux or macOS:

```sh
npx @omochi-ui/cli hub
npx @omochi-ui/cli --help
```

Or install once with `npm install -g @omochi-ui/cli` or
`cargo install omochi-cli --locked`, then use `omochi-ui` directly as below.
The dashboard UI is included, and `hub` starts a local gateway automatically.
See [installation](../../start/installation/) for supported platforms and source builds.
From a built checkout, the equivalent command is `bun run omochi-ui`.

`-h`, `--help`, and `help <command>` print help to stdout and exit successfully
without starting services or changing config.
Invalid arguments print usage to stderr and exit with code 2.

## Command map

| Task | Command | Browser UI included? |
| --- | --- | --- |
| Start a project composition | `up [FILE]` | Yes; `--open` opens it automatically |
| Develop a single app | `dev -- <command> [args...]` | Start the Vite preview separately |
| Compose dashboards from running apps | `hub` | Yes |
| Publish this machine's running apps | `gateway` | No; connect a hub |
| Show or name this machine | `id` | No |
| Register a remote gateway | `pair --name workstation` | No |
| Inspect live remote connection attempts | `status [--port PORT]` | No |
| Inspect or remove paired peers | `trust list`, `trust forget <nodeId>` | No |
| Generate SDK catalog types | `generate catalog` | No |

## up

Start local services, the hub, and a read-only dashboard preset from TOML.

```sh
omochi-ui up --open
omochi-ui up --port 5557 ./examples/quickstart/omochi.toml
```

The file defaults to `./omochi.toml`. A service uses either a literal `command`
or an `omochi-utils` shorthand. Commands do not use a shell; relative working
directories are resolved from the composition file. Child services receive
`OMOCHI_SERVICE` and remain independent: one exiting leaves its panel unavailable
without stopping the others. Utility shorthand supports `button`, `watch`,
`process`, `tail`, and `http`. Stdio-owned `stream` and `mcp` processes stay
external; this lets an agent connect and disconnect independently of the preset.

```toml
version = 1
id = "local-development"

[services.health]
utility = ["http", "http://127.0.0.1:5173/"]

[services.app]
command = ["uv", "run", "app.py"]
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

Rows divide the twelve-column dashboard evenly. Heights are `small`, `medium`,
or `large`; a panel's surface defaults to `main`. Presets do not modify saved
dashboard storage. Use **Save as dashboard** in the hub to make an editable copy.
Ctrl+C stops only the services and gateway started by this command.

| Option | Meaning | Default |
| --- | --- | --- |
| `--host HOST` | Hub bind address | `127.0.0.1` |
| `--port PORT` | Hub port; `0` chooses an available port | `5555` |
| `--allow-origin ORIGIN` | Add an exact browser origin; repeatable | Loopback origins only |
| `--open` | Open the selected preset in the desktop browser | Off |

## dev

Launch a child application and bridge its JSONL stdin/stdout to WebSocket.

```sh
# Terminal 1: browser preview
bun run --filter @omochi-ui/dev-server dev
# Terminal 2: app + bridge
omochi-ui dev -- uv run --project examples/python examples/python/app.py
```

Open `http://127.0.0.1:5173/`. The preview's default `/ws` proxy reaches the bridge.
All arguments after `--` belong to the child, including any `--help` flag.
Application logging must go to stderr; stdout is reserved for A2UI JSONL.

| Option | Meaning | Default |
| --- | --- | --- |
| `--host HOST` | Bridge bind address | `127.0.0.1` |
| `--port PORT` | Bridge port; `0` chooses an available port | `5555` |

The bridge serves WebSocket at `/ws`; it does not serve an HTML page. Startup
prints a preview URL with `?ws=` for its actual address. The preview must already
be running; use its printed port if Vite selected one other than 5173.
All clients share application state, and late clients receive a current snapshot.
The bridge has no authentication or TLS; keep the loopback default.

## hub

Serve the dashboard UI, discover available surfaces, and route browser actions.
Apps must already be running with `UnixSocketServerTransport`. The bundled
examples select that transport when `OMOCHI_SERVICE` is set.

```sh
# Terminal 1
OMOCHI_SERVICE=python-example uv run --project examples/python examples/python/app.py
# Terminal 2
omochi-ui hub
```

Open `http://127.0.0.1:5555/`. The hub serves its own UI and automatically starts
a local gateway when none is listening. It reuses an existing gateway and does
not stop a gateway it did not start.

| Option | Meaning | Default |
| --- | --- | --- |
| `--host HOST` | HTTP/WebSocket bind address | `127.0.0.1` |
| `--port PORT` | HTTP/WebSocket port; `0` chooses an available port | `5555` |
| `--no-auto-gateway` | Do not start a missing local gateway | Auto-start enabled |
| `--allow-origin ORIGIN` | Add an exact allowed browser origin; repeatable | Loopback origins only |
| `--remote` | Accept dashboard clients over iroh; conflicts with `--offline` | Off |
| `--web-url HTTPS_BASE` | Static hosted hub-client UI base; requires `--remote` | `https://app.omochi.fyi/` |
| `--offline` | Disable public address lookup and relays; retain LAN mDNS | Off |
| `--relay URL` | Custom relay URL; repeatable | Release: `https://relay.omochi.fyi`; debug: n0 public relays |
| `--assets DIR` | Override the embedded dashboard assets | Embedded assets |

Every registered gateway connects in the background; an unavailable gateway does
not delay local startup or other peers. Registration changes are picked up while
the hub runs. With no registered peers the hub creates no remote endpoint unless `--remote`
explicitly enables downstream dashboard access.
The browser-facing HTTP/WebSocket listener has no TLS or authentication; keep
its loopback binding. Origin checks apply to both HTTP and WebSocket requests.

Remote dashboard sharing uses `hub --remote`. The hosted client defaults to
`https://app.omochi.fyi/`; use `--web-url HTTPS_BASE` to override it.
Create identity invitations or reusable readonly links in the local dashboard’s
**Share dashboard** dialog. The hosted browser connects to this native hub;
recipients do not pair with its gateways. Release hubs default to
`https://relay.omochi.fyi`; browsers use the sharing link’s relay URLs and apply
the same production relay admission policy. Dashboard authorization remains
separate from relay admission.

## gateway

Discover this user's running SDK apps and publish their surfaces. The gateway
does not launch apps or serve the dashboard UI. Remote networking is opt-in:

```sh
omochi-ui gateway
# Enable remote access and issue an invitation:
omochi-ui gateway --remote --pair --name workstation
```

| Option | Meaning | Default |
| --- | --- | --- |
| `--remote` | Enable the authenticated iroh endpoint | Off |
| `--pair` | Issue a temporary invitation; requires `--remote` | Off |
| `--web-url HTTPS_BASE` | Also print a browser invitation for this dashboard base; requires `--remote` | `https://app.omochi.fyi/` |
| `--name NAME` | Gateway display name | `gateway` |
| `--offline` | LAN mDNS/direct connections without public services | Off |
| `--relay URL` | Custom relay URL; repeatable | Release: `https://relay.omochi.fyi`; debug: n0 public relays |

mDNS locates endpoints; it never grants access. Only registered hubs can open
application streams. On LANs that permit multicast and direct traffic, registered
peers can reconnect without internet access. Across networks, iroh uses direct
connections where possible and a relay otherwise.

For the hosted browser dashboard, use `gateway --remote --pair`. The web URL
defaults to `https://app.omochi.fyi/`; for self-hosting, set
`--web-url https://app.example.com/`. The base must be an
absolute HTTPS URL of at most 2048 characters with no credentials, query or
fragment. The command prints both the native invitation and the browser link
only when `--pair` issues a new invitation.
Both links carry the same temporary bearer secret: share them privately. Accepting
one enrolls the browser for all exposed services, actions and resources on that
gateway. Removing its browser connection record does not revoke gateway trust;
use `trust forget <browser-endpoint-id>` at the gateway to revoke it.

## pair

Import the gateway's invitation on the PC running the hub:

```sh
omochi-ui pair --name workstation
# Paste the invitation on stdin, then Ctrl+D.
omochi-ui hub
```

An invitation can also be the positional argument. Stdin keeps it out of shell
history. Possession grants access: the invitation expires after 180 seconds and
admits one hub. The same winning identity can retry an interrupted exchange.
`--name` is the local gateway alias; `--hub-name` defaults to `hub`.
Use `--offline` for LAN-only enrollment or `--relay URL` for a custom relay.
Configure matching relay options on gateway, pair and hub.
Release builds authenticate to the production relay automatically. Debug builds
can select it with `--relay https://relay.omochi.fyi`. Its built-in shared token
is never sent to other relay URLs. Public address lookup still uses n0.

Users do not enter IP addresses. Registered gateways reconnect by identity;
the alias makes them recognizable. Endpoint identity is the full 64-character
lowercase hexadecimal iroh EndpointId. This is a breaking protocol change:
pair old devices again and recreate saved dashboards.

## status

`omochi-ui status` reads connection diagnostics from the running hub
on `127.0.0.1:5555`. It prints JSON with each registered gateway’s EndpointId,
local alias and current transport status or last connection error. Offline peers
remain visible while reconnecting. Use `--port` (and `--host` for a different
bind address) when necessary. This command does not create identity or trust state.
The same information is available at `/api/connections`, with the hub’s Origin
checks. An empty array means no remote connection attempts are currently tracked.

## id and trust

```sh
omochi-ui id
omochi-ui id --hub
omochi-ui trust list
omochi-ui trust forget workstation
```

`id` prints the persistent gateway identity; `--hub` selects the hub identity.
`trust forget` accepts an EndpointId or a unique name. It durably removes trust
and waits for every registered live local process to destroy revoked connections.
Failure to acknowledge remains an error, even though the removal is already
saved. Pair again to establish trust after removal.

## generate catalog

Generate typed bindings from a local A2UI v0.9 catalog for any subset of SDKs.
Create output directories before running the command.

```sh
mkdir -p generated
omochi-ui generate catalog \
  --catalog ./catalog.json \
  --name ProductCatalog \
  --typescript ./generated/product-catalog.ts \
  --python ./generated/product_catalog.py \
  --rust ./generated/product_catalog.rs
```

| Option | Meaning |
| --- | --- |
| `--catalog FILE` | Required catalog JSON input |
| `--name NAME` | Type name; defaults to catalog title or `GeneratedCatalog` |
| `--typescript FILE` | TypeScript output |
| `--python FILE` | Python output |
| `--rust FILE` | Rust output |
| `--check` | Check outputs without writing; fail if missing or stale |

At least one output flag is required. Generation resolves local schemas only;
it does not fetch schemas over the network. Reuse the same invocation with
`--check` in CI.

Component names are used unchanged in generated type names and on the wire. They
must match `[A-Za-z_][A-Za-z0-9_]*`; TypeScript also permits `$` at any position.
Python component names must not start with `__`, because class references would
be name-mangled. Rust rejects `Component`, whose generated `ComponentProps` would
conflict with the SDK trait. Keyword names such as `class` are supported because
generated property types append `Props`. Restrictions apply only to the selected
output languages, and an unsupported name fails before any output is modified.

Local `$ref` siblings follow the [JSON Schema 2020-12 reference rules](https://json-schema.org/draft/2020-12/json-schema-core#section-8.2.3.1):
both the referenced schema and adjacent constraints apply. For example, a reference
to `enum: ["a", "b"]` with a sibling `enum: ["b", "c"]` becomes an intersection,
so TypeScript accepts only `"b"`. Annotation-only siblings such as `description`
do not change the type. Partial `required`, `properties` and `items` constraints
use an object or array type guaranteed by the reference; required fields without
a local property definition use `unknown` in TypeScript and retain their referenced
type through the intersection. Each side resolves relative references from its own
source file. Python and Rust use the `allOf` approximations described below.

Property and array-item schemas also combine adjacent constraints without a `$ref`.
For example, `anyOf` accepting strings or numbers alongside `enum: ["allowed"]`
accepts only `"allowed"` in TypeScript. Object properties and requirements next to
`allOf` remain part of the generated intersection. Python and Rust use the same
approximations as `allOf`; these bindings do not implement full JSON Schema validation.

Enums support JSON scalar values (strings, booleans, numbers and null), including
inside arrays and unions. Empty enums and object or array enum members are rejected
before any output is written. TypeScript uses literal unions; Python uses
`Literal` with `True`, `False` and `None`, widening floating-point members to `float`.
Rust keeps named enums for string properties, and uses `bool`, `serde_json::Number`
or `()` for homogeneous non-string enums and `Value` for mixed enums. Nested string
enums use `String`. These wider types preserve JSON values but do not restrict
values to the declared enum members. Rust string variant names that collide after
sanitization receive numeric suffixes.

Python preserves original property names, using functional `TypedDict` declarations
for keywords such as `class` or keys such as `data-label` that cannot be class fields.
Use dictionaries with these keys; required and optional fields retain their meaning.
TypeScript and Python reserve the component property `key` for the builder's node key. If a
component declares `key` in `properties` or `required`, including through `allOf`
or local references, requesting TypeScript or Python output fails before any output is changed.
Rename that property when targeting either language. Rust supports it because its
builder takes the node key separately from props. Keys inside nested object
properties are supported in all three languages.

Component bindings collect properties and required fields from the component itself
and every nested `allOf` layer, including common mixins. Repeated property names
combine their schemas as `allOf`, so TypeScript requires every layer's constraints
regardless of their order. Identical schemas are deduplicated. Python uses the
approximations below; Rust uses `Value` for multiple distinct layers. Required
fields are combined across layers. Fields inside conditional branches are not
added as unconditional component properties.

Component-level `anyOf` and `oneOf` are unsupported, including in nested `allOf`
layers and resolved local references. Generation reports the component and union
keyword before modifying any output. Unions within property schemas, including
array items and nested object properties, remain supported.

Python has no intersection type operator for property-level `allOf`. If all
remaining layers generate the same Python type, that type is used (including
`dict[str, Any]` for object layers); a single layer keeps its type. Other
combinations and empty `allOf` use `Any`. As with the other bindings, layers with
`if` or `then` are skipped; if none remain, Python uses `Any`. These approximations
also apply inside arrays and unions. They preserve JSON values but do not enforce
every schema constraint. An empty schema `{}` adds no constraints and uses
`unknown` in TypeScript, `Any` in Python and `Value` in Rust. Unsupported layers,
such as boolean schemas, still fail generation before any output is written.

Rust fields use snake case and explicitly preserve the catalog's original JSON
property names, including acronyms and underscores. Rust keywords use raw
identifiers (for example, `r#type`); `self`, `super` and `crate` receive a trailing
underscore. If a property cannot form a Rust identifier or multiple properties
map to the same Rust field, generation fails before writing any output.

## Ports and storage

| Resource | Default |
| --- | --- |
| Vite browser preview | `http://127.0.0.1:5173/` |
| Development bridge | `ws://127.0.0.1:5555/ws` |
| Hub UI / A2UI | `http://127.0.0.1:5555/` / `ws://127.0.0.1:5555/ws` |
| Gateway Unix socket | `$XDG_RUNTIME_DIR/omochi/gateway.sock` (fallback above) |
| Native identity, trust and dashboards | `$XDG_CONFIG_HOME/omochi/rust-v1/` |

`XDG_CONFIG_HOME` defaults to `~/.config`. Global `--config-dir` or
`OMOCHI_CONFIG_DIR` overrides native state. Legacy identity and dashboards are
left untouched. Global `--runtime-dir` or `OMOCHI_RUNTIME_ROOT` selects the SDK
registry; with `up`, this path must end in `/omochi` so child SDKs use the same
registry through `XDG_RUNTIME_DIR`. When `XDG_RUNTIME_DIR` is unset, the default
is `<system temporary directory>/omochi-<uid>/omochi`.
Runtime directories must be private and owned by the current user.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| `dev` starts but there is no page | Start `bun run --filter @omochi-ui/dev-server dev` and open its URL, not port 5555 |
| Preview says disconnected | Start the bridge; with a custom port, use the printed `?ws=` URL |
| Address already in use | `dev` and `hub` share default port 5555; stop one or set `--port 5557` |
| Vite uses a port other than 5173 | Use Vite's printed URL, adding the bridge's `?ws=` parameter if needed |
| Hub has no apps | Run an app with `OMOCHI_SERVICE` set and the same user/runtime directory |
| Remote gateway is unavailable | Check gateway `--remote`, trust records and matching online/offline/relay settings; LAN multicast may be blocked |
| Existing local gateway is incompatible | Stop the old gateway before starting the native hub |
| Native binary is missing | Run `bun run build:native`; for a release build set `OMOCHI_NATIVE_PROFILE=release` |
