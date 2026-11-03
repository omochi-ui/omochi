# @omochi-ui/core

Shared TypeScript protocol and transport helpers for SDKs and browser clients.
Native hubs, gateways, and bridges are implemented in the Rust workspace.

Nothing here knows about service discovery or about where a backend lives. The package
carries the JSONL reader, the child-process session and fan-out,
the A2UI v0.9 client-frame validator and replayable snapshot, the `/services` and
`/events` HTTP endpoints, and the surface-id prefixing that namespaces one backend's
surfaces inside a merged stream.

`@omochi-ui/core/testing` exports the helpers those components' test suites share.

The legacy `startWsEndpoint` helpers and `@omochi-ui/core/websocket` entry point
have been removed. TypeScript applications can host WebSocket clients with
`WebSocketServerTransport` from `@omochi-ui/sdk/node`, which uses `ws` and supports Node.js and Bun.
