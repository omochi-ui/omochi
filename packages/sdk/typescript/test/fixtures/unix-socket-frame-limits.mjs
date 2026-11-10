import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { createConnection } from "node:net";
import { tmpdir } from "node:os";
import path from "node:path";
import { createInterface } from "node:readline";
import { ResourceRegistry } from "@omochi-ui/sdk";
import { LocalServerTransport } from "@omochi-ui/sdk/node";

const limit = 4 * 1024 * 1024;
const root = await mkdtemp(path.join(tmpdir(), "omochi-limits-"));
const transport = new LocalServerTransport({ serviceId: "demo", runtimeDir: root });
const resources = new ResourceRegistry();
transport.resources = resources;
const uri = resources.bytes(new Uint8Array([1, 2, 3]), "image/png");
const clients = [];
const action = (input) => ({
  version: "v0.9",
  action: {
    name: "submit", surfaceId: "main", sourceComponentId: "submit",
    timestamp: "2026-11-04T00:00:00Z", context: { input },
  },
});
const encode = (message) => Buffer.from(JSON.stringify(message));
function sizedAction(size) {
  const available = size - encode(action("")).length;
  return action("あ".repeat(Math.floor(available / 3)) + "x".repeat(available % 3));
}
function deadline(promise, name) {
  let timer;
  return Promise.race([
    promise,
    new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`timed out: ${name}`)), 2000); }),
  ]).finally(() => clearTimeout(timer));
}
async function connect() {
  const socket = createConnection(transport.endpoint.path);
  clients.push(socket);
  socket.on("error", () => {}); // Oversized peers may receive ECONNRESET/EPIPE.
  const closed = new Promise((resolve) => socket.once("close", resolve));
  const lines = createInterface({ input: socket, crlfDelay: Infinity });
  lines.on("error", () => {});
  const messages = lines[Symbol.asyncIterator]();
  assert.equal(JSON.parse((await messages.next()).value).kind, "hello");
  return { socket, closed, messages };
}
const write = (socket, chunk) => new Promise((resolve) => socket.write(chunk, resolve));
try {
  await transport.ready;
  const healthy = await connect();
  const received = transport[Symbol.asyncIterator]();
  const verifyHealthy = async () => {
    const message = action("still alive");
    const next = received.next();
    healthy.socket.write(`${JSON.stringify(message)}\n`);
    assert.deepEqual((await deadline(next, "healthy action")).value, message);
    healthy.socket.write(`${JSON.stringify({ omochi: "resources/1", kind: "request", id: "read", op: "stat", resource: uri.split("/").at(-1) })}\n`);
    assert.equal(JSON.parse((await deadline(healthy.messages.next(), "healthy resource RPC")).value).status, 200);
  };

  const resource = encode({ omochi: "resources/1", kind: "request", id: "too-large", op: "stat", padding: "x".repeat(limit) });
  for (const [name, frame, ending] of [
    ["unterminated whitespace", Buffer.alloc(limit + 1, " "), ""],
    ["terminated whitespace", Buffer.alloc(limit + 1, " "), "\n"],
    ["oversized UTF-8 action", encode(sizedAction(limit + 1)), "\n"],
    ["CR counts toward limit", encode(sizedAction(limit)), "\r\n"],
    ["resource RPC", resource, "\n"],
  ]) {
    const peer = await connect();
    // Force the limit to span multiple writes, keeping the socket open at overflow.
    for (let offset = 0; offset < frame.length; offset += 65537) {
      const error = await write(peer.socket, frame.subarray(offset, offset + 65537));
      if (error) break;
    }
    if (ending && !peer.socket.destroyed) await write(peer.socket, Buffer.from(`${ending}${JSON.stringify(action("must not dispatch"))}\n`));
    await deadline(peer.closed, name);
    await verifyHealthy(); // Also proves the offending peer's tail was not queued.
  }

  for (const [name, message, ending] of [
    ["below limit", sizedAction(limit - 1), "\n"],
    ["exact limit", sizedAction(limit), "\n"],
    ["CRLF at limit", sizedAction(limit - 1), "\r\n"],
    ["EOF at limit", sizedAction(limit), ""],
  ]) {
    const peer = await connect();
    const next = received.next();
    const frame = encode(message);
    for (let offset = 0; offset < frame.length; offset += 65537) {
      assert.ifError(await write(peer.socket, frame.subarray(offset, offset + 65537)));
    }
    if (ending) peer.socket.write(ending);
    else peer.socket.end();
    assert.deepEqual((await deadline(next, name)).value, message);
    peer.socket.destroy();
  }
  await verifyHealthy();
} finally {
  for (const socket of clients) socket.destroy();
  await transport.close();
  resources.close();
  await rm(root, { recursive: true, force: true });
}
console.log("frame limits isolated");
