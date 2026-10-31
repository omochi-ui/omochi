import assert from "node:assert/strict";
import { test } from "@omochi-ui/core/testing";
import { App, schema, ProtocolError, DeclarationError, type Expr, type JsonValue } from "../src/index.js";

const input = schema.object({ temperature: schema.number(), note: schema.optional(schema.string()) });
test("validates wire input before running a handler", async () => {
  let calls = 0;
  const app = new App({});
  app.action("run", input, () => { calls++; });
  const runtime = app.build();
  for (const value of ([{}, { temperature: "bad" }, { temperature: 1, extra: true }, { temperature: Infinity }] satisfies JsonValue[])) {
    await assert.rejects(runtime.invoke("run", value as JsonValue), ProtocolError);
  }
  await runtime.invoke("run", { temperature: 1 });
  assert.equal(calls, 1);
});
test("serializes nested expressions and rejects non-JSON input", () => {
  const app = new App({});
  const action = app.action("run", schema.object({ values: schema.array(schema.number()) }), () => {});
  app.surface("main", ui => {
    const local = ui.local("form", { n: 2 });
    assert.deepEqual(action.call({ values: [1, local.field("n").value()] }).input, { values: [1, { path: "/$local/form/n" }] });
    assert.throws(() => action.call({ values: [NaN] }), DeclarationError);
  });
  app.build().render();
});
test("schemas handle tuples, null, maps and unions without coercion", () => {
  const s = schema.object({ tuple: schema.tuple(schema.string(), schema.boolean()), map: schema.map(schema.nullable(schema.integer())), kind: schema.union(schema.literal("a"), schema.literal("b")) });
  const v = { tuple: ["x", true], map: { one: null, two: 2 }, kind: "a" };
  assert.deepEqual(s.parse(v), v);
  assert.throws(() => s.parse({ ...v, map: { two: "2" } }), ProtocolError);
});

// Compile-only regression cases. Calling this function is intentionally unnecessary.
function typing() {
  const app = new App({});
  const run = app.action("run", input, () => {});
  // @ts-expect-error missing required input
  run.call({});
  // @ts-expect-error wrong input type
  run.call({ temperature: "bad" });
  app.surface("main", ui => {
    const local = ui.local("form", { temperature: 1, name: "x" });
    // @ts-expect-error missing field
    local.field("missing");
    // @ts-expect-error writable binding has wrong type
    ui.slider("n", 0, 1, { bind: local.field("name").bind() });
    // @ts-expect-error expression result has wrong type
    const expr: Expr<number> = local.field("name").value();
    // @ts-expect-error default and binding are exclusive
    ui.slider("n", 0, 1, { default: 1, bind: local.field("temperature").bind() });
  });
}
void typing;

test("shared action input validation fixture", async () => {
  const fixture = await Bun.file(new URL("../../../../spec/fixtures/action-input.json", import.meta.url)).json();
  for (const value of fixture.valid) assert.deepEqual(input.parse(value), value);
  for (const value of fixture.invalid) assert.throws(() => input.parse(value), ProtocolError);
});

test("optional members stay optional through nullable and union", () => {
  for (const value of [schema.nullable(schema.optional(schema.string())), schema.union(schema.number(), schema.optional(schema.string()))]) {
    const object = schema.object({ value });
    assert.deepEqual(object.parse({}), {});
    assert.deepEqual(object.parse({ value: "yes" }), { value: "yes" });
    assert.throws(() => object.parse({ value: false }), ProtocolError);
  }
});

test("invalid action inputs do not terminate the receive loop", async () => {
  const app = new App({});
  const calls: number[] = [];
  app.action("run", schema.number(), n => { calls.push(n); });
  const runtime = app.build();
  await runtime.start({ capabilities: new Set(), async send() {}, async *[Symbol.asyncIterator]() {
    yield { action: { name: "run", context: { input: "invalid" } } };
    yield { action: { name: "run", context: { input: 42 } } };
  } });
  assert.deepEqual(calls, [42]);
});

test("handler and transport failures still terminate the receive loop", async () => {
  for (const error of [new Error("handler failed"), new ProtocolError("handler protocol error")]) {
    const app = new App({});
    app.action("run", schema.number(), () => { throw error; });
    await assert.rejects(app.build().start({ capabilities: new Set(), async send() {}, async *[Symbol.asyncIterator]() {
      yield { action: { name: "run", context: { input: 42 } } };
    } }), e => e === error);
  }
  const app = new App({});
  await assert.rejects(app.build().start({ capabilities: new Set(), async send() { throw new Error("transport failed"); }, async *[Symbol.asyncIterator]() {} }), /transport failed/);
});

function scalarTyping() {
  new App({}).surface("main", ui => {
    ui.checkBox("enabled", { label: "Enabled", bind: ui.local("enabled", false).bind() });
    ui.slider("n", 0, 10, { bind: ui.local("n", 0).bind() });
    ui.textField("text", { label: "Text", bind: ui.local("text", "").bind() });
    // @ts-expect-error binding is still invariant
    ui.textField("invalid", { label: "Invalid", bind: ui.local("flag", true).bind() });
  });
}
void scalarTyping;

test("object merges are typed and require an explicit renderer capability", async () => {
  const { mergeObjects, Expr, MERGE_OBJECTS_CAPABILITY, CapabilityError, Reconciler } = await import("../src/index.js");
  const fixture = await Bun.file(new URL("../../../../spec/fixtures/action-input.json", import.meta.url)).json();
  const app = new App({});
  const run = app.action("run", schema.object({ title: schema.string(), count: schema.number() }), () => {});
  // @ts-expect-error scalar expressions are not object operands
  mergeObjects(Expr.reference<number>("/scalar"));
  // @ts-expect-error arrays are not object operands
  mergeObjects([1, 2]);
  const input = mergeObjects({ title: "x" }, Expr.reference<{ count: number }>("/$local/nested"));
  assert.deepEqual(run.call(input).input, fixture.merge.expression);
  // @ts-expect-error inferred merge output does not satisfy action input
  run.call(mergeObjects({ title: "x" }, { count: "wrong" }));
  app.surface("main", ui => ui.button("Run", run.call(input)));
  const declaration = app.build().declaration("main");
  assert.deepEqual(declaration.requiredCapabilities, [MERGE_OBJECTS_CAPABILITY]);
  assert.throws(() => new Reconciler().render(declaration), CapabilityError);
  assert.ok(new Reconciler([MERGE_OBJECTS_CAPABILITY]).render(declaration).length);
});
