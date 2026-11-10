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
const badChoice: RefProps = { ...valid, choice: 'c' };
// @ts-expect-error Both enums constrain the value, regardless of order.
const badReverse: RefProps = { ...valid, reverse: 'a' };
// @ts-expect-error The referenced required field remains required.
const missingLabel: RefProps = {
  ...valid,
  details: { count: 2, enabled: true },
};
// @ts-expect-error Sibling properties and requirements also apply.
const missingEnabled: RefProps = {
  ...valid,
  details: { label: 'label', count: 2 },
};
// @ts-expect-error A required-only sibling makes the referenced optional field required.
const missingCount: RefProps = { ...valid, requiredOnly: { label: 'label' } };
// @ts-expect-error The referenced property type remains enforced with required-only siblings.
const badCount: RefProps = {
  ...valid,
  requiredOnly: { label: 'label', count: '2' },
};
// @ts-expect-error Both item schemas constrain every array item.
const badArray: RefProps = { ...valid, array: ['c'] };
// @ts-expect-error Adding the array type does not discard the referenced items.
const badArrayType: RefProps = { ...valid, arrayType: [2] };
// @ts-expect-error Adding the object type does not discard referenced required fields.
const badObjectType: RefProps = { ...valid, objectType: {} };
// @ts-expect-error Both the enum and sibling type apply.
const badTyped: RefProps = { ...valid, typed: 2 };
// @ts-expect-error Both union schemas apply.
const badUnion: RefProps = { ...valid, union: true };
// @ts-expect-error Sibling component requirements are retained.
const missingComponentField: RefProps = { choice: 'b', details: valid.details };
// @ts-expect-error Adjacent enum and anyOf constraints both apply.
const badCombined: RefProps = { ...valid, combined: 'c' };
// @ts-expect-error Partial requirements also apply inside sibling allOf.
const badRequiredInAllOf: RefProps = {
  ...valid,
  requiredInAllOf: { label: 'label' },
};
console.log(JSON.stringify([valid]));
