---
title: "Connect machines"
description: "Connect machines · omochi"
sidebar:
  order: 5
---

## Use apps on another machine

[Install the CLI](../../start/installation/) on both machines. On the machine hosting your apps, run:

```sh
omochi-ui gateway --remote --pair --name workstation
```

On the machine running the Hub, import the printed invitation:

```sh
omochi-ui pair --name workstation
# Paste the invitation on stdin, then Ctrl+D.
omochi-ui hub
```

Either invitation link printed by the Gateway can be pasted into `pair`. Invitations last three minutes and register one Hub. Issue a new invitation for each additional Hub. Using stdin keeps the invitation out of shell history.

A registered Hub can access every app, action, and resource exposed by that Gateway. Share invitations privately and keep the services running on both machines while using them. Registered peers reconnect without entering IP addresses.

## Connect from a hosted browser

On the Hub machine, run:

```sh
omochi-ui hub --remote
```

Open the local screen at **http://127.0.0.1:5555/**. In Connections, choose **Register owner browser**, open the link in the target browser, and accept. Invitations last three minutes. Registered owner access continues until revoked at the Hub. Keep the Hub running while using it.

Owners can manage all dashboards, views, sources, app actions, sharing, and Gateway registration. Register and revoke owners from the local screen. Use a [dashboard share](../dashboards/#share-a-saved-dashboard) to give access to just one board.

To add a Gateway from this browser, run `gateway --remote --pair` on the app machine, open its HTTPS link or QR, select your owner Hub, and choose **Add to this hub**. Clients connected through sharing links cannot add Gateways.

The browser saves its connection information for later use. If you clear browser storage, register again using a new owner invitation. See [troubleshooting](../../understand/troubleshooting/#disconnected-during-gateway-registration) if registration is interrupted.

## Connect from the mobile or desktop app

The [Omochi app](../mobile/) can accept a Gateway invitation with **Add to this device**, or open an external Hub's owner/share link. Issue a new owner invitation for the app; an invitation already used in a browser cannot be reused.

## Configure networking

Connections normally locate peers automatically and use a relay when a direct connection is unavailable. Use `--relay URL` for a custom relay or `--offline` for LAN-only use. See [configuration](../../reference/configuration/) for details.

Hosted browser access does not require exposing the Hub's HTTP port. Keep its default `127.0.0.1` binding. `hub --remote` cannot be combined with `--offline`.

## Inspect and revoke access

```sh
omochi-ui status
omochi-ui trust list
omochi-ui trust forget NAME_OR_ENDPOINT_ID
```

Run `trust forget` at the Gateway to revoke a Hub's access to its apps. Revoke Hub owners from the local screen and sharing links from the board's sharing dialog. These permissions are managed separately.

Removing a client's local connection record does not revoke access at the peer. Delivered data and completed actions cannot be recalled. See [troubleshooting](../../understand/troubleshooting/#revocation-reports-an-error) if revocation fails.
