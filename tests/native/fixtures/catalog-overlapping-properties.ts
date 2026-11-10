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
// @ts-expect-error Every layer constrains ForwardProps.
const _missingLabelForward: ForwardProps = {
  ...valid,
  details: { enabled: true, count: 2 },
};
// @ts-expect-error Every layer constrains ReverseProps.
const _missingLabelReverse: ReverseProps = {
  ...valid,
  details: { enabled: true, count: 2 },
};
// @ts-expect-error Every layer constrains ParentProps.
const _missingLabelParent: ParentProps = {
  ...valid,
  details: { enabled: true, count: 2 },
};
// @ts-expect-error Every layer constrains ParentReverseProps.
const _missingLabelParentReverse: ParentReverseProps = {
  ...valid,
  details: { enabled: true, count: 2 },
};
// @ts-expect-error Every layer constrains ForwardProps.
const _missingEnabledForward: ForwardProps = {
  ...valid,
  details: { label: 'label', count: 2 },
};
// @ts-expect-error Every layer constrains ReverseProps.
const _missingEnabledReverse: ReverseProps = {
  ...valid,
  details: { label: 'label', count: 2 },
};
// @ts-expect-error Every layer constrains ParentProps.
const _missingEnabledParent: ParentProps = {
  ...valid,
  details: { label: 'label', count: 2 },
};
// @ts-expect-error Every layer constrains ParentReverseProps.
const _missingEnabledParentReverse: ParentReverseProps = {
  ...valid,
  details: { label: 'label', count: 2 },
};
// @ts-expect-error Every layer constrains ForwardProps.
const _missingCountForward: ForwardProps = {
  ...valid,
  details: { label: 'label', enabled: true },
};
// @ts-expect-error Every layer constrains ReverseProps.
const _missingCountReverse: ReverseProps = {
  ...valid,
  details: { label: 'label', enabled: true },
};
// @ts-expect-error Every layer constrains ParentProps.
const _missingCountParent: ParentProps = {
  ...valid,
  details: { label: 'label', enabled: true },
};
// @ts-expect-error Every layer constrains ParentReverseProps.
const _missingCountParentReverse: ParentReverseProps = {
  ...valid,
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
