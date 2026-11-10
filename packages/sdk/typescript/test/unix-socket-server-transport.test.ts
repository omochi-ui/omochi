import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { access, mkdtemp, readFile, rm, mkdir } from "node:fs/promises";
import { createConnection, Socket, type Server } from "node:net";
import { tmpdir } from "node:os";
import path from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { test } from "@omochi-ui/core/testing";

import { schema, App } from "../src/index.js";
import { LocalServerTransport } from "../src/node/index.js";
import type { JsonObject } from "../src/index.js";

for (const runtime of ["node", "bun"]) {
  test(`bounds Unix frame bytes before parsing in ${runtime}`, { timeout: 20_000 }, () => {
    const result = spawnSync(runtime, [
      "--unhandled-rejections=strict",
      fileURLToPath(new URL("./fixtures/unix-socket-frame-limits.mjs", import.meta.url)),
    ], { encoding: "utf8", timeout: 15_000 });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), "frame limits isolated");
  });
  test(`isolates malformed Unix client frames in ${runtime}`, () => {
    const result = spawnSync(runtime, [
      "--unhandled-rejections=strict",
      fileURLToPath(new URL("./fixtures/unix-socket-client-frames.mjs", import.meta.url)),
    ], { encoding: "utf8", timeout: 10_000 });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), "client frames isolated");
  });
  test(`isolates a disconnect during Unix snapshot replay in ${runtime}`, () => {
    const result = spawnSync(runtime, [
      "--unhandled-rejections=strict",
      fileURLToPath(new URL("./fixtures/unix-socket-disconnect.mjs", import.meta.url)),
    ], { encoding: "utf8", timeout: 10_000 });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), "disconnect isolated");
  });
}

