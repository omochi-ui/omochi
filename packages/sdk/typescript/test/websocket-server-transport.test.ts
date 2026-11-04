import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createServer } from "node:net";
import { fileURLToPath } from "node:url";
import { test } from "@omochi-ui/core/testing";

import { schema, App } from "../src/index.js";
import { WebSocketServerTransport } from "../src/node/index.js";
import type { JsonObject } from "../src/index.js";

for (const runtime of ["bun", "node"]) {
  test(`isolates malformed WebSocket frames and shuts down in ${runtime}`, () => {
    const result = spawnSync(runtime, [
      "--unhandled-rejections=strict",
      fileURLToPath(new URL("./fixtures/websocket-protocol-errors.mjs", import.meta.url)),
    ], { encoding: "utf8", timeout: 10_000 });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), "protocol error isolated");
  });

  test(`continues after valid client errors and rejects invalid reports in ${runtime}`, () => {
    const result = spawnSync(runtime, [
      "--unhandled-rejections=strict",
      fileURLToPath(new URL("./fixtures/websocket-client-errors.mjs", import.meta.url)),
    ], { encoding: "utf8", timeout: 10_000 });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), "client errors handled");
  });

  for (const phase of ["starting", "ready", "bind-failure", "bind-failure-settled"]) {
    test(`closes a ${phase} WebSocket server and exits in ${runtime}`, () => {
      const result = spawnSync(runtime, [
        "--unhandled-rejections=strict",
        fileURLToPath(new URL("./fixtures/websocket-server-close.mjs", import.meta.url)),
        phase,
      ], { encoding: "utf8", timeout: 10_000 });
      assert.ifError(result.error);
      assert.equal(result.status, 0, result.stderr);
      assert.equal(result.stdout.trim(), "closed");
    });
  }
}

