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

## Share a saved dashboard

Start `omochi-ui hub --remote`, save a board and open **Share dashboard**.
Create a link with independent app-interaction and layout-editing permissions;
both are disabled by default. Anyone holding a forwarded copy has the same
permissions. No browser enrollment is required. Keep the Hub and source apps
running. Links expose the current board's complete view state and published images.

Editors may change names, titles, positions and sizes. Adding/removing views,
changing sources, deleting boards and managing shares require owner authority.
To change permissions, revoke and reissue the link. Revocation applies to every
holder of that link, including connected clients. Delivered data cannot be recalled.

All v1 public links and identity shares must be reissued after upgrade. Owner
enrollment and Gateway trust are unchanged. See the
[sharing guide](https://github.com/omochi-ui/omochi/blob/main/docs/dashboard-sharing.md).
