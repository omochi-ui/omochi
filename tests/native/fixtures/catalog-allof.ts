import type { CombinedProps, KeywordProps } from "./catalog.js";

const props: CombinedProps = {
  object: { label: "label", enabled: true }, single: "single", primitive: "primitive",
  mixed: "ready", conditional: "ready", array: ["ready"], union: true, nested: "nested",
};
const keyword: KeywordProps = { class: "keyword" };
// @ts-expect-error Both object constraints remain present in TypeScript.
const incomplete: CombinedProps["object"] = { label: "missing enabled" };
// @ts-expect-error The string enum intersection remains restricted in TypeScript.
const invalid: CombinedProps["mixed"] = "other";
void [props, keyword, incomplete, invalid];
