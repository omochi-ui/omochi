import type { RefProps } from './catalog.js';

const valid: RefProps = {
  choice: 'b',
  reverse: 'b',
  annotated: 'label',
  details: { label: 'label', count: 2, enabled: true },
  requiredOnly: { label: 'label', count: 2 },
  array: ['b'],
  arrayType: ['label'],
  objectType: { label: 'label' },
  typed: 'b',
  union: 'label',
  combined: 'b',
  requiredInAllOf: { label: 'label', count: 2 },
};
// @ts-expect-error Both enums constrain the value.
const _badChoice: RefProps = { ...valid, choice: 'c' };
// @ts-expect-error Both enums constrain the value, regardless of order.
const _badReverse: RefProps = { ...valid, reverse: 'a' };
// @ts-expect-error The referenced required field remains required.
const _missingLabel: RefProps = {
  ...valid,
  details: { count: 2, enabled: true },
};
// @ts-expect-error Sibling properties and requirements also apply.
const _missingEnabled: RefProps = {
  ...valid,
  details: { label: 'label', count: 2 },
};
// @ts-expect-error A required-only sibling makes the referenced optional field required.
const _missingCount: RefProps = { ...valid, requiredOnly: { label: 'label' } };
// @ts-expect-error The referenced property type remains enforced with required-only siblings.
const _badCount: RefProps = {
  ...valid,
  requiredOnly: { label: 'label', count: '2' },
};
// @ts-expect-error Both item schemas constrain every array item.
const _badArray: RefProps = { ...valid, array: ['c'] };
// @ts-expect-error Adding the array type does not discard the referenced items.
const _badArrayType: RefProps = { ...valid, arrayType: [2] };
// @ts-expect-error Adding the object type does not discard referenced required fields.
const _badObjectType: RefProps = { ...valid, objectType: {} };
// @ts-expect-error Both the enum and sibling type apply.
const _badTyped: RefProps = { ...valid, typed: 2 };
// @ts-expect-error Both union schemas apply.
const _badUnion: RefProps = { ...valid, union: true };
// @ts-expect-error Sibling component requirements are retained.
const _missingComponentField: RefProps = {
  choice: 'b',
  details: valid.details,
};
// @ts-expect-error Adjacent enum and anyOf constraints both apply.
const _badCombined: RefProps = { ...valid, combined: 'c' };
// @ts-expect-error Partial requirements also apply inside sibling allOf.
const _badRequiredInAllOf: RefProps = {
  ...valid,
  requiredInAllOf: { label: 'label' },
};
console.log(JSON.stringify([valid]));