function opened(socket: WebSocket): Promise<void> {
  return new Promise((resolve, reject) => {
    socket.addEventListener("open", () => resolve(), { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
}

function messages(socket: WebSocket): () => Promise<JsonObject> {
  const pending: JsonObject[] = [];
  const waiters: Array<(message: JsonObject) => void> = [];
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(String(event.data)) as JsonObject;
    if (message.omochi === "resources/1") return;
    const waiter = waiters.shift();
    if (waiter) waiter(message);
    else pending.push(message);
  });
  return () => {
    const message = pending.shift();
    return message ? Promise.resolve(message) : new Promise((resolve) => waiters.push(resolve));
  };
}

async function initialSnapshot(next: () => Promise<JsonObject>): Promise<JsonObject[]> {
  return Promise.all([next(), next(), next()]);
}

test("hosts one shared app and snapshots its latest state for late clients", async (t) => {
  const transport = new WebSocketServerTransport({
    host: "127.0.0.1",
    port: 0,
    path: "/a2ui",
    capabilities: ["test-capability"],
  });
  assert.equal(transport.url, null);
  assert.deepEqual([...transport.capabilities], ["test-capability"]);

  const state = { count: 0 };
  const app = new App(state);
  const increment = app.action("increment", schema.literal(null), async (_input, context) => {
    await context.update((current) => { current.count += 1; });
  });
  app.surface("main", (ui, current) => {
    ui.text(`Count: ${current.count}`, { key: "count" });
    ui.button("Increment", increment.call(null), { key: "increment" });
  });
  const running = app.start(transport);
  await transport.ready;
  assert.match(transport.url!, /^ws:\/\/127\.0\.0\.1:\d+\/a2ui$/);
  t.after(async () => {
    await transport.close();
    await running;
  });

  const first = new WebSocket(transport.url!);
  const nextFirst = messages(first);
  await opened(first);
  t.after(() => first.close());
  const firstInitial = await initialSnapshot(nextFirst);
  assert.ok("createSurface" in firstInitial[0]!);
  assert.ok("updateComponents" in firstInitial[1]!);
  assert.ok("updateDataModel" in firstInitial[2]!);

  const second = new WebSocket(transport.url!);
  const nextSecond = messages(second);
  await opened(second);
  t.after(() => second.close());
  await initialSnapshot(nextSecond);

  first.send(JSON.stringify({
    version: "v0.9",
    action: {
      name: "increment",
      surfaceId: "main",
      sourceComponentId: "increment",
      timestamp: "2026-08-12T00:00:00.000Z",
      context: {},
    },
    metadata: {
      a2uiClientDataModel: { version: "v0.9", surfaces: { main: { inputs: {} } } },
    },
  }));
  const firstUpdate = await nextFirst();
  const secondUpdate = await nextSecond();
  assert.deepEqual(firstUpdate, secondUpdate);
  assert.equal(state.count, 1);
  const components = (firstUpdate.updateComponents as JsonObject).components as JsonObject[];
  assert.equal(components.find((component) => component.id === "count")?.text, "Count: 1");

  const late = new WebSocket(transport.url!);
  const nextLate = messages(late);
  await opened(late);
  t.after(() => late.close());
  const lateSnapshot = await initialSnapshot(nextLate);
  const lateComponents = (lateSnapshot[1]!.updateComponents as JsonObject).components as JsonObject[];
  assert.equal(lateComponents.find((component) => component.id === "count")?.text, "Count: 1");

  await transport.close();
  await transport.close();
  await running;
});

test("rejects invalid input without disconnecting other clients", async (t) => {
  const transport = new WebSocketServerTransport({ port: 0 });
  const app = new App({});
  app.surface("main", (ui) => ui.text("Ready"));
  const running = app.start(transport);
  await transport.ready;
  t.after(async () => {
    await transport.close();
    await running;
  });

  const healthy = new WebSocket(transport.url!);
  await opened(healthy);
  const invalidJson = new WebSocket(transport.url!);
  await opened(invalidJson);
  invalidJson.send("not json");
  assert.equal(await new Promise<number>((resolve) => invalidJson.addEventListener("close", event => resolve(event.code), { once: true })), 1007);
  assert.equal(healthy.readyState, WebSocket.OPEN);

  const invalidMessage = new WebSocket(transport.url!);
  await opened(invalidMessage);
  invalidMessage.send(JSON.stringify({ version: "v0.9", error: { message: "not an action" } }));
  assert.equal(await new Promise<number>((resolve) => invalidMessage.addEventListener("close", event => resolve(event.code), { once: true })), 1007);
  assert.equal(healthy.readyState, WebSocket.OPEN);

  const binary = new WebSocket(transport.url!);
  await opened(binary);
  binary.send(Buffer.from("{}"));
  assert.equal(await new Promise<number>((resolve) => binary.addEventListener("close", event => resolve(event.code), { once: true })), 1003);
  assert.equal(healthy.readyState, WebSocket.OPEN);
  healthy.close();
});

test("reports bind errors through ready and app.start", async () => {
  const occupied = createServer();
  await new Promise<void>((resolve) => occupied.listen(0, "127.0.0.1", resolve));
  const address = occupied.address();
  assert.ok(address && typeof address !== "string");
  const transport = new WebSocketServerTransport({ host: "127.0.0.1", port: address.port });
  const app = new App({});
  app.surface("main", (ui) => ui.text("Ready"));
  await assert.rejects(transport.ready, { code: "EADDRINUSE" });
  await assert.rejects(app.start(transport), { code: "EADDRINUSE" });
  await transport.close();
  await new Promise<void>((resolve, reject) => occupied.close((error) => error ? reject(error) : resolve()));
});

test("only upgrades the configured path and closes active clients on shutdown", async t => {
  const transport = new WebSocketServerTransport({ port: 0, path: "/a2ui" });
  t.after(() => transport.close());
  await transport.ready;
  const wrongPath = new WebSocket(transport.url!.replace('/a2ui', '/other'));
  t.after(() => wrongPath.close());
  await assert.rejects(opened(wrongPath));
  assert.equal((await fetch(transport.url!.replace('ws:', 'http:'))).status, 404);
  const client = new WebSocket(transport.url!);
  t.after(() => client.close());
  await opened(client);
  const closed = new Promise<number>(resolve => client.addEventListener('close', event => resolve(event.code), { once: true }));
  await transport.close();
  assert.equal(await closed, 1001);
});
