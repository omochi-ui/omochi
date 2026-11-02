---
title: "Compose a dashboard"
description: "Compose a dashboard · omochi"
sidebar:
  order: 2
---

## Publish a discoverable application

From a built checkout:

```sh
OMOCHI_SERVICE=python-example uv run --project examples/python examples/python/app.py
```

In another terminal:

```sh
bun run omochi-ui hub
```

Open **http://127.0.0.1:5555/**, create a dashboard, add a surface, and save. The bundled application selects a discoverable Unix socket when `OMOCHI_SERVICE` is set. The hub starts a local gateway if needed and discovers running apps; it does not launch those apps.

## Compose processes with up

`omochi-ui up --open [FILE]` starts the services and hub declared in a project preset. The quickstart uses `examples/quickstart/omochi.toml`. Its initial layout is temporary until saved as a dashboard. Ctrl+C stops processes owned by `up`.

## Layout and lifetime

A surface is independently placeable. Dashboard configuration persists the layout, not the application's live state. A stopped service becomes unavailable; restart the application to restore its surface. `hub` and `dev` both default to 5555, so stop one or select a different port.

See [CLI options](../../reference/cli/) and [configuration](../../reference/configuration/).
