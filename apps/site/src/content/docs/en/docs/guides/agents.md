---
title: "MCP & approval surfaces"
description: "MCP & approval surfaces · omochi"
sidebar:
  order: 4
---

## Configure the MCP server

[Install the CLI and utilities](../../start/installation/), then configure an MCP-capable agent:

```json
{
  "mcpServers": {
    "omochi": {
      "command": "omochi-utils",
      "args": ["mcp", "--service-id", "agent", "--name", "Agent"]
    }
  }
}
```

If the agent cannot find the command, set `command` to the installed executable's absolute path.

## Display an agent's view

Start `omochi-ui hub` in another terminal. Ask the agent to inspect your project and publish a `main` Surface with a summary and a useful next command. Add it through **Add view** in the Hub.

Surfaces remain available while the MCP server runs. To respond to buttons, the agent must read events using `read_events` and publish updates. Clicking a button does not wake an idle agent automatically. See the [utilities reference](../../reference/utils/#mcp) for arguments and examples.

## Add tool approval controls

The [agent approval example](https://github.com/omochi-ui/omochi/tree/main/examples/agent-approval) provides an Approve/Reject view for Codex, Claude Code, Cursor, and OpenCode. Follow its README to configure hooks and run the demo. This example runs from a source checkout.

Approval follows each agent's permission controls. Check which hooks are supported and what happens when the service is unavailable before using it.
