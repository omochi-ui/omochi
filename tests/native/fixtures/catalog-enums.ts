import type { ScalarProps } from "./catalog.js";

const valid: ScalarProps = {
  enabled: true, count: 1, ratio: 1.5, empty: null, mixed: "true", tone: "info",
  flags: [true, false], counts: [[1, 2]], choices: [true, 2], nullable: null,
};
// @ts-expect-error Boolean enum members must retain their JSON type.
const stringBoolean: ScalarProps["enabled"] = "true";
// @ts-expect-error Numeric enum members must retain their JSON type.
const stringNumber: ScalarProps["count"] = "1";
// @ts-expect-error Numbers outside the enum are invalid.
const invalidNumber: ScalarProps["count"] = 3;
// @ts-expect-error Floating-point enum values are restricted in TypeScript.
const invalidRatio: ScalarProps["ratio"] = 3.5;
// @ts-expect-error Nested enums must retain their JSON type.
const invalidFlags: ScalarProps["flags"] = ["true"];
// @ts-expect-error Nested numeric enums must retain their JSON type.
const invalidCounts: ScalarProps["counts"] = [["1"]];
// @ts-expect-error Union enum members must retain their JSON type.
const invalidChoices: ScalarProps["choices"] = ["true"];
void [valid, stringBoolean, stringNumber, invalidNumber, invalidRatio, invalidFlags, invalidCounts, invalidChoices];
