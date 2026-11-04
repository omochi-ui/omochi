import type { OptionalProps, RequiredOnlyProps, ShortcutProps } from "./catalog.js";

const shortcut: ShortcutProps = { key: "Enter" };
const optional: OptionalProps = {};
const supplied: OptionalProps = { key: "Escape" };
const required: RequiredOnlyProps = { key: { arbitrary: true } };
// @ts-expect-error The component property key remains required.
const missing: ShortcutProps = {};
// @ts-expect-error The component property key retains its schema type.
const wrong: ShortcutProps = { key: 1 };
// @ts-expect-error Required-only key survives local references and allOf.
const missingRequired: RequiredOnlyProps = {};
console.log(JSON.stringify([shortcut, optional, supplied, required]));
void [missing, wrong, missingRequired];
