import type { NamedProps } from './catalog.js';

const props: NamedProps = {
  sourceURL: 'source',
  data_key: 'data',
  type: 'type',
  match: 'match',
  self: 'self',
  crate: 'crate',
  super: 'super',
  gen: 'gen',
};
const optional: NamedProps = { ...props, optionalURL: 'optional' };
// @ts-expect-error The original acronym spelling is required.
const renamed: NamedProps['sourceUrl'] = 'source';
// @ts-expect-error Underscores are part of the wire name.
const camelized: NamedProps['dataKey'] = 'data';
console.log(JSON.stringify([props, optional]));
void [renamed, camelized];
