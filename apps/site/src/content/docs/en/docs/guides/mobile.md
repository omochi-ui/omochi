---
title: "Mobile & desktop app"
description: "Mobile & desktop app · omochi"
sidebar:
  order: 7
---

The Omochi app displays and controls Surfaces on Android, iOS, macOS, Windows, and Linux. See the [app README](https://github.com/omochi-ui/omochi/blob/main/apps/mobile/README.md) for platform builds, downloads, and support status.

## Connect to a Gateway

Run `omochi-ui gateway --remote --pair` on the computer hosting your apps. Open the printed HTTPS invitation page on your device and choose **Open in Omochi app**. Check the peer and expiry in the app, then choose **Add to this device**. You can also paste the invitation in Connections.

Select a service's Surface from Apps. The app uses its embedded Hub, so no separate Hub is required. Keep the registered Gateway and source apps running.

## Connect to an external Hub

Use Hub connections in the toolbar to open or paste an [owner invitation](../remote/#connect-from-a-hosted-browser) or [sharing link](../dashboards/#share-a-saved-dashboard). One connection is displayed at a time; using an external Hub suspends the embedded Hub.

Issue a new owner invitation for the app. An invitation used in a browser cannot be reused. Sharing links permit the app interaction and layout editing allowed by that link.

Open a sharing link temporarily or save it on the device to reconnect with the same permissions later. Deleting the saved connection does not revoke access at the Hub; revoke it on the peer Hub to end access.

## Connection and media limits

HTTPS links do not currently launch the app directly. Paste invitations or use `omochi://` handoff links.

The embedded Hub supports up to two Gateways and eight services. External-Hub connections display published images; remote audio/video are unavailable. Linux Video playback is also unavailable. See the [app README](https://github.com/omochi-ui/omochi/blob/main/apps/mobile/README.md) for support details.
