---
title: "Troubleshooting"
description: "Troubleshooting · omochi"
sidebar:
  order: 2
---

## The app prints JSON instead of opening a browser

This is expected when `app.start()` uses stdio. Run it through `omochi-ui dev -- COMMAND`, start the separate Vite preview, and open port 5173. See [first UI](../../start/first-ui/).

## Port 5555 is already occupied

`dev` and `hub` share that default. Stop the existing process or choose `--port`. When changing the bridge port, update the preview connection to match.

## A service does not appear

Confirm the application is running in discoverable mode (`OMOCHI_SERVICE` for bundled examples), under the same user and runtime directory as the gateway. Check ownership and permissions without relaxing mode 0700. The hub does not start arbitrary SDK apps for you.

## Input seems stale after an update

Renderer-local drafts survive rerenders. Use the Local reset API to intentionally overwrite them; changing application state alone does not reset input.

## An action hangs during an update

Do not await `runtime.update()` inside an active action. Use the handler's context update. Actions remain serial even when publishing progress.

## Remote connection fails

Check `status` and `trust list`, confirm enrollment and peer role, and issue a new invitation if the previous one expired. Invitations are single-use and last 180 seconds. `--offline` disables public lookup/relays. Old trust must be replaced by re-pairing.

## Media fails to load

Keep the resource registered throughout playback. Check size limits and MIME type. No codec conversion is performed. Custom transports must route resource responses back to the originating connection.
