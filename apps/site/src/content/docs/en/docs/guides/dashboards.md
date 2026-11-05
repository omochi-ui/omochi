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

Start the native Hub with `omochi-ui hub --remote`, save the dashboard and open
**Share dashboard**. The hosted client at `https://app.omochi.fyi/` connects to
that Hub over iroh; keep the Hub running.

| Share mode | Access |
| --- | --- |
| **Invite one browser** | One enrolled identity; optional app interaction and layout editing |
| **Anyone with the link** | Reusable readonly bearer link; forwarding it grants access |

Identity invitations expire after three minutes; accepted access lasts until
revoked. Issue a separate invitation per recipient. Layout editors can change
names, titles, positions and sizes. Adding/removing views, changing sources,
deleting the board and managing shares require owner access. Interaction may
change application state.

Shares follow the saved board: owner additions become visible, removals withdraw
views, and deletion ends access. Recipients see the permitted Surfaces' complete
state and published images. Unavailable sources remain unavailable.

Use **Revoke access…** to remove an identity grant or an entire public link.
Revocation waits for live listener teardown; an acknowledgement error remains an
error after durable removal. Forgetting a browser connection does not revoke it
on the Hub.

For full management, use [owner access](../remote/#connect-from-a-hosted-browser).
The [sharing guide](https://github.com/omochi-ui/omochi/blob/main/docs/dashboard-sharing.md)
explains browser identity, storage failures, reconnect and Flutter clients.
