---
title: "Troubleshooting"
description: "Troubleshooting · omochi"
sidebar:
  order: 2
---

## The app prints JSON instead of opening a browser

`app.start()` defaults to stdin/stdout. To display it in a Hub, select `LocalServerTransport` as shown in the [SDK guide](../../guides/sdks/). See [Transports](../../reference/transports/) for custom clients and stdio previews.

## Port 5555 is already occupied

`dev` and `hub` share that default. Stop the existing process or choose `--port`. When changing the bridge port, use its printed `?ws=` URL to update the preview target.

## A service does not appear

Check that the app publishes through `LocalServerTransport` under the same OS user and runtime directory as the Gateway. The Hub does not start apps automatically. Check paths and environment variables in [configuration](../../reference/configuration/); keep Unix runtime directories at mode 0700 and Windows ACLs restricted to your user.

## Input seems stale after an update

Drafts survive rerenders. Use the [Local reset API](../../concepts/inputs/#rerenders-and-resets) to overwrite them explicitly.

## An action hangs during an update

Await the handler's context update inside an Action, rather than `runtime.update()`. Actions execute serially.

## Remote connection fails

Check `omochi-ui status` and `omochi-ui trust list`. Confirm that the Gateway runs with `--remote`, the peer is registered, and relay/offline settings match. Issue a new invitation if an unused one expired; invitations last three minutes. Check multicast and firewall settings if direct LAN connections fail.

## Disconnected during Gateway registration

Reconnect to the Hub with the same browser and owner connection that started registration, then inspect its result. If saving is pending, choose **Retry saving registration**; no new invitation is needed. Closing the invitation screen does not cancel registration already started on the Hub.

## Another tab owns the connection

One tab at a time uses the browser identity. Use the connected tab or close it and retry. If you need to reopen a share, use the original link.

## Revocation reports an error

Refresh the screen or run `trust list` to check whether the registration remains. Removal may have been saved while a running process failed to confirm that connections closed. Stop or restart unresponsive Hubs/Gateways, then verify their state. Do not treat the error as a successful revocation.

## Media fails to load

Keep resources registered throughout playback. Check size limits, MIME type, and the client's [media support](../../guides/media/#media-support-by-client). No codec conversion is performed. Custom transports must reply to the originating connection.
