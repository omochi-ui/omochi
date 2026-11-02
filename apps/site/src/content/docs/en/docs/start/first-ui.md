---
title: "Your first UI"
description: "Your first UI · omochi"
sidebar:
  order: 2
---

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
