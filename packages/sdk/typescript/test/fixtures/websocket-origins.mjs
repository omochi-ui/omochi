import assert from "node:assert/strict";
import { once } from "node:events";
import { get } from "node:http";
import { WebSocket } from "ws";
import { App } from "@omochi-ui/sdk";
import { WebSocketServerTransport } from "@omochi-ui/sdk/node";

const transport = new WebSocketServerTransport({ port: 0 });
const resources = new App({}).resources;
const content = "private media";
const resource = resources.bytes(Buffer.from(content), "video/mp4").split("/").at(-1);
transport.resources = resources;
const snapshot = { version: "v0.9", createSurface: { surfaceId: "main", catalogId: "test" } };
const action = name => ({
  version: "v0.9",
  action: { name, surfaceId: "main", sourceComponentId: "button", timestamp: "2026-08-12T00:00:00.000Z", context: {} },
});
const resourceRequest = { omochi: "resources/1", kind: "request", id: "read", op: "read", resource, offset: 0, length: content.length };
const clients = [];

function httpResource(url, headers) {
  return new Promise((resolve, reject) => {
    get(url, { headers }, response => {
      let body = "";
      response.on("data", chunk => { body += chunk; });
      response.on("end", () => resolve({ status: response.statusCode, body }));
      response.on("error", reject);
    }).on("error", reject);
  });
}

try {
  await transport.ready;
  await transport.send(snapshot);
  const httpUrl = transport.url.replace("ws:", "http:").replace("/ws", `/resources/_/${resource}`);
  const iterator = transport[Symbol.asyncIterator]();
  const rejectedHeaders = [
    { Origin: "https://untrusted.example" },
    { Origin: "null" },
    { Origin: "" },
    { Origin: "not-an-origin" },
    { Origin: "http://localhost.untrusted.example" },
    { Origin: "http://localhost@untrusted.example" },
    { Origin: "http://localhost/path" },
    { Origin: "http://localhost:65536" },
    { Origin: "http://localhost:5555, https://untrusted.example" },
    { Origin: "ws://localhost" },
    { "Sec-Fetch-Site": "cross-site" },
    { Origin: "http://localhost:5555", "Sec-Fetch-Site": "cross-site" },
  ];
  for (const headers of rejectedHeaders) {
    const client = new WebSocket(transport.url, { headers });
    clients.push(client);
    let opened = false;
    const messages = [];
    const statuses = [];
    client.on("open", () => {
      opened = true;
      // A regression would expose snapshots and allow both privileged operations.
      client.send(JSON.stringify(action("untrusted")));
      client.send(JSON.stringify(resourceRequest));
      client.close();
    });
    client.on("message", data => messages.push(JSON.parse(data.toString())));
    client.on("unexpected-response", (_request, response) => {
      statuses.push(response.statusCode);
      response.resume();
      client.terminate();
    });
    client.on("error", () => {});
    await new Promise(resolve => client.once("close", resolve));
    assert.equal(opened, false, JSON.stringify(headers));
    assert.deepEqual(statuses, [403]);
    assert.deepEqual(messages, [], "rejected clients must receive no snapshot or resource response");
    assert.deepEqual(await httpResource(httpUrl, headers), { status: 403, body: "" });
  }

  const allowedHeaders = [
    {}, // Existing non-browser clients do not send Origin.
    { Origin: "http://localhost" },
    { Origin: "https://localhost:5555" },
    { Origin: "http://127.0.0.1:5555", "Sec-Fetch-Site": "same-site" },
    { Origin: "https://127.0.0.1" },
    { Origin: "http://[::1]:5555" },
    { Origin: "https://[::1]" },
  ];
  for (const [index, headers] of allowedHeaders.entries()) {
    const client = new WebSocket(transport.url, { headers });
    clients.push(client);
    const pending = [];
    const waiters = [];
    client.on("message", data => {
      const message = JSON.parse(data.toString());
      const waiter = waiters.shift();
      if (waiter) waiter(message);
      else pending.push(message);
    });
    const next = () => pending.length ? Promise.resolve(pending.shift()) : new Promise(resolve => waiters.push(resolve));
    await once(client, "open");
    assert.deepEqual(await next(), { omochi: "resources/1", kind: "hello" });
    assert.deepEqual(await next(), snapshot);
    client.send(JSON.stringify(resourceRequest));
    assert.deepEqual(await next(), { omochi: "resources/1", kind: "response", id: "read", status: 200, data: Buffer.from(content).toString("base64") });
    const expectedAction = action(`trusted-${index}`);
    client.send(JSON.stringify(expectedAction));
    assert.deepEqual(await iterator.next(), { value: expectedAction, done: false });
    assert.deepEqual(await httpResource(httpUrl, headers), { status: 200, body: content });
    const closed = once(client, "close");
    client.close();
    await closed;
  }
  await transport.close();
  assert.equal((await iterator.next()).done, true, "no action from a rejected connection may be queued");
} finally {
  for (const client of clients) client.terminate();
  await transport.close();
  resources.close();
}
console.log("origins checked");
