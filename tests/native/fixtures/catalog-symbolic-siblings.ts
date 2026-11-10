import type { SymbolicProps } from './catalog.js';

const valid: SymbolicProps = {
  label: 'allowed',
  plain: { path: '/label' },
  typed: 'any string',
  binding: { path: '/allowed' },
  items: ['allowed'],
  alias: 'allowed',
  nested: [['allowed']],
  list: ['allowed'],
};
const plainString: SymbolicProps = { ...valid, plain: 'any string' };
const plainFunction: SymbolicProps = {
  ...valid,
  plain: { call: 'label', args: {}, returnType: 'string' },
};
// @ts-expect-error A symbolic reference does not discard the adjacent enum.
const _badLabel: SymbolicProps = { ...valid, label: 'rejected' };
// @ts-expect-error The adjacent string type excludes dynamic binding objects.
const _badLabelBinding: SymbolicProps = { ...valid, label: { path: '/label' } };
// @ts-expect-error A type sibling also applies without an enum.
const _badTyped: SymbolicProps = { ...valid, typed: { path: '/label' } };
// @ts-expect-error Object property constraints narrow the referenced binding.
const _badBinding: SymbolicProps = { ...valid, binding: { path: '/other' } };
// @ts-expect-error Symbolic sibling constraints apply to array items.
const _badItems: SymbolicProps = { ...valid, items: ['rejected'] };
// @ts-expect-error Local references to common types preserve sibling constraints.
const _badAlias: SymbolicProps = { ...valid, alias: 'rejected' };
// @ts-expect-error Local references retain symbolic constraints in nested arrays.
const _badNested: SymbolicProps = { ...valid, nested: [[{ path: '/label' }]] };
// @ts-expect-error An array sibling restricts the items of a dynamic string list.
const _badList: SymbolicProps = { ...valid, list: ['rejected'] };
// @ts-expect-error An array sibling excludes dynamic list bindings.
const _badListBinding: SymbolicProps = { ...valid, list: { path: '/list' } };
void [valid, plainString, plainFunction];
