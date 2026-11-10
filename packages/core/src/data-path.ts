import { SessionFault } from './browser-session.js';

export const MAX_JSON_DEPTH = 128;
export const MAX_PATH_INDICES = 100000;
export const PROHIBITED_KEYS: ReadonlySet<string> = new Set([
  '__proto__',
  'prototype',
  'constructor',
]);
export const isArrayIndex = (part: string): boolean =>
  /^(0|[1-9]\d*)$/.test(part);

/** Preserve empty pointer segments; only the whole '' and '/' denote the root. */
export function dataPath(path: string): readonly string[] {
  if (path === '' || path === '/') return [];
  if (typeof path !== 'string' || !path.startsWith('/'))
    throw new SessionFault('ProtocolError');
  const parts = path
    .slice(1)
    .split('/')
    .map((part) => part.replaceAll('~1', '/').replaceAll('~0', '~'));
  if (parts.length > MAX_JSON_DEPTH) throw new SessionFault('ProtocolError');
  let indices = 0;
  for (const part of parts) {
    if (PROHIBITED_KEYS.has(part)) throw new SessionFault('ProtocolError');
    if (isArrayIndex(part)) indices += Number(part);
    if (indices > MAX_PATH_INDICES) throw new SessionFault('ProtocolError');
  }
  return parts;
}
