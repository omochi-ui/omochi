# Agent approvals

Keep using Codex, Claude Code, Cursor, or OpenCode in their own UI and review
their tool calls in one Omochi panel. Every call that reaches the configured
pre-execution hook waits for **Approve** or **Reject**. An optional rejection
note is returned to the agent. Approval does not change the tool's arguments.

## Run the example

From the repository root, with Bun 1.4.3+ on Linux or macOS:

```sh
bun install --frozen-lockfile
bun run build
bun run omochi-ui up --open examples/agent-approval/omochi.toml
```

The command starts the approval service and a loopback-only hub, and opens its
temporary dashboard. Stop them with Ctrl+C. If port 5555 is occupied, add
`--port 5557` to the `up` command. For an existing hub, start just the service
with `bun run examples/agent-approval/app.ts` and select its `main` view.

In another terminal, send four simulated calls:

```sh
bun run examples/agent-approval/demo.ts
```

The demo only displays requests; it **never executes** the displayed commands.
Approve one and reject another with a note. Each decision prints in the demo's
terminal. Requests can be handled in any order, including across sessions.

## Connect an agent

Replace `/absolute/path/to/omochi` in the samples with this checkout's absolute
path. Ensure `bun` is on the agent's PATH, or use its absolute executable path.
Merge these entries into existing settings; do not replace unrelated hooks.
This example does not install or change agent settings automatically.

| Agent | Sample | Where it goes |
| --- | --- | --- |
| Codex | [codex.json](config/codex.json) | `.codex/hooks.json` or `~/.codex/hooks.json` |
| Claude Code | [claude-code.json](config/claude-code.json) | `hooks` in `.claude/settings.json` or `~/.claude/settings.json` |
| Cursor | [cursor.json](config/cursor.json) | `.cursor/hooks.json` or `~/.cursor/hooks.json` |
| OpenCode | [opencode.js](config/opencode.js) | `.opencode/plugins/omochi-approval.js` or `~/.config/opencode/plugins/omochi-approval.js` |

Codex requires reviewing and trusting non-managed hooks through `/hooks`.
Review Claude Code's hooks with `/hooks` and restart the session if prompted.
Cursor reloads its hook configuration. Restart OpenCode after adding its loader.
Keep the checkout available: the loader imports the plugin from it, and the
plugin resolves the example's local imports there.

The JSON samples match all tools. Narrow `matcher` to the agent's tool names
to review fewer calls: for example, `Bash|Edit|Write` for Claude Code,
`Bash|apply_patch` for Codex, or `Shell|Write` for Cursor. For OpenCode, wrap the
plugin hook with a tool-name filter in your loader:

```js
import approval from "/absolute/path/to/omochi/examples/agent-approval/opencode.ts";

export default async function plugin(context) {
  const hooks = await approval(context);
  return {
    "tool.execute.before": async (input, output) => {
      if (["bash", "edit", "write"].includes(input.tool)) {
        await hooks["tool.execute.before"](input, output);
      }
    },
  };
}
```

Use this wrapper **instead of** the unfiltered loader to avoid duplicate hooks.

## Decisions and availability

- Approve returns the agent's supported allow result; OpenCode continues its
  hook. Native restrictions still apply according to that agent's semantics.
  Claude Code's hook approval can satisfy its ordinary permission prompt;
  Codex may still ask for sandbox escalation. This example does not change
  permission modes, sandbox settings, or allowlists.
- Reject returns the note, or a default refusal when it is blank. OpenCode
  receives an error with that message. Notes are never sent on approval.
- When the service is unavailable, a connection fails, or no decision arrives
  within **five minutes**, the hook abstains and the agent uses its normal
  permissions. This may let the tool run without a human decision. Command
  hooks exit 0 with empty stdout; OpenCode returns normally. Keep Cursor's
  `failClosed: false` for this behavior. The enclosing hook deadline is
  **360 seconds**, longer than the client's deadline.
- Closing the browser leaves requests pending until their deadline. Reopening
  it restores the current queue. Stopping the service releases all waiters.
  Interrupting a hook removes its pending request. Late clicks and competing
  browser tabs cannot decide a request twice.
- The latest 100 outcomes are kept in memory, including abstentions. Restarting
  clears history. Tool arguments and notes are not written to a history file.

Calls that an agent does not expose to these hooks cannot be reviewed here.
For example, Codex documents exceptions for some tool execution paths. This is
an optional review UI, not an enforcement boundary. Claude Code's interactive
tools such as `AskUserQuestion` still need their own native answers; this panel
does not provide a question-answer form or rewrite their inputs.

The service publishes `main` over Omochi's existing A2UI Unix transport.
Hook requests use a separate `approval.sock` in the same private service
directory, normally `$XDG_RUNTIME_DIR/omochi/agent-approval/`. Both processes
must run as the same OS user and use the same runtime directory. With no
`XDG_RUNTIME_DIR`, the SDK's per-user temporary-directory fallback is used.
For a custom service ID or runtime location, set `OMOCHI_APPROVAL_SOCKET` to
the full socket path in the **agent's** environment. The service prints its
path on startup. Cloud agents and native Windows are outside this example.

## Implementation and verification

`queue.ts` owns request lifetimes and one-time decisions; `server.ts` and
`client.ts` exchange one JSONL request/response per connection. `ui.ts` declares
the existing SDK components. `adapters.ts`, `hook.ts`, and `opencode.ts` map
agent-specific hooks onto that shared queue. There is no SDK or protocol change.

```sh
bun run test:approval
```

The suite typechecks the example and tests normalization, exact agent outputs,
parallel requests, rejection notes, stale decisions, bounded history, timeout,
disconnect, shutdown, malformed frames, OpenCode's hook, and command-hook
subprocesses. It also drives A2UI action messages through the real Unix
transport, starts the documented composition, and resolves all four demo
requests through the hub's WebSocket. `bun run test` includes this suite after
the workspace tests.

Agent wire formats follow these official references, checked on 2026-10-25:
[Codex hooks](https://learn.chatgpt.com/docs/hooks),
[Claude Code hooks](https://code.claude.com/docs/en/hooks#pretooluse),
[Cursor hooks](https://cursor.com/docs/hooks), and
[OpenCode plugins](https://opencode.ai/docs/plugins/#env-protection).
Tests exercise the adapters and local transports without a model account.
The 17 example tests and the existing workspace tests pass on Bun 1.4.3.
Browser appearance and screenshots were not verified because no browser was
available in the implementation session. Live model-driven sessions in the
four agents have not been verified; test the hook configuration in your
installed version before relying on it.

ACP is not required here. Its standard
[`session/request_permission`](https://agentclientprotocol.com/protocol/v1/tool-calls#requesting-permission)
could feed a future adapter, but it covers agent-issued permission requests
rather than every tool call and has no standard free-text rejection-note field.
