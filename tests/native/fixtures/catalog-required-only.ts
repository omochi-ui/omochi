import type { DirectProps, NestedProps, OnlyProps } from "./catalog.js";

const direct: DirectProps = { label: "label", metadata: { enabled: true } };
const nested: NestedProps = { label: "label", parentData: null, nestedData: [1, "two"] };
const only: OnlyProps = { payload: false };
// @ts-expect-error Required keys without property schemas must still be supplied.
const missingDirect: DirectProps = { label: "label" };
// @ts-expect-error Parent required-only keys survive recursive allOf.
const missingParent: NestedProps = { label: "label", nestedData: [] };
// @ts-expect-error Nested required-only keys survive recursive allOf.
const missingNested: NestedProps = { label: "label", parentData: null };
// @ts-expect-error Components without properties still have required keys.
const missingOnly: OnlyProps = {};
// @ts-expect-error A later allOf layer supplies the required property's type.
const wrongLabel: NestedProps = { ...nested, label: 1 };
// @ts-expect-error An unconstrained required value is unknown, not any.
const uncheckedMetadata: string = direct.metadata;
// @ts-expect-error Wire component and id remain reserved.
const reserved: DirectProps = { ...direct, component: "Direct", id: "id" };

console.log(JSON.stringify([direct, nested, only]));
void [missingDirect, missingParent, missingNested, missingOnly, wrongLabel, uncheckedMetadata, reserved];
