import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "@omochi-ui/core/testing";

import { Reconciler } from "../src/reconcile.js";
import type { A2UIMessage, Declaration, JsonValue } from "../src/types.js";

const fixture = JSON.parse(readFileSync(new URL("../../../../spec/fixtures/reconciliation.json", import.meta.url), "utf8")) as {
  cases: { name: string; before: JsonValue; after: JsonValue; expectedMessages: A2UIMessage[] }[];
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

for (const scenario of fixture.cases) {
  test(`reconciles JSON types: ${scenario.name}`, () => {
    const reconciler = new Reconciler();
    reconciler.render(declaration(scenario.before));
    assert.deepEqual(reconciler.render(declaration(scenario.after)), scenario.expectedMessages);
    assert.deepEqual(reconciler.render(declaration(scenario.after)), []);
  });
}
