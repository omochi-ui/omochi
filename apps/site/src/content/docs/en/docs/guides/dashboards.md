---
title: "Compose a dashboard"
description: "Compose a dashboard · omochi"
sidebar:
  order: 2
---

## Arrange views

Start the Hub and a utility using [your first UI](../../start/first-ui/), or publish your own app using the [SDK guide](../sdks/).

Create a dashboard in the Hub and select a Surface through **Add view**. Adjust its title, position, and size, then save. Multiple Surfaces from one app can be placed independently.

The Hub discovers running apps; keep their processes running while using their views. A stopped app becomes unavailable and returns when restarted with the same service ID. Saving a dashboard preserves its layout, not the application's running state.

## Start multiple processes together

Declare services and a layout in your project's `omochi.toml`, then start them together:

```sh
omochi-ui up --open
```

The initial layout is temporary. Choose **Save as dashboard** to keep an editable copy. Ctrl+C stops the processes started by this command. See the [CLI reference](../../reference/cli/#up) for a configuration example.

## Share a saved dashboard

```sh
omochi-ui hub --remote
```

Save a board and open **Share dashboard**. App interaction and layout editing can be allowed independently; both are disabled by default. Send the resulting link to recipients. Anyone holding it, including forwarded copies, receives the same permissions.

Keep the Hub and source apps running while recipients use the board. Shares follow the saved board, including views added or removed by the owner. Recipients can access the complete state and published images of the permitted Surfaces. Deleting the board ends its shares.

Layout editing permits changes to names, titles, positions, and sizes. Adding or removing views, changing sources, deleting boards, and managing shares require owner access. Allowing app interaction may change the source app's data.

To change permissions, revoke and reissue the link. **Revoke access…** ends access for everyone holding that link. Removing a local connection record does not revoke permission, and delivered data or completed actions cannot be recalled. See [troubleshooting](../../understand/troubleshooting/#revocation-reports-an-error) if revocation reports an error.

Use an [owner connection](../remote/#connect-from-a-hosted-browser) for full management. The [mobile and desktop app](../mobile/) can also open sharing links.

## Switch and save connections

The sidebar's **Connections** lists owner connections and shared dashboards.
Select a connection, then choose one of its dashboards below. Switching back
restores your previous dashboard. Shared access stays separate from owner access;
choose an owner connection in the list to manage the Hub.

Open a connection's **…** control to rename it, inspect its IDs, open a shared link in the app, or remove its
saved entry. **Save this shared connection** keeps the link and its access secret
in this browser so it can be opened after closing the tab. Without saving,
opened shares remain available only in the current tab. Saving does not increase
permissions, and removing a saved connection does not revoke Hub access.

On narrow screens, **Switch connection** opens the connection list. Choose **Disconnect
from Hub** in the current connection's **…** control to make the browser identity available to another tab. Close or finish
an owner invitation before disconnecting. Export, Board view and Delete are in
**More actions**; deleting a dashboard still requires confirmation.
When disconnected, choose a connection from the centered list. **Connection details** below it contains the browser ID and its reset action.
