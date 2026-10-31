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
