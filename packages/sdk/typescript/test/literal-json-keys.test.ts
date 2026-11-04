import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "@omochi-ui/core/testing";

import { App, BASIC_CATALOG, DeclarationError, Reconciler, UI, schema } from "../src/index.js";
import type { A2UIMessage, Declaration, JsonObject, NodeIR } from "../src/index.js";

const fixture = JSON.parse(readFileSync(new URL("../../../../spec/fixtures/literal-json-keys.json", import.meta.url), "utf8")) as {
  catalogId: string;
  keys: string[];
  builder: { defaults: Record<string, string>; initials: Record<string, Record<string, { value: string }>> };
  root: NodeIR;
  steps: { name: string; state: Pick<Declaration, "data" | "locals">; expectedMessages: A2UIMessage[] }[];
  flatten: { root: NodeIR; expectedComponents: JsonObject[] };
};

function declaration(root: NodeIR, state: Pick<Declaration, "data" | "locals">): Declaration {
  return { irVersion: "2", surfaceId: "main", catalogId: fixture.catalogId, root, ...state, requiredCapabilities: [] };
}

test("reconciles literal JSON keys using the shared fixture", () => {
  const reconciler = new Reconciler();
  for (const step of fixture.steps) {
    const value = declaration(fixture.root, step.state);
    assert.deepEqual(reconciler.render(value), step.expectedMessages, step.name);
    assert.deepEqual(reconciler.render(value), [], step.name);
  }
});

test("public builder retains prototype names as local and default input keys", () => {
  const ui = new UI("main", BASIC_CATALOG);
  for (const key of fixture.keys) {
    const local = ui.local(key, fixture.builder.initials[key]!);
    assert.equal(local.bind().path, `/$local/${key}`);
    assert.throws(() => ui.local(key, "duplicate"), DeclarationError);
    ui.textField(key, { label: key, default: fixture.builder.defaults[key]! });
  }
  const result = ui.build();
  assert.deepEqual(result.data, { inputs: fixture.builder.defaults });
  assert.deepEqual(result.locals, Object.fromEntries(fixture.keys.map(key => [key, { initial: fixture.builder.initials[key] }])));
  const serialized = JSON.parse(JSON.stringify(result)) as Declaration;
  assert.deepEqual(serialized, result);
  assert.equal(Object.getPrototypeOf(result.locals), Object.prototype);
  assert.equal(Object.getPrototypeOf(result.data.inputs), Object.prototype);
  const messages = new Reconciler().render(result);
  assert.deepEqual(messages.at(-1)!.updateDataModel, { surfaceId: "main", path: "/", value: { inputs: fixture.builder.defaults, $local: fixture.builder.initials } });

  const next = new UI("main", BASIC_CATALOG, result.data);
  for (const key of fixture.keys) next.textField(key, { label: key, default: "replacement" });
  assert.deepEqual(next.build().data, result.data);
  assert.deepEqual(new UI("main", BASIC_CATALOG, result.data).build().data, { inputs: {} });
});

test("slot pointers create own properties without changing object prototypes", () => {
  const prototype = Object.getOwnPropertyDescriptors(Object.prototype);
  const messages = new Reconciler().render(declaration(fixture.flatten.root, { data: {}, locals: {} }));
  assert.deepEqual(Object.getOwnPropertyDescriptors(Object.prototype), prototype);
  assert.deepEqual(messages[1]!.updateComponents, { surfaceId: "main", components: fixture.flatten.expectedComponents });
  const components = (messages[1]!.updateComponents as JsonObject).components as JsonObject[];
  assert.equal(Object.getPrototypeOf(components.at(-1)), Object.prototype);
});

test("schemas and action inputs preserve literal JSON keys", async () => {
  const input = schema.object(Object.fromEntries(fixture.keys.map(key => [key, schema.string()])));
  for (const shape of [input, schema.map(schema.string()), schema.object({}, { unknownKeys: "allow" })]) {
    const parsed = shape.parse(fixture.builder.defaults);
    assert.deepEqual(parsed, fixture.builder.defaults);
    assert.equal(Object.getPrototypeOf(parsed), Object.prototype);
  }
  const app = new App({});
  let calls = 0;
  const action = app.action("echo", input, value => {
    assert.deepEqual(value, fixture.builder.defaults);
    calls++;
  });
  assert.deepEqual(action.call(fixture.builder.defaults).input, fixture.builder.defaults);
  await app.build().invoke("echo", fixture.builder.defaults);
  assert.equal(calls, 1);
});
