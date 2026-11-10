import type { AdjacentProps } from './catalog.js';

const valid: AdjacentProps = {
  choice: 'allowed',
  details: { label: 'label', count: 2 },
  typed: 'allowed',
  both: 'label',
  items: ['allowed'],
  objects: [{ label: 'label', count: 2 }],
  array: ['allowed'],
};
// @ts-expect-error The adjacent enum restricts the union.
const _badChoice: AdjacentProps = { ...valid, choice: 'other' };
// @ts-expect-error The adjacent enum also excludes the numeric branch.
const _badNumber: AdjacentProps = { ...valid, choice: 2 };
// @ts-expect-error The outer required property survives allOf.
const _missingLabel: AdjacentProps = { ...valid, details: { count: 2 } };
// @ts-expect-error The allOf required property is also enforced.
const _missingCount: AdjacentProps = { ...valid, details: { label: 'label' } };
// @ts-expect-error The outer property retains its type.
const _badLabel: AdjacentProps = { ...valid, details: { label: 1, count: 2 } };
// @ts-expect-error The explicit type restricts the enum.
const _badTyped: AdjacentProps = { ...valid, typed: 2 };
// @ts-expect-error Both adjacent unions constrain the value.
const _badUnion: AdjacentProps = { ...valid, both: 2 };
// @ts-expect-error The first adjacent union also constrains the value.
const _badOtherUnion: AdjacentProps = { ...valid, both: true };
// @ts-expect-error Array item schemas preserve adjacent constraints.
const _badItems: AdjacentProps = { ...valid, items: ['other'] };
// @ts-expect-error Object constraints remain required within array items.
const _badObjects: AdjacentProps = { ...valid, objects: [{ count: 2 }] };
// @ts-expect-error Adjacent array item constraints inherit the outer array type.
const _badArray: AdjacentProps = { ...valid, array: ['other'] };
console.log(JSON.stringify([valid]));
