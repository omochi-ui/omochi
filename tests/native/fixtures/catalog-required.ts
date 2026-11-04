import type { RecordProps } from "./catalog.js";

const minimal: RecordProps = {
  value: {},
  emptyRequired: {},
  partial: { label: "required" },
  full: { enabled: true, count: 1 },
  nested: { value: {} },
  items: [{ label: "required" }],
  optionalItems: [{}],
  intersection: { label: "required" },
  any: { label: "required" },
  one: { enabled: true, count: 1 },
  composedItems: [{ label: "required" }, false],
};
const populated: RecordProps = {
  ...minimal,
  value: { optional: "present" },
  emptyRequired: { optional: "present" },
  partial: { label: "required", count: 1 },
  nested: { value: { optional: "present" }, details: { label: "required" } },
  items: [{ label: "required", count: 1 }],
  optionalItems: [{ optional: "present" }],
  intersection: { label: "required", optional: "present" },
  any: false,
  one: "alternative",
  composedItems: [{ label: "required", optional: "present" }],
};

// @ts-expect-error Partial required lists still require the listed property.
const missingPartial: RecordProps["partial"] = {};
// @ts-expect-error Every property listed in required must be supplied.
const missingCount: RecordProps["full"] = { enabled: true };
// @ts-expect-error The other required property must also be supplied.
const missingEnabled: RecordProps["full"] = { count: 1 };
// @ts-expect-error The parent object's required list is independent of its child's.
const missingNestedValue: RecordProps["nested"] = {};
// @ts-expect-error An optional object still enforces its own required fields when present.
const missingNestedLabel: RecordProps["nested"] = { value: {}, details: {} };
// @ts-expect-error Array elements retain their required fields.
const missingItemLabel: RecordProps["items"] = [{}];
// @ts-expect-error Intersections retain required fields from each object layer.
const missingIntersectionLabel: RecordProps["intersection"] = { optional: "present" };
// @ts-expect-error anyOf object branches retain required fields.
const missingAnyLabel: RecordProps["any"] = {};
// @ts-expect-error oneOf object branches retain required fields.
const missingOneCount: RecordProps["one"] = { enabled: true };
// @ts-expect-error Arrays of composed object types retain required fields.
const missingComposedLabel: RecordProps["composedItems"] = [{}];
// @ts-expect-error Optional fields retain their declared value types.
const wrongOptionalType: RecordProps["value"] = { optional: 1 };

void [minimal, populated, missingPartial, missingCount, missingEnabled, missingNestedValue,
  missingNestedLabel, missingItemLabel, missingIntersectionLabel, missingAnyLabel,
  missingOneCount, missingComposedLabel, wrongOptionalType];
