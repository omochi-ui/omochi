import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "@omochi-ui/core/testing";
import { parseClientFrame } from "@omochi-ui/core";

import { schema, App, CallbackTransport, DeclarationError, ProtocolError, type JsonObject } from "../src/index.js";

const fixture = JSON.parse(readFileSync(new URL("../../../../spec/fixtures/basic.json", import.meta.url), "utf8")) as { initialMessages: JsonObject[]; action: JsonObject; afterActionMessages: JsonObject[] };
const clientErrors = JSON.parse(readFileSync(new URL("../../../../spec/fixtures/client-errors.json", import.meta.url), "utf8")) as { reports: JsonObject[]; invalid: JsonObject[] };

function messagesTransport(messages: JsonObject[], sent: JsonObject[] = []) {
  return new CallbackTransport(async message => { sent.push(message); }, async function* () { yield* messages; });
}

function makeRuntime() {
  const state = { temperature: 50, result: null as string | null };
  const app = new App(state);
  const run = app.action("run", schema.object({ temperature: schema.number() }), async (input, context) => {
    await context.update((current) => { current.temperature = input.temperature; current.result = `Result: ${input.temperature}`; });
  });
  app.surface("main", (ui, current) => {
    const form = ui.local("form", { temperature: current.temperature });
    ui.text("Hello", { key: "hello" });
    ui.slider("temperature", 0, 100, { bind: form.field("temperature").bind() });
    ui.button("Run", run.call({ temperature: form.field("temperature").value() }));
    if (current.result) ui.text(current.result, { key: "result" });
  });
  return { runtime: app.build(), state };
}

test("matches the shared State/Surface/Action fixture", async () => {
  const { runtime, state } = makeRuntime();
  assert.deepEqual(runtime.render(), fixture.initialMessages);
  assert.deepEqual(await runtime.handle(fixture.action), fixture.afterActionMessages);
  assert.equal(state.temperature, 75);
});

test("client error reports allow subsequent actions through the receive loop", async () => {
  for (const report of clientErrors.reports) {
    assert.equal(parseClientFrame(JSON.stringify(report), false).ok, true);
    const { runtime, state } = makeRuntime();
    assert.deepEqual(await runtime.handle(report), []);
    const sent: JsonObject[] = [];
    await runtime.start(messagesTransport([report, fixture.action], sent));
    assert.equal(state.temperature, 75);
    assert.deepEqual(sent.slice(1), [...fixture.initialMessages, ...fixture.afterActionMessages]);
  }
});

test("malformed client reports and unknown messages still stop the receive loop", async () => {
  for (const message of clientErrors.invalid) {
    const { runtime, state } = makeRuntime();
    await assert.rejects(runtime.handle(message), ProtocolError);
    await assert.rejects(runtime.start(messagesTransport([message, fixture.action])), ProtocolError);
    assert.equal(state.temperature, 50);
  }
});

test("client reports do not mask subsequent handler failures", async () => {
  for (const failure of [new Error("handler failed"), new ProtocolError("handler failed")]) {
    const app = new App({});
    let calls = 0;
    app.action("fail", schema.literal(null), () => { calls++; throw failure; });
    const action = { type: "invoke", action: "fail" };
    await assert.rejects(app.build().start(messagesTransport([...clientErrors.reports, action, action])), error => error === failure);
    assert.equal(calls, 1);
  }
});

test("invokes a typed action without a Surface", async () => {
  const state = { value: 0 };
  const app = new App(state);
  app.action("set", schema.object({ value: schema.number() }), async (input, context) => context.update((current) => { current.value = input.value; }));
  const runtime = app.build();
  assert.deepEqual(await runtime.handle({ type: "invoke", action: "set", input: { value: 7 } }), []);
  assert.equal(state.value, 7);
});

test("keeps local drafts across rerenders and resets only on resetKey changes", async () => {
  const state = { title: "one", revision: 0 };
  const app = new App(state);
  app.surface("main", (ui, current) => { const form = ui.local("form", { draft: current.title }, { resetKey: current.revision }); ui.textField("draft", { label: "Draft", bind: form.field("draft").bind() }); });
  const runtime = app.build();
  runtime.render();
  assert.deepEqual(await runtime.update((current) => { current.title = "two"; }), []);
  const reset = await runtime.update((current) => { current.revision++; });
  assert.deepEqual(reset, [{ version: "v0.9", updateDataModel: { surfaceId: "main", path: "/$local/form", value: { draft: "two" } } }]);
});

test("updates every Surface through the same transaction", async () => {
  const state = { value: 0 };
  const app = new App(state);
  const increment = app.action("increment", schema.literal(null), async (_input, context) => context.update((current) => { current.value++; }));
  app.surface("one", (ui, current) => { ui.text(String(current.value), { key: "value" }); ui.button("+", increment.call(null)); });
  app.surface("two", (ui, current) => { ui.text(String(current.value), { key: "value" }); });
  const runtime = app.build(); runtime.render("one"); runtime.render("two");
  const messages = await runtime.invoke("increment", null);
  assert.deepEqual(new Set(messages.map((message) => ((message.updateComponents as JsonObject).surfaceId))), new Set(["one", "two"]));
});

test("coalesces concurrent external updates into one latest render", async () => {
  const state = { value: 0 }; const app = new App(state);
  app.surface("main", (ui, current) => ui.text(String(current.value), { key: "value" }));
  const runtime = app.build(); runtime.render();
  const [first, second] = await Promise.all([runtime.update((current) => { current.value = 1; }), runtime.update((current) => { current.value = 2; })]);
  assert.deepEqual(first, second);
  assert.equal(((first[0]!.updateComponents as JsonObject).components as JsonObject[])[0]!.text, "2");
});

test("getSurface is a full observation independent of reconciliation history", async () => {
  const { runtime } = makeRuntime(); runtime.render();
  const messages = await runtime.handle({ type: "getSurface", surface: "main" });
  assert.equal(messages[0]!.createSurface !== undefined, true);
});

test("rejects duplicate component and local keys", () => {
  const app = new App({}); app.surface("main", (ui) => { ui.local("x", {}); ui.local("x", {}); });
  assert.throws(() => app.build().render(), DeclarationError);
});

test("start sends a full snapshot even after pre-render and exposes lifecycle", async () => {
  const { runtime } = makeRuntime(); runtime.render();
  const sent: JsonObject[] = [];
  const transport = { capabilities: new Set<string>(), async send(message: JsonObject) { sent.push(message); }, async *[Symbol.asyncIterator]() {} };
  await runtime.start(transport);
  assert.deepEqual(sent[0], { omochi: "resources/1", kind: "hello" });
  assert.equal(sent[1]!.createSurface !== undefined, true);
  await runtime.ready(); await runtime.waitClosed();
});
