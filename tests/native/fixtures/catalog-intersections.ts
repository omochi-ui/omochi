import type { IntersectionProps } from "./catalog.js";

const valid: IntersectionProps = {
  choices: "b", any: 1, one: true,
  children: { componentId: "child", path: "/items", marker: "marked" },
  nested: [["b"], []],
};
const otherAny: IntersectionProps["any"] = 2;
const otherOne: IntersectionProps["one"] = 1;

// @ts-expect-error The first enum's exclusive member fails the second constraint.
const firstChoice: IntersectionProps["choices"] = "a";
// @ts-expect-error The second enum's exclusive member fails the first constraint.
const lastChoice: IntersectionProps["choices"] = "c";
// @ts-expect-error An anyOf alternative must also satisfy the enum constraint.
const anyString: IntersectionProps["any"] = "text";
// @ts-expect-error Numbers must still belong to the enum.
const anyNumber: IntersectionProps["any"] = 3;
// @ts-expect-error A oneOf alternative must also satisfy the enum constraint.
const oneBoolean: IntersectionProps["one"] = false;
// @ts-expect-error Symbolic ChildList array alternatives also need the marker.
const childrenArray: IntersectionProps["children"] = ["child"];
// @ts-expect-error Symbolic ChildList object alternatives also need the marker.
const childrenObject: IntersectionProps["children"] = { componentId: "child", path: "/items" };
// @ts-expect-error Nested intersections inside nested arrays retain every constraint.
const nestedChoice: IntersectionProps["nested"] = [["a"]];
// @ts-expect-error Nested intersection arrays reject scalar inner values.
const flatNested: IntersectionProps["nested"] = ["b"];

void [valid, otherAny, otherOne, firstChoice, lastChoice, anyString, anyNumber, oneBoolean, childrenArray, childrenObject, nestedChoice, flatNested];
