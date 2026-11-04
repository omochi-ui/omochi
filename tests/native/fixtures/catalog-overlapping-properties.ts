import type { ForwardProps, ReverseProps, ParentProps, ParentReverseProps } from "./catalog.js";

const valid = { value: "b" as const, details: { label: "label", enabled: true, count: 2 } };
const forward: ForwardProps = valid;
const reverse: ReverseProps = valid;
const parent: ParentProps = valid;
const parentReverse: ParentReverseProps = valid;
// @ts-expect-error Every layer constrains ForwardProps.
const badAForward: ForwardProps = { ...valid, value: "a" };
// @ts-expect-error Every layer constrains ReverseProps.
const badAReverse: ReverseProps = { ...valid, value: "a" };
// @ts-expect-error Every layer constrains ParentProps.
const badAParent: ParentProps = { ...valid, value: "a" };
// @ts-expect-error Every layer constrains ParentReverseProps.
const badAParentReverse: ParentReverseProps = { ...valid, value: "a" };
// @ts-expect-error Every layer constrains ForwardProps.
const badCForward: ForwardProps = { ...valid, value: "c" };
// @ts-expect-error Every layer constrains ReverseProps.
const badCReverse: ReverseProps = { ...valid, value: "c" };
// @ts-expect-error Every layer constrains ParentProps.
const badCParent: ParentProps = { ...valid, value: "c" };
// @ts-expect-error Every layer constrains ParentReverseProps.
const badCParentReverse: ParentReverseProps = { ...valid, value: "c" };
// @ts-expect-error Every layer constrains ForwardProps.
const missingLabelForward: ForwardProps = { ...valid, details: { enabled: true, count: 2 } };
// @ts-expect-error Every layer constrains ReverseProps.
const missingLabelReverse: ReverseProps = { ...valid, details: { enabled: true, count: 2 } };
// @ts-expect-error Every layer constrains ParentProps.
const missingLabelParent: ParentProps = { ...valid, details: { enabled: true, count: 2 } };
// @ts-expect-error Every layer constrains ParentReverseProps.
const missingLabelParentReverse: ParentReverseProps = { ...valid, details: { enabled: true, count: 2 } };
// @ts-expect-error Every layer constrains ForwardProps.
const missingEnabledForward: ForwardProps = { ...valid, details: { label: "label", count: 2 } };
// @ts-expect-error Every layer constrains ReverseProps.
const missingEnabledReverse: ReverseProps = { ...valid, details: { label: "label", count: 2 } };
// @ts-expect-error Every layer constrains ParentProps.
const missingEnabledParent: ParentProps = { ...valid, details: { label: "label", count: 2 } };
// @ts-expect-error Every layer constrains ParentReverseProps.
const missingEnabledParentReverse: ParentReverseProps = { ...valid, details: { label: "label", count: 2 } };
// @ts-expect-error Every layer constrains ForwardProps.
const missingCountForward: ForwardProps = { ...valid, details: { label: "label", enabled: true } };
// @ts-expect-error Every layer constrains ReverseProps.
const missingCountReverse: ReverseProps = { ...valid, details: { label: "label", enabled: true } };
// @ts-expect-error Every layer constrains ParentProps.
const missingCountParent: ParentProps = { ...valid, details: { label: "label", enabled: true } };
// @ts-expect-error Every layer constrains ParentReverseProps.
const missingCountParentReverse: ParentReverseProps = { ...valid, details: { label: "label", enabled: true } };
// @ts-expect-error Every layer constrains ForwardProps.
const missingValueForward: ForwardProps = { details: valid.details };
// @ts-expect-error Every layer constrains ReverseProps.
const missingValueReverse: ReverseProps = { details: valid.details };
// @ts-expect-error Every layer constrains ParentProps.
const missingValueParent: ParentProps = { details: valid.details };
// @ts-expect-error Every layer constrains ParentReverseProps.
const missingValueParentReverse: ParentReverseProps = { details: valid.details };
// @ts-expect-error Every layer constrains ForwardProps.
const missingDetailsForward: ForwardProps = { value: "b" };
// @ts-expect-error Every layer constrains ReverseProps.
const missingDetailsReverse: ReverseProps = { value: "b" };
// @ts-expect-error Every layer constrains ParentProps.
const missingDetailsParent: ParentProps = { value: "b" };
// @ts-expect-error Every layer constrains ParentReverseProps.
const missingDetailsParentReverse: ParentReverseProps = { value: "b" };
console.log(JSON.stringify([forward, reverse, parent, parentReverse]));
