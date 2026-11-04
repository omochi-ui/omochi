// Strict rejection handling and natural process exit catch detached reads/listeners.
import assert from "node:assert/strict";
import { once } from "node:events";
import { access, mkdtemp, rm } from "node:fs/promises";
import { createConnection } from "node:net";
import { tmpdir } from "node:os";
import path from "node:path";
import { PassThrough } from "node:stream";
import { setImmediate } from "node:timers/promises";
import { WebSocket } from "ws";
import { App, CallbackTransport, StdioTransport, schema } from "@omochi-ui/sdk";
import { UnixSocketServerTransport, WebSocketServerTransport } from "@omochi-ui/sdk/node";

const mode = process.argv[2];
const app = new App({});
let actions = 0;
app.action("increment", schema.literal(null), () => { actions++; });
const action = { type: "invoke", action: "increment", input: null };
let runtime;
if (mode === "action") app.action("stop", schema.literal(null), async () => { await runtime.shutdown(); });
runtime = app.build();
runtime.resources.bytes(new Uint8Array([1]), "image/png");

if (mode === "custom" || mode === "late-rejection" || mode === "callback") {
  const reading = Promise.withResolvers();
  const pending = Promise.withResolvers();
  let returned = 0;
  let reads = 0;
  let closed = false;
  const source = {
    [Symbol.asyncIterator]() { return this; },
    next() { reads++; reading.resolve(); return pending.promise; },
    return() { returned++; return Promise.reject(new Error("cleanup rejected")); },
  };
  const sent = [];
  const transport = mode === "callback"
    ? new CallbackTransport(async message => { sent.push(message); }, () => source)
    : { capabilities: new Set(), send: async () => {}, close() { closed = true; }, [Symbol.asyncIterator]() { return source; } };
  const started = runtime.start(transport);
  await reading.promise;
  await runtime.shutdown();
  await runtime.waitClosed();
  await started;
  assert.equal(returned, 1);
  if (mode === "late-rejection") pending.reject(new Error("late read failure"));
  else pending.resolve({ done: false, value: mode === "callback"
    ? { omochi: "resources/1", kind: "request", id: "late", op: "stat", resource: "0".repeat(32) }
    : action });
  await setImmediate();
  assert.equal(actions, 0);
  assert.equal(reads, 1);
  assert.equal(closed, false, "caller-owned close is not a runtime hook");
  assert.equal(sent.length, mode === "callback" ? 1 : 0);
} else if (mode === "action") {
  await runtime.start(new CallbackTransport(async () => {}, async function* () {
    yield { type: "invoke", action: "stop", input: null };
    yield action;
  }));
  assert.equal(actions, 0, "queued actions are discarded on shutdown");
} else if (mode === "stdio") {
  const input = new PassThrough();
  Object.defineProperty(process, "stdin", { value: input });
  const transport = new StdioTransport();
  transport.send = async () => {};
  const started = runtime.start(transport);
  await runtime.ready();
  await setImmediate();
  assert.ok(input.listenerCount("data") > 0);
  await runtime.shutdown();
  await started;
  assert.equal(input.listenerCount("data"), 0);
  assert.equal(input.destroyed, false);
  input.destroy();
} else if (mode === "websocket" || mode === "unix") {
  const root = mode === "unix" ? await mkdtemp(path.join(tmpdir(), "omochi-shutdown-")) : null;
  const transport = root
    ? new UnixSocketServerTransport({ serviceId: "app", runtimeDir: root })
    : new WebSocketServerTransport({ port: 0 });
  try {
    const started = runtime.start(transport);
    await runtime.ready();
    await transport.ready;
    const socketPath = transport.socketPath;
    const client = root ? createConnection(socketPath) : new WebSocket(transport.url);
    if (root) client.resume();
    await once(client, root ? "connect" : "open");
    const disconnected = once(client, "close");
    await runtime.shutdown();
    await runtime.waitClosed();
    await started;
    await disconnected;
    if (root) await assert.rejects(access(path.join(root, "app")), { code: "ENOENT" });
    const url = root ? null : new URL(transport.url);
    const probe = root ? createConnection(socketPath) : createConnection({ host: url.hostname, port: Number(url.port) });
    try { await assert.rejects(once(probe, "connect"), { code: root ? "ENOENT" : "ECONNREFUSED" }); }
    finally { probe.destroy(); }
  } finally {
    await transport.close();
    if (root) await rm(root, { recursive: true, force: true });
  }
} else if (mode === "task-finally") {
  let cleaned = false;
  runtime.spawn(async signal => {
    try { await new Promise(resolve => signal.addEventListener("abort", resolve, { once: true })); }
    finally { await runtime.shutdown(); cleaned = true; }
  });
  await runtime.shutdown();
  await runtime.waitClosed();
  assert.equal(cleaned, true);
} else if (mode === "cleanup-error" || mode === "handler-error") {
  const primary = new Error("receive failed");
  const cleanup = new Error("stop failed");
  const reading = Promise.withResolvers();
  let cleaned = false;
  runtime.spawn(async signal => {
    await new Promise(resolve => signal.addEventListener("abort", resolve, { once: true }));
    cleaned = true;
  });
  const started = runtime.start({
    capabilities: new Set(), send: async () => {},
    async stopReceiving() { throw cleanup; },
    async *[Symbol.asyncIterator]() {
      reading.resolve();
      if (mode === "handler-error") throw primary;
      await new Promise(() => {});
    },
  });
  const rejected = assert.rejects(started, error => error === (mode === "handler-error" ? primary : cleanup));
  await reading.promise;
  if (mode === "cleanup-error") await assert.rejects(runtime.shutdown(), error => error === cleanup);
  await rejected;
  await runtime.waitClosed();
  assert.equal(cleaned, true);
}
assert.throws(() => runtime.resources.bytes(new Uint8Array(), "image/png"), /resource registry closed/);
await runtime.shutdown();
console.log("closed");
