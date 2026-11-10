import type { PickerProps } from './catalog.js';

const valid: PickerProps = {
  choices: ['a', 'b'],
  mixed: ['a', 1],
  flags: [true, 1],
  nested: [['a', 'b', 1], []],
  children: [['child'], { componentId: 'child', path: '/items' }],
  records: [{ left: 'a', right: 1 }],
};
const empty: PickerProps = {
  choices: [],
  mixed: [],
  flags: [],
  nested: [],
  children: [],
  records: [],
};

// @ts-expect-error An enum array must reject a scalar enum member.
const scalarChoice: PickerProps['choices'] = 'a';
// @ts-expect-error Every enum array element must belong to the enum.
const invalidChoice: PickerProps['choices'] = ['c'];
// @ts-expect-error An anyOf array must reject scalar alternatives.
const scalarMixed: PickerProps['mixed'] = 'a';
// @ts-expect-error A oneOf array must reject scalar alternatives.
const scalarFlag: PickerProps['flags'] = true;
// @ts-expect-error Nested arrays must reject a scalar outer value.
const scalarNested: PickerProps['nested'] = 'a';
// @ts-expect-error Nested arrays must reject scalar inner values.
const flatNested: PickerProps['nested'] = ['a'];
// @ts-expect-error Nested arrays still restrict the innermost elements.
const invalidNested: PickerProps['nested'] = [['c']];
// @ts-expect-error Symbolic ChildList unions also require an outer array.
const flatChildren: PickerProps['children'] = ['child'];
// @ts-expect-error Every intersection array element must contain both fields.
const invalidRecord: PickerProps['records'] = [{ left: 'a' }];

void [
  valid,
  empty,
  scalarChoice,
  invalidChoice,
  scalarMixed,
  scalarFlag,
  scalarNested,
  flatNested,
  invalidNested,
  flatChildren,
  invalidRecord,
];