async function runtimeDir(t: import("@omochi-ui/core/testing").TestContext): Promise<string> {
  const directory = await mkdtemp(path.join(tmpdir(), "omochi-unix-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return directory;
}

function connectClient(socketPath: string): { socket: Socket; next: () => Promise<JsonObject> } {
  const socket = createConnection(socketPath);
  const pending: JsonObject[] = [];
  const waiters: Array<(message: JsonObject) => void> = [];
  createInterface({ input: socket, crlfDelay: Infinity }).on("line", (line) => {
    if (!line.trim()) return;
    const message = JSON.parse(line) as JsonObject;
    if (message.omochi === "resources/1") return;
    const waiter = waiters.shift();
    if (waiter) waiter(message);
    else pending.push(message);
  });
  return {
    socket,
    next: () => {
      const message = pending.shift();
      return message ? Promise.resolve(message) : new Promise((resolve) => waiters.push(resolve));
    },
  };
}

function counterApp(): { app: App<{ count: number }> } {
  const app = new App({ count: 0 });
  const increment = app.action("increment", schema.literal(null), async (_input, context) => {
    await context.update((current) => { current.count += 1; });
  });
  app.surface("main", (ui, current) => {
    ui.text(`Count: ${current.count}`, { key: "count" });
    ui.button("Increment", increment.call(null), { key: "increment" });
  });
  return { app };
}

test("publishes a discoverable socket and manifest, then removes both on close", async (t) => {
  const root = await runtimeDir(t);
  const transport = new LocalServerTransport({ serviceId: "demo", name: "Demo", icon: "🎤", runtimeDir: root });
  const { app } = counterApp();
  const running = app.start(transport);
  await transport.ready;

  const directory = path.join(root, "svc-demo~");
  assert.equal(transport.directory, directory);
  assert.equal(socketPath(transport), path.join(directory, "a2ui.sock"));
  assert.deepEqual(JSON.parse(await readFile(path.join(directory, "manifest.json"), "utf8")), {
    name: "Demo",
    version: 2, serviceId: "demo", endpoint: { kind: "unix", path: "a2ui.sock" },
    icon: "🎤",
  });

  const client = connectClient(socketPath(transport));
  t.after(() => client.socket.destroy());
  const first = await client.next();
  assert.deepEqual(first, {
    version: "v0.9",
    createSurface: { surfaceId: "main", catalogId: "https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json", sendDataModel: true },
  });
  await client.next();
  await client.next();

  await transport.close();
  await running;
  await assert.rejects(access(path.join(directory, "manifest.json")));
  await assert.rejects(access(directory));
});

test("fans one application out to every client and snapshots for late joiners", async (t) => {
  const root = await runtimeDir(t);
  const transport = new LocalServerTransport({ serviceId: "demo", runtimeDir: root });
  const { app } = counterApp();
  const running = app.start(transport);
  await transport.ready;
  t.after(async () => {
    await transport.close();
    await running;
  });

  const first = connectClient(socketPath(transport));
  t.after(() => first.socket.destroy());
  await first.next();
  await first.next();
  await first.next();

  first.socket.write(`${JSON.stringify({
    version: "v0.9",
    action: { name: "increment", surfaceId: "main", sourceComponentId: "increment", timestamp: "2026-08-12T00:00:00.000Z", context: {} },
  })}\n`);
  await first.next();

  const late = connectClient(socketPath(transport));
  t.after(() => late.socket.destroy());
  const replay = [await late.next(), await late.next(), await late.next()];
  const components = replay.find((message) => "updateComponents" in message) as
    | { updateComponents: { components: Array<{ id: string; text?: string }> } }
    | undefined;
  assert.ok(components);
  assert.equal(components.updateComponents.components.find((component) => component.id === "count")?.text, "Count: 1");
});

test("refuses a service id that would escape the runtime directory", async (t) => {
  const root = await runtimeDir(t);
  for (const serviceId of [".", "..", "a/b", "", "x".repeat(65)]) {
    assert.throws(() => new LocalServerTransport({ serviceId, runtimeDir: root }), TypeError);
  }
});

test("recovers a socket left behind by a crash and refuses a live duplicate", async (t) => {
  const root = await runtimeDir(t);
  const first = new LocalServerTransport({ serviceId: "demo", runtimeDir: root });
  const { app } = counterApp();
  const running = app.start(first);
  await first.ready;

  const duplicate = new LocalServerTransport({ serviceId: "demo", runtimeDir: root });
  await assert.rejects(duplicate.ready, /WouldBlock|locked|temporarily unavailable/i);
  await duplicate.close();
  await access(socketPath(first));
  await access(path.join(first.directory!, "manifest.json"));
  const surviving = connectClient(socketPath(first));
  await surviving.next();
  surviving.socket.destroy();

  await first.close();
  await running;

  // A stale socket file must not block the next start.
  const restarted = new LocalServerTransport({ serviceId: "demo", runtimeDir: root });
  const second = counterApp();
  const secondRunning = second.app.start(restarted);
  await restarted.ready;
  t.after(async () => {
    await restarted.close();
    await secondRunning;
  });
  assert.ok(socketPath(restarted));
});


test("close during startup is idempotent and releases publication ownership", async (t) => {
  const root = await runtimeDir(t);
  const transport = new LocalServerTransport({ serviceId: "early", runtimeDir: root });
  const closing = transport.close();
  assert.equal(transport.close(), closing);
  await closing;
  assert.equal(transport.endpoint, null);
  const successor = new LocalServerTransport({ serviceId: "early", runtimeDir: root });
  await successor.ready;
  await transport.close();
  await access(path.join(successor.directory!, "manifest.json"));
  await successor.close();
});

for (const runtime of ["node", "bun"]) {
  test(`bounds unix slow-reader bytes while healthy replay and resources progress in ${runtime}`, { timeout: 35_000 }, () => {
    const result = spawnSync(runtime, [
      "--unhandled-rejections=strict",
      fileURLToPath(new URL("./fixtures/server-slow-reader.mjs", import.meta.url)),
      "unix",
    ], { encoding: "utf8", timeout: 30_000 });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), "slow reader isolated");
  });
}

function socketPath(transport: LocalServerTransport): string {
  const endpoint = transport.endpoint;
  assert.ok(endpoint?.kind === "unix");
  return endpoint.path;
}
