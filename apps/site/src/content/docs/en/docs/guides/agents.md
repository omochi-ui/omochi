---
title: "MCP & approval surfaces"
description: "MCP & approval surfaces · omochi"
sidebar:
  order: 4
---

## Give an agent a surface

Build the checkout first. Configure an MCP-capable agent with an absolute checkout path:

```json
{"mcpServers":{"omochi":{"command":"/absolute/path/to/omochi/packages/npm/utils/bin/omochi-utils","args":["mcp","--service-id","quickstart-agent","--name","Agent"]}}}
```

Start `bun run omochi-ui hub`. Ask the agent to inspect your project and publish a `main` surface with a summary and a useful next command. Add the agent view from the hub's view picker. The agent process owns the surface; disconnecting its MCP server retracts it.

## Tool workflow

`omochi-utils mcp` is a stdio MCP server launched by the agent. It publishes ordinary discoverable services, so agent-created surfaces can share a dashboard with SDK apps and command tools. See the [utilities reference](../../reference/utils/) for the tool contract.

## Approval example

The [agent approval integration](https://github.com/omochi-ui/omochi/tree/main/examples/agent-approval) provides an Approve/Reject surface for Codex, Claude Code, Cursor, and OpenCode tool hooks, with optional rejection notes and an agent-free demo. Follow its README for the hook-specific setup. An approval surface does not replace the agent's own permission controls.
