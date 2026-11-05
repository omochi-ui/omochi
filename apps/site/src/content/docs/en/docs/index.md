---
title: "Documentation"
description: "Documentation · omochi"
sidebar:
  order: 1
---

Omochi attaches browser UIs to Python, TypeScript, and Rust programs. Declare state, actions, and views in your application language; the shared browser renderer displays A2UI v0.9 updates.

## Choose your path

- **New to Omochi?** [Install the tools](./start/installation/) and [build your first UI](./start/first-ui/).
- **Building an application?** Learn [state, actions, and surfaces](./concepts/runtime/), then choose an [SDK](./guides/sdks/).
- **Connecting existing tools?** Publish [command buttons and logs](./guides/utils/), or connect an [MCP agent](./guides/agents/).
- **Working across apps or machines?** Arrange a [dashboard](./guides/dashboards/), share a saved board, or connect as an owner through [remote connections](./guides/remote/).
- **Using the Flutter app?** Follow the [mobile and desktop guide](https://github.com/omochi-ui/omochi/blob/main/apps/mobile/README.md) for its embedded Hub and external-Hub clients.

## Two parts, one workflow

The SDK runs in your program. The native CLI runs the development bridge, gateway, or hub. Starting an SDK alone emits protocol messages; it does not open a browser.

These guides describe the current repository. Native services and SDKs have separate release versions. Installation instructions distinguish a source checkout from released distributions.

For implementation contracts, development and design history, start with the
[repository documentation map](https://github.com/omochi-ui/omochi/blob/main/docs/README.md)
and [specification index](https://github.com/omochi-ui/omochi/blob/main/spec/README.md).
