import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "@omochi-ui/core/testing";

import { App } from "../src/index.js";
import { Reconciler } from "../src/reconcile.js";
import type { A2UIMessage, Declaration, JsonValue, LocalDeclaration } from "../src/types.js";

type LocalResetValue = LocalDeclaration & { initial: string };

const fixture = JSON.parse(readFileSync(new URL("../../../../spec/fixtures/reconciliation.json", import.meta.url), "utf8")) as {
  cases: { name: string; before: JsonValue; after: JsonValue; expectedMessages: A2UIMessage[] }[];
  localResetCases: { name: string; before: LocalResetValue; after: LocalResetValue; expectedMessages: A2UIMessage[] }[];
};

function declaration(value: JsonValue): Declaration {
  return {
    irVersion: "2",
    surfaceId: "main",
    catalogId: "https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json",
    root: {
      key: "root", component: "Button",
      props: { action: { event: { name: "set", context: { input: value } } } },
      slots: [{ path: "/child", kind: "single", nodes: [{ key: "label", component: "Text", props: { text: "Set" } }] }],
    },
    data: { value },
    locals: { form: { initial: { draft: value }, resetKey: value } },
    requiredCapabilities: [],
  };
}

test("undefined reset keys match omitted keys and differ from null", () => {
  const reconciler = new Reconciler();
  const render = (local: LocalDeclaration) => reconciler.render({
    ...declaration("unchanged"), locals: { form: local },
  });
  const reset: A2UIMessage[] = [
    { version: "v0.9", updateDataModel: { surfaceId: "main", path: "/$local/form", value: "new" } },
  ];
  render({ initial: "old", resetKey: undefined });
  assert.deepEqual(render({ initial: "new" }), []);
  assert.deepEqual(render({ initial: "new", resetKey: undefined }), []);
  assert.deepEqual(render({ initial: "new", resetKey: null }), reset);
  assert.deepEqual(render({ initial: "new", resetKey: undefined }), reset);
});

for (const scenario of fixture.localResetCases) {
  test(`reconciles local reset key presence: ${scenario.name}`, () => {
    const reconciler = new Reconciler();
    const before = { ...declaration("unchanged"), locals: { form: scenario.before } };
    const after = { ...declaration("unchanged"), locals: { form: scenario.after } };
    reconciler.render(before);
    assert.deepEqual(reconciler.render(after), scenario.expectedMessages);
    assert.deepEqual(reconciler.render(after), []);

    const state = { local: scenario.before };
    const app = new App(state);
    app.surface("main", (ui, current) => {
      const local = current.local;
      if ("resetKey" in local) ui.local("form", local.initial, { resetKey: local.resetKey });
      else ui.local("form", local.initial);
      ui.text("Unchanged", { key: "label" });
    });
    const runtime = app.build();
    runtime.render();
    state.local = scenario.after;
    assert.deepEqual(runtime.render(), scenario.expectedMessages);
    assert.deepEqual(runtime.render(), []);
  });
}

for (const scenario of fixture.cases) {
  test(`reconciles JSON types: ${scenario.name}`, () => {
    const reconciler = new Reconciler();
    reconciler.render(declaration(scenario.before));
    assert.deepEqual(reconciler.render(declaration(scenario.after)), scenario.expectedMessages);
    assert.deepEqual(reconciler.render(declaration(scenario.after)), []);
  });
}
