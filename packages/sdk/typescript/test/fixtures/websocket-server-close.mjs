// Natural process exit verifies that shutdown leaves no listening server or client.
import assert from "node:assert/strict";
import { once } from "node:events";
import { createConnection, createServer } from "node:net";
import { WebSocket } from "ws";
import { WebSocketServerTransport } from "@omochi-ui/sdk/node";

const phase = process.argv[2];
const bindFailure = phase.startsWith("bind-failure");
let occupied;
let port = 0;
if (bindFailure) {
  occupied = createServer();
  occupied.listen(0, "127.0.0.1");
  await once(occupied, "listening");
  port = occupied.address().port;
}

const transport = new WebSocketServerTransport({ port });
const ready = bindFailure
  ? assert.rejects(transport.ready, { code: "EADDRINUSE" })
  : transport.ready;
let clientClosed;
if (phase === "ready") {
  await ready;
  const client = new WebSocket(transport.url);
  await once(client, "open");
  clientClosed = once(client, "close");
} else if (phase === "bind-failure-settled") {
  await ready;
}

const closing = transport.close();
assert.equal(transport.close(), closing, "concurrent close calls share completion");
await closing;
await ready;
assert.equal(transport.close(), closing, "completed close calls share completion");
await transport.close();
if (clientClosed) assert.equal((await clientClosed)[0], 1001);

if (occupied) {
  await new Promise((resolve, reject) => occupied.close(error => error ? reject(error) : resolve()));
} else {
  const address = new URL(transport.url);
  const probe = createConnection({ host: address.hostname, port: Number(address.port) });
  try {
    await assert.rejects(once(probe, "connect"), { code: "ECONNREFUSED" });
  } finally {
    probe.destroy();
  }
  assert.deepEqual(await transport[Symbol.asyncIterator]().next(), { value: undefined, done: true });
}
console.log("closed");
