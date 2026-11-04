// Raw TCP preserves malformed targets that URL-based HTTP clients would reject.
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
const content = "private media";
const resource = app.resources.bytes(Buffer.from(content), "video/mp4").split("/").at(-1);
const transport = new WebSocketServerTransport({ port: 0 });
const running = app.start(transport);
let healthy;

function request(target, upgrade, origin) {
  const address = new URL(transport.url);
  return new Promise((resolve, reject) => {
    const socket = createConnection({ host: address.hostname, port: Number(address.port) });
    let response = "";
    socket.on("data", data => { response += data; });
    socket.once("error", reject);
    socket.once("close", () => resolve(response));
    socket.once("connect", () => socket.write([
      `GET ${target} HTTP/1.1`, `Host: ${address.host}`,
      ...(upgrade ? [
        "Connection: Upgrade", "Upgrade: websocket", "Sec-WebSocket-Version: 13",
        "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==",
      ] : ["Connection: close"]),
      ...(origin ? [`Origin: ${origin}`] : []), "", "",
    ].join("\r\n")));
  });
}

try {
  await transport.ready;
  healthy = new WebSocket(`${transport.url}?client=healthy`);
  const pending = [];
  const waiters = [];
  healthy.on("message", data => {
    const message = JSON.parse(data.toString());
    const waiter = waiters.shift();
    if (waiter) waiter(message);
    else pending.push(message);
  });
  const next = () => pending.length ? Promise.resolve(pending.shift()) : new Promise(resolve => waiters.push(resolve));
  await once(healthy, "open");
  assert.deepEqual(await next(), { omochi: "resources/1", kind: "hello" });
  await Promise.all([next(), next(), next()]);

  for (const target of ['//[', 'http://[', '//localhost:65536', '//%zz']) {
    for (const upgrade of [false, true]) {
      for (const origin of [undefined, "https://untrusted.example"]) {
        assert.match(await request(target, upgrade, origin), /^HTTP\/1\.1 400 /, `${target}, upgrade=${upgrade}`);
      }
    }
  }
  for (const route of ['%', '%GG', '%C3%28']) {
    assert.match(await request(`/resources/${route}/${resource}`, false), /^HTTP\/1\.1 400 /);
  }
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
  healthy.send(JSON.stringify({
    omochi: "resources/1", kind: "request", id: "read", op: "read", resource, offset: 0, length: content.length,
  }));
  assert.deepEqual(await next(), {
    omochi: "resources/1", kind: "response", id: "read", status: 200, data: Buffer.from(content).toString("base64"),
  });
  const resourcePath = `/resources/%5F/${resource}?download=1`;
  assert.match(await request(resourcePath, false, "https://untrusted.example"), /^HTTP\/1\.1 403 /);
  assert.match(await request('/ws', true, "https://untrusted.example"), /^HTTP\/1\.1 403 /);
  const response = await request(resourcePath, false, "http://localhost:1234");
  assert.match(response, /^HTTP\/1\.1 200 /);
  assert.ok(response.endsWith(`\r\n\r\n${content}`));
  const closed = once(healthy, "close");
  await transport.close();
  assert.equal((await closed)[0], 1001);
  await running;
} finally {
  healthy?.terminate();
  await transport.close();
  await running;
  app.resources.close();
}
console.log("request targets isolated");
