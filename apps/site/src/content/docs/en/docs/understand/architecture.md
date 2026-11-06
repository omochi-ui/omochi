---
title: "Architecture"
description: "Architecture · omochi"
sidebar:
  order: 1
---

Omochi brings views published by your apps into a Hub, where you can use them from a browser or the Omochi app.

```text
App → Gateway → Hub → Browser / Omochi app
```

## Roles

| Part | Role |
| --- | --- |
| SDK app | Defines state, Actions, and Surfaces and publishes its UI |
| Utilities | Publish commands, logs, and agent results as Surfaces |
| Gateway | Discovers apps running on the same machine and forwards them to Hubs |
| Hub | Combines Surfaces from multiple apps and manages dashboard layouts and sharing |
| Browser / Omochi app | Renders Surfaces and sends input and actions to apps |

On one computer, `omochi-ui hub` also starts the Gateway it needs. Start your apps separately. To use apps on another machine, [register its Gateway using an invitation](../../guides/remote/).

## State and storage

The publishing process owns application state. Each client owns its unfinished form drafts, so multiple people using the same app have independent inputs. Submitting an action updates the app's shared state.

The Hub saves dashboard layouts. Restarting the Hub or browser does not restore an app's running state.

## Remote use

The screen at `https://app.omochi.fyi/` connects to a running Hub. It becomes unavailable when that machine stops. Use an owner connection for full management or a sharing link for one board.

Gateway registration is separate from Hub owner and share permissions. See [Connect machines](../../guides/remote/) for enrollment and revocation, and the [dashboard guide](../../guides/dashboards/) for sharing scope.

The [Omochi app](../../guides/mobile/) includes a Hub. It can connect directly to Gateways or act as an owner/shared client of an external Hub.
