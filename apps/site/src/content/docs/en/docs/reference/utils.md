---
title: "Utilities reference"
description: "Utilities reference · omochi"
sidebar:
  order: 3
---

Examples use the [installed](../../start/installation/) `omochi-utils` command. Start `omochi-ui hub` separately and add a tool's Surface through **Add view**.

## Turn a command into a button

```sh
omochi-utils button --service-id build --name "Build" -- bun run build
# In another terminal:
omochi-ui hub
```

Open the hub's printed URL, then use **Add view** to add `Build · main` to a
dashboard. The command runs only when clicked. The surface shows live combined
stdout/stderr, running status and the exit code. Only one execution can run at
a time, including when multiple browsers are connected. After completion or a
startup failure the button can be used again.

```sh
omochi-utils button --name "Tests" --cwd /path/to/project -- bun run test
omochi-utils button --name "Pipeline" -- sh -c 'printf hello | wc -c'
```

Arguments after `--` are passed literally without a shell. Commands inherit the
environment and current directory unless `--cwd` is supplied; stdin is closed.
Output keeps the latest 64 KiB, marks truncation, and resets on each run. It is
displayed as literal code, not interpreted as Markdown. Ctrl+C or SIGTERM stops
child process groups (TERM, then KILL within one second) and retracts the service.

## Watch a command

```sh
omochi-utils watch --interval 2s -- git status --short
omochi-utils watch --diff --on-change --history 10 -- bun run test
omochi-utils watch --timeout 30s --cwd /path/to/project -- command args
```

`watch` runs immediately, waits for the command to finish, then waits for the
interval before running again. Runs never overlap. The surface can pause future
runs, resume immediately, or run once now. `--timeout` stops a hung process group.
`--diff` shows the current output and a line diff from the previous run;
`--on-change` suppresses completed updates whose output and exit result are
unchanged. `--history N` retains 1–100 short execution records without retaining
old full output.

## Control a long-running process

```sh
omochi-utils process --name "Dev server" -- bun run dev
omochi-utils process --restart on-failure --restart-delay 2s -- worker
```

Processes start only when **Start** is clicked. The surface provides **Stop** and
**Restart**, retains the latest 64 KiB of logs across restarts, and records exit
reasons. `--restart` accepts `never` (the default), `on-failure`, or `always`.
Manual stops and CLI shutdown never trigger automatic restart.

## Follow a log file or stdin

```sh
omochi-utils tail --lines 200 ./logs/api.log
journalctl -f | omochi-utils stream --name "System log"
```

`tail` waits if the path does not exist and follows the name across truncation
and log rotation, like `tail -F`. It treats the file as UTF-8 and never writes to
it. `stream` displays stdin as UTF-8. When stdin reaches EOF, it leaves the final
surface visible until SIGINT or SIGTERM instead of exiting immediately.

## Monitor HTTP

```sh
omochi-utils http http://127.0.0.1:3000/health
omochi-utils http --interval 30s --timeout 5s \
  --expect-status 200 --expect-body '"ready":true' https://example.test/health
```

`http` sends non-overlapping GET requests and provides the same pause, resume,
and run-now controls as `watch`. It shows status, latency, final URL, content
type, size, consecutive failures, and health transitions. The default expected
status is 200–299. Text responses are shown up to 64 KiB; JSON media types are
parsed and pretty-printed. Binary response bodies are not displayed. For custom
methods, headers, authentication, or request bodies, use `watch -- curl ...`.

## MCP

Configure a stdio MCP server in your agent. Use `omochi-utils` from its PATH, or the installed executable's absolute path:

```json
{
  "mcpServers": {
    "omochi": {
      "command": "omochi-utils",
      "args": ["mcp", "--name", "Agent"]
    }
  }
}
```

Run the agent, Gateway, and Hub as the same OS user with the same runtime directory for local discovery. See the [MCP guide](../../guides/agents/) for setup.

| Tool | Arguments | Result |
| --- | --- | --- |
| `publish_messages` | `messages`: nonempty A2UI v0.9 message array | `accepted`, `serviceId`, `surfaceIds` |
| `list_surfaces` | none | `serviceId`, `surfaceIds` |
| `read_events` | `after` (default 0), `timeoutMs` (default 0, max 30000) | `events`, `nextCursor`, `dropped`, `closed` |

`publish_messages` accepts standard `createSurface`, `updateComponents`,
`updateDataModel`, and `deleteSurface` messages. It validates the entire batch
against the current state before sending anything. Errors report the failing
array index, leaving the published state intact. Concurrent calls are serialized.
Use IDs local to each Surface.
Validation checks wire shape and surface lifecycle; catalog-specific component
validation is still performed by the renderer.

For example, pass this to `publish_messages` to publish an interactive form:

```json
{
  "messages": [
    {
      "version": "v0.9",
      "createSurface": {
        "surfaceId": "main",
        "catalogId": "https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json",
        "sendDataModel": true
      }
    },
    {
      "version": "v0.9",
      "updateComponents": {
        "surfaceId": "main",
        "components": [
          { "id": "root", "component": "Column", "children": ["title", "request", "submit"] },
          { "id": "title", "component": "Text", "text": "What should we build?" },
          { "id": "request", "component": "TextField", "label": "Request", "value": { "path": "/request" } },
          { "id": "submit", "component": "Button", "child": "label", "action": { "event": { "name": "submit", "context": { "request": { "path": "/request" } } } } },
          { "id": "label", "component": "Text", "text": "Send" }
        ]
      }
    },
    { "version": "v0.9", "updateDataModel": { "surfaceId": "main", "path": "/", "value": { "request": "" } } }
  ]
}
```

Call `read_events` with `{ "after": 0, "timeoutMs": 30000 }`. Each returned event
contains `sequence`, `receivedAt` (ISO timestamp), and the original A2UI client
`message`, including `action.context` and any client data model metadata. The
example's submitted text is in `message.action.context.request`. Respond by
publishing another message batch:

```json
{
  "messages": [{
    "version": "v0.9",
    "updateComponents": {
      "surfaceId": "main",
      "components": [{ "id": "title", "component": "Text", "text": "Working…" }]
    }
  }]
}
```

Use the returned `nextCursor` as `after` for the next read. Reads do not consume
events, so retrying a cursor is safe. The latest 1,000 events are retained;
`dropped` reports how many events since the requested cursor have expired. A
timeout returns an empty array. Pending reads support MCP cancellation and are
woken on shutdown. Button clicks do not automatically wake an idle agent: it
must call `read_events`. These tools do not execute shell commands or callbacks.

To remove a surface, publish
`{"version":"v0.9","deleteSurface":{"surfaceId":"main"}}`.
There is no persistence: stdin EOF, SIGINT or SIGTERM closes the MCP server and
retracts its service. A gateway connecting later receives current surface state.

## Shared options and lifetime

All subcommands accept `--service-id ID` and `--name NAME`. IDs default to a
unique `utils-<uuid>`; use an explicit stable ID if saving a dashboard layout.
Starting a second process with a live ID fails without replacing the first.
Durations accept `ms`, `s`, and `m` suffixes. Each process has its own surfaces
and event buffer. Service registration does not start the gateway or
automatically place panels on a dashboard.

Use `omochi-utils --help` or a subcommand's `--help` for usage. Runtime directories must be private and owned by the current user; see [configuration](../configuration/).
