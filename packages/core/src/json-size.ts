import { SessionFault } from './browser-session.js';
import { MAX_JSON_DEPTH, PROHIBITED_KEYS, isArrayIndex } from './data-path.js';

export interface JsonMeasure {
  readonly byteSize: number;
  readonly maxDepth: number;
}
interface Measured extends JsonMeasure {
  readonly immutable: boolean;
}
const immutableSizes = new WeakMap<object, Measured>();
const invalid = (): never => {
  throw new SessionFault('ProtocolError');
};

/** Compact JSON string bytes, without encoding or copying the entire string. */
export function jsonStringBytes(value: string): number {
  let size = 2;
  for (let i = 0; i < value.length; i++) {
    const unit = value.charCodeAt(i);
    if (
      unit === 34 ||
      unit === 92 ||
      unit === 8 ||
      unit === 9 ||
      unit === 10 ||
      unit === 12 ||
      unit === 13
    )
      size += 2;
    else if (unit < 32) size += 6;
    else if (unit < 128) size++;
    else if (unit < 2048) size += 2;
    else if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = value.charCodeAt(i + 1);
      if (next >= 0xdc00 && next <= 0xdfff) {
        size += 4;
        i++;
      } else size += 6;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) size += 6;
    else size += 3;
  }
  return size;
}

/** Validates own data properties, never invoking application getters or toJSON. */
export function jsonEntries(value: object): [string, unknown][] {
  const array = Array.isArray(value);
  if (
    !array &&
    Object.getPrototypeOf(value) !== Object.prototype &&
    Object.getPrototypeOf(value) !== null
  )
    invalid();
  const entries: [string, unknown][] = [];
  for (const key of Reflect.ownKeys(value)) {
    if (typeof key !== 'string' || PROHIBITED_KEYS.has(key)) return invalid();
    const descriptor = Object.getOwnPropertyDescriptor(value, key)!;
    if (!('value' in descriptor)) invalid();
    if (array && key === 'length') continue;
    if (
      array &&
      (!isArrayIndex(key) || Number(key) >= (value as unknown[]).length)
    )
      invalid();
    if (!descriptor.enumerable) invalid();
    entries.push([key, descriptor.value]);
  }
  return entries;
}

export function measureJson(value: unknown): JsonMeasure {
  const active = new Set<object>();
  function visit(node: unknown, depth: number): Measured {
    let byteSize: number;
    if (node === undefined) byteSize = 0;
    else if (node === null) byteSize = 4;
    else if (typeof node === 'string') byteSize = jsonStringBytes(node);
    else if (typeof node === 'boolean') byteSize = node ? 4 : 5;
    else if (typeof node === 'number' && Number.isFinite(node))
      byteSize = JSON.stringify(node).length;
    else if (typeof node === 'object') {
      if (active.has(node)) invalid();
      const cached = immutableSizes.get(node);
      if (cached) {
        if (depth + cached.maxDepth > MAX_JSON_DEPTH) invalid();
        return cached;
      }
      if (depth >= MAX_JSON_DEPTH) invalid();
      active.add(node);
      const array = Array.isArray(node);
      const entries = jsonEntries(node);
      let size = array ? 2 + (node.length ? node.length * 5 - 1 : 0) : 2;
      let count = 0;
      let maxDepth = 1;
      let immutable = Object.isFrozen(node);
      for (const [key, child] of entries) {
        const measured = visit(child, depth + 1);
        immutable &&= measured.immutable;
        if (!array && child === undefined) continue;
        maxDepth = Math.max(maxDepth, 1 + measured.maxDepth);
        if (array) size += (child === undefined ? 4 : measured.byteSize) - 4;
        else
          size +=
            (count++ ? 1 : 0) + jsonStringBytes(key) + 1 + measured.byteSize;
      }
      active.delete(node);
      const result = { byteSize: size, maxDepth, immutable };
      // Only fully immutable trees can safely outlive this traversal in the cache.
      if (immutable) immutableSizes.set(node, result);
      return result;
    } else return invalid();
    return { byteSize, maxDepth: 0, immutable: true };
  }
  return visit(value, 0);
}

export function jsonBytes(value: unknown): number {
  return measureJson(value).byteSize;
}

/** Detached JSON wire normalization, used only after the caller admits its size. */
export function materializeJson(value: unknown): unknown {
  if (value === null || typeof value !== 'object') return value;
  if (Array.isArray(value)) {
    const result: unknown[] = Array.from({ length: value.length }, () => null);
    for (const [key, child] of jsonEntries(value))
      result[Number(key)] = child === undefined ? null : materializeJson(child);
    return result;
  }
  const result: Record<string, unknown> = {};
  for (const [key, child] of jsonEntries(value)) {
    if (child !== undefined)
      Object.defineProperty(result, key, {
        value: materializeJson(child),
        enumerable: true,
        writable: true,
        configurable: true,
      });
  }
  return result;
}
