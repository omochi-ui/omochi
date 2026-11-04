// Run in a subprocess so an unhandled server error or leaked socket fails the test.
import assert from "node:assert/strict";
import { once } from "node:events";
import { createConnection } from "node:net";
import { WebSocket } from "ws";
import { App, schema } from "@omochi-ui/sdk";
import { WebSocketServerTransport } from "@omochi-ui/sdk/node";

const state = { count: 0 };
const app = new App(state);
const increment = app.action("increment", schema.literal(null), async (_input, context) => {
  await context.update(current => { current.count += 1; });
});
app.surface("main", (ui, current) => {
  ui.text(`Count: ${current.count}`, { key: "count" });
  ui.button("Increment", increment.call(null), { key: "increment" });
});

const transport = new WebSocketServerTransport({ port: 0 });
const running = app.start(transport);
let healthy;
let malformed;
try {
  await transport.ready;
  healthy = new WebSocket(transport.url);
  const pending = [];
  const waiters = [];
  healthy.on("message", data => {
    const message = JSON.parse(data.toString());
    if (message.omochi === "resources/1") return;
    const waiter = waiters.shift();
    if (waiter) waiter(message);
    else pending.push(message);
  });
  const next = () => pending.length
    ? Promise.resolve(pending.shift())
    : new Promise(resolve => waiters.push(resolve));
  await once(healthy, "open");
  await Promise.all([next(), next(), next()]);

  // A raw peer works in Node and Bun and deliberately never answers a close frame.
  const address = new URL(transport.url);
  malformed = createConnection({ host: address.hostname, port: Number(address.port) });
  await once(malformed, "connect");
  const upgraded = new Promise((resolve, reject) => {
    let response = "";
    malformed.on("data", data => {
      response += data.toString();
      if (response.includes("\r\n\r\n")) resolve(response);
    });
    malformed.once("error", reject);
  });
  malformed.write([
    `GET ${address.pathname} HTTP/1.1`,
    `Host: ${address.host}`,
    "Upgrade: websocket",
    "Connection: Upgrade",
    "Sec-WebSocket-Version: 13",
    "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==",
    "", "",
  ].join("\r\n"));
  assert.match(await upgraded, /^HTTP\/1\.1 101 /);
  const malformedClosed = once(malformed, "close");
  // FIN + RSV1 + text, masked empty payload: RSV1 is forbidden without compression.
  malformed.write(Buffer.from([0xc1, 0x80, 0, 0, 0, 0]));
  await malformedClosed;

  assert.equal(healthy.readyState, WebSocket.OPEN);
  healthy.send(JSON.stringify({
    version: "v0.9",
    action: {
      name: "increment", surfaceId: "main", sourceComponentId: "increment",
      timestamp: "2026-08-12T00:00:00.000Z", context: {},
    },
  }));
  const update = await next();
  assert.equal(state.count, 1);
  assert.equal(update.updateComponents.components.find(component => component.id === "count").text, "Count: 1");

  const healthyClosed = once(healthy, "close");
  await transport.close();
  assert.equal((await healthyClosed)[0], 1001);
  await running;
} finally {
  malformed?.destroy();
  healthy?.terminate();
  await transport.close();
  await running;
}
console.log("protocol error isolated");
