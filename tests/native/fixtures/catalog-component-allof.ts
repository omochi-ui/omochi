import type { EmptyProps, NestedProps, SiblingProps } from "./catalog.js";

const sibling: SiblingProps = { label: "label", count: 2 };
const nested: NestedProps = { label: "label", count: 2, isValid: true };
const empty: EmptyProps = { label: "label" };
const optional: NestedProps = {
  ...nested, note: "note", accessibility: { label: "accessible" },
  checks: [], validationErrors: ["error"],
};
// @ts-expect-error Required parent properties survive allOf.
const missingParent: SiblingProps = { count: 2 };
// @ts-expect-error Required child properties survive allOf.
const missingChild: SiblingProps = { label: "label" };
// @ts-expect-error Required properties survive nested allOf.
const missingNested: NestedProps = { count: 2, isValid: true };
// @ts-expect-error A parent's required list applies to child properties.
const missingCount: NestedProps = { label: "label", isValid: true };
// @ts-expect-error A parent's required list applies to common mixin properties.
const missingCommon: NestedProps = { label: "label", count: 2 };
// @ts-expect-error An empty allOf does not erase required parent properties.
const missingEmpty: EmptyProps = {};
// @ts-expect-error Optional nested fields retain their types.
const wrongNote: NestedProps = { ...nested, note: 2 };
// @ts-expect-error Conditional branch fields are not unconditional props.
const conditional: SiblingProps = { ...sibling, conditional: "conditional" };
// @ts-expect-error The component id remains reserved even in nested layers.
const reserved: NestedProps = { ...nested, id: "id" };

console.log(JSON.stringify([sibling, nested, empty, optional]));
void [missingParent, missingChild, missingNested, missingCount, missingCommon,
  missingEmpty, wrongNote, conditional, reserved];
