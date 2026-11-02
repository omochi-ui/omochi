---
title: "Turn commands into tools"
description: "Turn commands into tools · omochi"
sidebar:
  order: 3
---

Native utilities publish discoverable surfaces without writing an SDK application. From a built checkout, start a hub in one terminal:

```sh
bun run omochi-ui hub
```

Run one utility in another terminal and add its surface to the dashboard:

```sh
bun run omochi-utils button --name "Build" -- bun run build
bun run omochi-utils watch --diff --interval 2s -- git status --short
bun run omochi-utils http http://127.0.0.1:3000/health
```

A button runs a command when pressed. Watch periodically executes a command and can display changes. HTTP publishes a health view. These processes own their service lifetime; keep them running while using their surfaces.

Use `process` for a start/stop control, `tail` to follow a file, and `stream` to display stdin. See [the complete reference](../../reference/utils/) for arguments and behavior.
