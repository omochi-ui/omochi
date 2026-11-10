import type {
  ForwardProps,
  ReverseProps,
  ParentProps,
  ParentReverseProps,
} from './catalog.js';

const valid = {
  value: 'b' as const,
  details: { label: 'label', enabled: true, count: 2 },
};
const forward: ForwardProps = valid;
const reverse: ReverseProps = valid;
const parent: ParentProps = valid;
const parentReverse: ParentReverseProps = valid;
// @ts-expect-error Every layer constrains ForwardProps.
const _badAForward: ForwardProps = { ...valid, value: 'a' };
// @ts-expect-error Every layer constrains ReverseProps.
const _badAReverse: ReverseProps = { ...valid, value: 'a' };
// @ts-expect-error Every layer constrains ParentProps.
const _badAParent: ParentProps = { ...valid, value: 'a' };
// @ts-expect-error Every layer constrains ParentReverseProps.
const _badAParentReverse: ParentReverseProps = { ...valid, value: 'a' };
// @ts-expect-error Every layer constrains ForwardProps.
const _badCForward: ForwardProps = { ...valid, value: 'c' };
// @ts-expect-error Every layer constrains ReverseProps.
const _badCReverse: ReverseProps = { ...valid, value: 'c' };
// @ts-expect-error Every layer constrains ParentProps.
const _badCParent: ParentProps = { ...valid, value: 'c' };
// @ts-expect-error Every layer constrains ParentReverseProps.
const _badCParentReverse: ParentReverseProps = { ...valid, value: 'c' };
const _missingLabelForward: ForwardProps = {
  ...valid,
  // @ts-expect-error Every layer constrains ForwardProps.
  details: { enabled: true, count: 2 },
};
const _missingLabelReverse: ReverseProps = {
  ...valid,
  // @ts-expect-error Every layer constrains ReverseProps.
  details: { enabled: true, count: 2 },
};
const _missingLabelParent: ParentProps = {
  ...valid,
  // @ts-expect-error Every layer constrains ParentProps.
  details: { enabled: true, count: 2 },
};
const _missingLabelParentReverse: ParentReverseProps = {
  ...valid,
  // @ts-expect-error Every layer constrains ParentReverseProps.
  details: { enabled: true, count: 2 },
};
const _missingEnabledForward: ForwardProps = {
  ...valid,
  // @ts-expect-error Every layer constrains ForwardProps.
  details: { label: 'label', count: 2 },
};
const _missingEnabledReverse: ReverseProps = {
  ...valid,
  // @ts-expect-error Every layer constrains ReverseProps.
  details: { label: 'label', count: 2 },
};
const _missingEnabledParent: ParentProps = {
  ...valid,
  // @ts-expect-error Every layer constrains ParentProps.
  details: { label: 'label', count: 2 },
};
const _missingEnabledParentReverse: ParentReverseProps = {
  ...valid,
  // @ts-expect-error Every layer constrains ParentReverseProps.
  details: { label: 'label', count: 2 },
};
const _missingCountForward: ForwardProps = {
  ...valid,
  // @ts-expect-error Every layer constrains ForwardProps.
  details: { label: 'label', enabled: true },
};
const _missingCountReverse: ReverseProps = {
  ...valid,
  // @ts-expect-error Every layer constrains ReverseProps.
  details: { label: 'label', enabled: true },
};
const _missingCountParent: ParentProps = {
  ...valid,
  // @ts-expect-error Every layer constrains ParentProps.
  details: { label: 'label', enabled: true },
};
const _missingCountParentReverse: ParentReverseProps = {
  ...valid,
  // @ts-expect-error Every layer constrains ParentReverseProps.
  details: { label: 'label', enabled: true },
};
// @ts-expect-error Every layer constrains ForwardProps.
const _missingValueForward: ForwardProps = { details: valid.details };
// @ts-expect-error Every layer constrains ReverseProps.
const _missingValueReverse: ReverseProps = { details: valid.details };
// @ts-expect-error Every layer constrains ParentProps.
const _missingValueParent: ParentProps = { details: valid.details };
// @ts-expect-error Every layer constrains ParentReverseProps.
const _missingValueParentReverse: ParentReverseProps = {
  details: valid.details,
};
// @ts-expect-error Every layer constrains ForwardProps.
const _missingDetailsForward: ForwardProps = { value: 'b' };
// @ts-expect-error Every layer constrains ReverseProps.
const _missingDetailsReverse: ReverseProps = { value: 'b' };
// @ts-expect-error Every layer constrains ParentProps.
const _missingDetailsParent: ParentProps = { value: 'b' };
// @ts-expect-error Every layer constrains ParentReverseProps.
const _missingDetailsParentReverse: ParentReverseProps = { value: 'b' };
console.log(JSON.stringify([forward, reverse, parent, parentReverse]));
