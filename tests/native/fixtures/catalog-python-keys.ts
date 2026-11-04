import type { NamedProps } from "./catalog.js";

const props: NamedProps = {
  class: "button", async: true, "data-label": "label", "1st": 1,
  "": "empty", __private: "private", "K": "kelvin", 'quote"line\n': "escaped",
};
const optional: NamedProps = { ...props, None: "none", "optional key": "optional" };
// @ts-expect-error The original property name is required.
const renamed: NamedProps["dataLabel"] = "label";
console.log(JSON.stringify([props, optional]));
void renamed;
