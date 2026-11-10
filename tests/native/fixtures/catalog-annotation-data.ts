import type { AnnotationProps } from './catalog.js';

const props: AnnotationProps = {
  value: { $ref: 'literal.json' },
  default: 'label',
  examples: ['example'],
};
// @ts-expect-error Properties named default still resolve their string schema.
const wrongDefault: AnnotationProps = { ...props, default: 42 };
// @ts-expect-error Items under properties named examples still resolve their schema.
const wrongExamples: AnnotationProps = { ...props, examples: [42] };
void [props, wrongDefault, wrongExamples];
