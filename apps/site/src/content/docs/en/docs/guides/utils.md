---
title: "Turn commands into tools"
description: "Turn commands into tools · omochi"
sidebar:
  order: 3
---

Native utilities publish discoverable surfaces without writing an SDK application. On Linux, macOS or Windows 11 with Node.js 22+, start a hub from any directory:

```sh
npx @omochi-ui/cli hub
```

The hub starts a local gateway automatically. Open **http://127.0.0.1:5555/** , run a utility in another terminal, and add its surface with **Add view**. You can also [install the commands](../../start/installation/) to use `omochi-ui` and `omochi-utils` directly:

```sh
npx @omochi-ui/utils button --name "Build" -- bun run build
npx @omochi-ui/utils watch --diff --interval 2s -- git status --short
npx @omochi-ui/utils http http://127.0.0.1:3000/health
```

A button runs a command when pressed. Watch periodically executes a command and can display changes. HTTP publishes a health view. These processes own their service lifetime; keep them running while using their surfaces.

Use `process` for a start/stop control, `tail` to follow a file, and `stream` to display stdin. See [the complete reference](../../reference/utils/) for arguments and behavior.
