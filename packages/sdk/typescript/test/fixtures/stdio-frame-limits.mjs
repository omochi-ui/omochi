import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { App, StdioTransport, schema } from "@omochi-ui/sdk";

const limit = 4 * 1024 * 1024;
const mode = process.argv[3];
const emit = value => console.log(JSON.stringify(value));
const oversized = error => error.name === "JsonlFrameTooLargeError";

if (process.argv[2] === "child") {
  const transport = new StdioTransport();
  if (mode === "runtime") {
    const release = Promise.withResolvers();
    const send = transport.send.bind(transport);
    transport.send = async frame => {
      await send(frame);
      if (frame.kind === "response" && frame.id === "pending") release.resolve();
    };
    const app = new App({});
    app.action("wait", schema.literal(null), async () => {
      emit({ started: true });
      await release.promise;
    });
    const runtime = app.build();
    const result = runtime.start(transport);
    await assert.rejects(result, oversized);
    await runtime.waitClosed();
  } else {
    emit({ ready: true });
    const messages = transport[Symbol.asyncIterator]();
    if (mode === "overflow") {
      await assert.rejects(messages.next(), oversized);
    } else {
      assert.deepEqual((await messages.next()).value, { text: "雪🦀" });
      assert.equal((await messages.next()).done, true);
    }
    transport.stopReceiving();
  }
  if (mode !== "boundary") assert.equal(process.stdin.destroyed, false);
  for (const event of ["data", "readable"]) {
    assert.equal(process.stdin.listenerCount(event), 0, `leaked stdin ${event} listener`);
  }
  emit({ finished: true });
} else {
  assert.equal(process.versions.bun ? "bun" : "node", process.argv[2], "unexpected test runtime");
  function deadline(promise, name) {
    let timer;
    return Promise.race([
      promise,
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`timed out: ${name}`)), 3000); }),
    ]).finally(() => clearTimeout(timer));
  }
  async function child(mode, run) {
    const process = spawn(globalThis.process.execPath, [
      "--unhandled-rejections=strict", fileURLToPath(import.meta.url), "child", mode,
    ], { stdio: ["pipe", "pipe", "inherit"] });
    process.stdin.on("error", () => {});
    const exited = new Promise((resolve, reject) => {
      process.once("error", reject);
      process.once("exit", (code, signal) => resolve({ code, signal }));
    });
    const output = createInterface({ input: process.stdout, crlfDelay: Infinity })[Symbol.asyncIterator]();
    const next = async () => {
      const result = await deadline(output.next(), `${mode} child output`);
      assert.equal(result.done, false);
      return JSON.parse(result.value);
    };
    const write = bytes => new Promise(resolve => process.stdin.write(bytes, resolve));
    try {
      const ready = await next();
      assert.ok(ready.ready || ready.kind === "hello");
      await run({ process, next, write });
      assert.deepEqual(await next(), { finished: true });
      // Read completion above must happen with stdin open. Node's process-global
      // stdin pipe can retain an event-loop reference even after pause/cleanup.
      process.stdin.end();
      assert.deepEqual(await deadline(exited, "child exit"), { code: 0, signal: null });
    } finally {
      process.kill();
      process.stdin.destroy();
      process.stdout.destroy();
      await exited;
    }
  }

  // All oversized inputs keep stdin open. No LF/EOF may be needed to fail.
  const resource = Buffer.from(JSON.stringify({ omochi: "resources/1", kind: "request", padding: "x".repeat(limit) }));
  for (const frame of [
    Buffer.alloc(limit + 1, " "),
    Buffer.concat([Buffer.alloc(limit + 1, " "), Buffer.from('\n"stop"\n')]),
    Buffer.concat([Buffer.alloc(limit, " "), Buffer.from("\r")]),
    Buffer.from(`"${"雪".repeat(Math.ceil(limit / 3))}"`),
    resource,
  ]) {
    await child("overflow", async ({ write }) => {
      for (let offset = 0; offset < frame.length; offset += 65537) {
        if (await write(frame.subarray(offset, offset + 65537))) break;
      }
    });
  }
  for (const ending of ["\n", "\r\n", ""]) {
    await child("boundary", async ({ process, write }) => {
      const message = Buffer.from(JSON.stringify({ text: "雪🦀" }));
      const padding = Buffer.alloc(limit - message.length - Number(ending.startsWith("\r")), " ");
      await write(padding);
      // A multi-byte character is split across writes.
      const split = message.indexOf(Buffer.from("雪")) + 1;
      await write(message.subarray(0, split));
      await write(Buffer.concat([message.subarray(split), Buffer.from(ending)]));
      process.stdin.end();
    });
  }
  await child("runtime", async ({ next, write }) => {
    await write(Buffer.from(`${JSON.stringify({ type: "invoke", action: "wait", input: null })}\n`));
    assert.deepEqual(await next(), { started: true });
    await write(Buffer.from(`${JSON.stringify({ omochi: "resources/1", kind: "request", id: "pending", resource: "0".repeat(32), op: "stat" })}\r\n`));
    const response = await next();
    assert.equal(response.id, "pending");
    assert.equal(response.status, 404);
    await write(Buffer.alloc(limit + 1, " "));
  });
  console.log("stdio frames bounded");
}
