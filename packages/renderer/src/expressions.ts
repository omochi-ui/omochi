import { createFunctionImplementation, isSignal, peekValue, type DataContext } from '@a2ui/web_core/v0_9';
import { z } from 'zod';

/** Resolve expression syntax once; values obtained from the model are already data. */
export function resolveInput(value: unknown, context: DataContext, depth = 0): unknown {
  if (depth > 100) throw new Error('action input exceeds maximum expression depth');
  if (!value || typeof value !== 'object') return value;
  if (Array.isArray(value)) return value.map(item => resolveInput(item, context, depth + 1));
  if ('path' in value) {
    if (typeof value.path !== 'string') throw new Error('invalid input reference');
    const resolved: unknown = context.resolveDynamicValue({ path: value.path });
    if (resolved === undefined) throw new Error(`missing input reference: ${value.path}`);
    return resolved;
  }
  if ('call' in value) {
    if (value.call === 'omochiMergeObjects') {
      const args = (value as { args?: { objects?: unknown } }).args;
      return mergeInputs(args?.objects, context, depth + 1);
    }
    const call = value as { call: unknown; args?: unknown; returnType?: unknown };
    if (typeof call.call !== 'string' || !call.args || typeof call.args !== 'object' || Array.isArray(call.args)) throw new Error('invalid input function');
    const args = Object.fromEntries(Object.entries(call.args).map(([key, item]) => [key, resolveInput(item, context, depth + 1)]));
    // Invoking directly avoids interpreting path/call properties in resolved data a second time.
    const result: unknown = context.functionInvoker(call.call, args, context, new AbortController().signal);
    const resolved = isSignal(result) ? peekValue(result) : result;
    if (resolved === undefined) throw new Error('input function failed');
    return resolved;
  }
  return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, resolveInput(item, context, depth + 1)]));
}
function mergeInputs(objects: unknown, context: DataContext, depth = 0): Record<string, unknown> {
  if (!Array.isArray(objects)) throw new Error('mergeObjects expects an objects array');
  const entries = new Map<string, unknown>();
  for (const expression of objects) {
    const object = resolveInput(expression, context, depth + 1);
    if (!object || typeof object !== 'object' || Array.isArray(object)) throw new Error('mergeObjects expects JSON objects');
    for (const [key, value] of Object.entries(object)) {
      if (entries.has(key)) throw new Error(`duplicate input key: ${key}`);
      entries.set(key, value);
    }
  }
  return Object.fromEntries(entries);
}
export const mergeObjectsFunction = createFunctionImplementation({
  name: 'omochiMergeObjects', returnType: 'object', schema: z.object({ objects: z.array(z.unknown()) }),
}, ({ objects }, context) => mergeInputs(objects, context));

export function assertResolvedInput(value: unknown): void {
  if (value === undefined) throw new Error('action input could not be resolved');
  if (value && typeof value === 'object') for (const item of Object.values(value)) assertResolvedInput(item);
}
