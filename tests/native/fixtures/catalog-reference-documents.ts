import type { ReferenceProps } from "./catalog.js";

const props: ReferenceProps = { value: "label" };
// @ts-expect-error The external document's local reference resolves to string.
const wrong: ReferenceProps = { value: 42 };
// @ts-expect-error The reference property remains required.
const missing: ReferenceProps = {};
void [props, wrong, missing];
