---
title: "Your first UI"
description: "Your first UI · omochi"
sidebar:
  order: 2
---

## Open the hub

In terminal 1, run from any directory. You need Node.js 22+ on Linux or macOS; see [installation](../installation/) for other options.

```sh
npx @omochi-ui/cli hub
```

Open **http://127.0.0.1:5555/**. The hub automatically starts a local gateway, so it is ready to discover apps.

## Add your first surface

In terminal 2, start a utility that displays the current time:

```sh
npx @omochi-ui/utils watch --interval 2s -- date
```

Create a dashboard in the browser, choose **Add view**, select the utility's surface, and save. The time updates every two seconds. This demonstrates the complete path with just the hub and a utility. Stop each terminal with Ctrl+C when finished.

## Connect your SDK app

Start your app instead of the utility and add its surface through **Add view**. Local discovery requires `UnixSocketServerTransport`. Plain `app.start()` defaults to stdin/stdout and does not appear in the hub automatically. See the [SDK transport guide](../../guides/sdks/#transports).

The remaining steps use a source checkout to develop Omochi or try its bundled examples. Stop the hub above before following them.

## Open the bundled workspace

After [installation](../installation/), run from the checkout root:

```sh
bun run quickstart
```

This builds the workspace and opens a temporary dashboard. Start Vite from the **Preview server** panel; the HTTP health panel should recover. Choose **Save as dashboard** to keep the layout. Ctrl+C stops the hub and the processes launched by the composition.

## Preview a single program

Use two terminals at the repository root. In terminal 1:

```sh
bun run --filter @omochi-ui/dev-server dev
```

In terminal 2:

```sh
bun run omochi-ui dev -- uv run --project examples/python examples/python/app.py
```

Open **http://127.0.0.1:5173/**. Change the temperature and press **Run**. The browser resolves the draft value, the Python action updates application state, and the result appears in the surface.

The preview frontend and the bridge are separate servers. The bridge uses port 5555 by default; the frontend uses 5173. Stop the quickstart first to avoid a conflicting hub on port 5555.

## Try another language

Keep terminal 1 running. Replace the command in terminal 2 with:

```sh
# TypeScript
bun run omochi-ui dev -- bun run --cwd examples/typescript start
# Rust (run one application at a time)
bun run omochi-ui dev -- cargo run --manifest-path examples/rust/Cargo.toml
```

Next, read [the runtime concepts](../../concepts/runtime/) and the [SDK guide](../../guides/sdks/).
