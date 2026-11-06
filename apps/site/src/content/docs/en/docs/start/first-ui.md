---
title: "Your first UI"
description: "Your first UI · omochi"
sidebar:
  order: 2
---

## Open the Hub

On Linux or macOS with Node.js 22+, run this in terminal 1. See [installation](../installation/) for other options.

```sh
npx @omochi-ui/cli hub
```

Open **http://127.0.0.1:5555/**.

## Display the time

In terminal 2, start a utility that displays the current time:

```sh
npx @omochi-ui/utils watch --interval 2s -- date
```

Create a dashboard in the browser, choose **Add view**, select the utility's Surface, and save. The time updates every two seconds. A Surface is a named view published by an app or tool.

Keep both the Hub and utility running while using the view. Stop each terminal with Ctrl+C when finished. The dashboard layout is saved; the utility's running state is not.

## Continue

- Add a UI to your program: [Use the SDKs](../../guides/sdks/)
- Display commands and logs: [Turn commands into tools](../../guides/utils/)
- Arrange or share views: [Compose a dashboard](../../guides/dashboards/)
