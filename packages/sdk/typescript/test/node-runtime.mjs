// Run with Node against the built public entry points to catch Bun-only APIs.
// From this package after building: node --test test/node-runtime.mjs
import assert from "node:assert/strict";
import { once } from "node:events";
import { test } from "node:test";
import { WebSocket } from "ws";
import { App, schema } from "@omochi-ui/sdk";
import { WebSocketServerTransport } from "@omochi-ui/sdk/node";

test("Node hosts WebSocket clients, handles actions and serves HTTP resources", { timeout: 10_000 }, async () => {
  assert.equal(typeof globalThis.Bun, "undefined");
  const state = { count: 0 };
  const app = new App(state);
  const increment = app.action("increment", schema.literal(null), async (_input, context) => {
    await context.update(current => { current.count += 1; });
  });
  const uri = app.resources.bytes(Buffer.from("0123456789"), "video/mp4");
  app.surface("main", (ui, current) => {
    ui.text(`Count: ${current.count}`, { key: "count" });
    ui.button("Increment", increment.call(null), { key: "increment" });
    ui.video(uri);
  });

  const transport = new WebSocketServerTransport({ port: 0 });
  const running = app.start(transport);
  let client;
  try {
    await transport.ready;
    assert.match(transport.url, /^ws:\/\/127\.0\.0\.1:\d+\/ws$/);
    client = new WebSocket(transport.url);
    const pending = [];
    const waiters = [];
    client.on("message", data => {
      const message = JSON.parse(data.toString());
      if (message.omochi === "resources/1") return;
      const waiter = waiters.shift();
      if (waiter) waiter(message);
      else pending.push(message);
    });
    const next = () => pending.length
      ? Promise.resolve(pending.shift())
      : new Promise(resolve => waiters.push(resolve));
    await once(client, "open");
    const snapshot = await Promise.all([next(), next(), next()]);
    assert.ok(snapshot[0].createSurface);
    assert.ok(snapshot[1].updateComponents);
    assert.ok(snapshot[2].updateDataModel);

    client.send(JSON.stringify({
      version: "v0.9",
      action: {
        name: "increment", surfaceId: "main", sourceComponentId: "increment",
        timestamp: "2026-08-12T00:00:00.000Z", context: {},
      },
      metadata: { a2uiClientDataModel: { version: "v0.9", surfaces: { main: { inputs: {} } } } },
    }));
    const update = await next();
    assert.equal(state.count, 1);
    assert.equal(update.updateComponents.components.find(component => component.id === "count").text, "Count: 1");

    const resource = new URL(`/resources/_/${uri.split("/").at(-1)}`, transport.url.replace("ws:", "http:"));
    const partial = await fetch(resource, { headers: { Range: "bytes=2-4" } });
    assert.equal(partial.status, 206);
    assert.equal(await partial.text(), "234");
    const head = await fetch(resource, { method: "HEAD" });
    assert.equal(head.headers.get("content-length"), "10");
    assert.equal(await head.text(), "");
    assert.equal((await fetch(resource, { headers: { Origin: "https://evil.example" } })).status, 403);

    const closed = once(client, "close");
    await transport.close();
    assert.equal((await closed)[0], 1001);
  } finally {
    client?.terminate();
    await transport.close();
    await running;
  }
});
