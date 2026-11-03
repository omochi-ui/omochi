import { isValidNodeId, isValidServiceId, isValidSurfaceId } from '@omochi-ui/core/browser';
import { overlaps, type BoardExport, type Dashboard, type DashboardInput, type Panel, type SourceLabel } from './contracts.js';
import { BoardFault } from './board-repository.js';

export type { SourceLabel } from './contracts.js';
export const BOARD_FILE_LIMIT = 1024 * 1024;
const invalid = (): never => { throw new BoardFault('InvalidBoard'); };
export function boardFields(value: unknown, required: readonly string[], optional: readonly string[] = []): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value) ||
      ![Object.prototype, null].includes(Object.getPrototypeOf(value))) return invalid();
  const keys = Reflect.ownKeys(value);
  if (required.some(key => !Object.hasOwn(value, key)) || keys.some(key => typeof key !== 'string' || ![...required, ...optional].includes(key) ||
      !Object.getOwnPropertyDescriptor(value, key)?.enumerable || !Object.hasOwn(Object.getOwnPropertyDescriptor(value, key)!, 'value'))) return invalid();
  return value as Record<string, unknown>;
}
function array(value: unknown, max: number): unknown[] {
  if (!Array.isArray(value) || Object.getPrototypeOf(value) !== Array.prototype || value.length > max || Reflect.ownKeys(value).length !== value.length + 1) return invalid();
  for (let index = 0; index < value.length; index++) {
    const item = Object.getOwnPropertyDescriptor(value, String(index));
    if (!item?.enumerable || !Object.hasOwn(item, 'value')) return invalid();
  }
  return value;
}
function text(value: unknown, max: number): value is string { return typeof value === 'string' && value.length <= max && !!value.trim(); }
function integer(value: unknown, min: number, max: number): value is number { return Number.isSafeInteger(value) && Number(value) >= min && Number(value) <= max; }

/** One shared semantic validator for exchange, browser writes and HTTP responses. Returns owned data. */
export function validateBoardInput(value: unknown, trimName = false): DashboardInput {
  const input = boardFields(value, ['name', 'panels']);
  if (!text(input.name, 200)) return invalid();
  const panels: Panel[] = [], ids = new Set<string>(), references = new Set<string>();
  for (const item of array(input.panels, 200)) {
    const panel = boardFields(item, ['id', 'title', 'reference', 'x', 'y', 'w', 'h']);
    const reference = boardFields(panel.reference, ['nodeId', 'serviceId', 'surfaceId']);
    if (!text(panel.id, 100) || !text(panel.title, 200) || typeof reference.nodeId !== 'string' || !isValidNodeId(reference.nodeId) ||
        typeof reference.serviceId !== 'string' || !isValidServiceId(reference.serviceId) || typeof reference.surfaceId !== 'string' || !isValidSurfaceId(reference.surfaceId) ||
        !integer(panel.x, 0, 11) || !integer(panel.y, 0, 100000) || !integer(panel.w, 1, 12) || !integer(panel.h, 1, 1000) || panel.x + panel.w > 12) return invalid();
    const key = JSON.stringify([reference.nodeId, reference.serviceId, reference.surfaceId]);
    if (ids.has(panel.id) || references.has(key)) return invalid();
    ids.add(panel.id); references.add(key);
    const owned: Panel = { id: panel.id, title: panel.title,
      reference: { nodeId: reference.nodeId, serviceId: reference.serviceId, surfaceId: reference.surfaceId },
      x: panel.x, y: panel.y, w: panel.w, h: panel.h };
    if (panels.some(previous => overlaps(previous, owned))) return invalid();
    panels.push(owned);
  }
  return { name: trimName ? input.name.trim() : input.name, panels };
}
function source(value: unknown): SourceLabel {
  const record = boardFields(value, ['nodeId'], ['name']);
  if (typeof record.nodeId !== 'string' || !isValidNodeId(record.nodeId) || (Object.hasOwn(record, 'name') && !text(record.name, 256))) return invalid();
  return { nodeId: record.nodeId, ...(typeof record.name === 'string' ? { name: record.name } : {}) };
}
function checkDepth(text: string): void {
  let depth = 0, quoted = false, escaped = false;
  for (const char of text) {
    if (quoted) {
      if (escaped) escaped = false;
      else if (char === '\\') escaped = true;
      else if (char === '"') quoted = false;
    } else if (char === '"') quoted = true;
    else if (char === '{' || char === '[') { if (++depth > 128) invalid(); }
    else if (char === '}' || char === ']') depth--;
  }
}
export function parseBoard(bytes: Uint8Array): { input: DashboardInput; sources: SourceLabel[] } {
  if (!(bytes instanceof Uint8Array) || bytes.length > BOARD_FILE_LIMIT) return invalid();
  let parsed: unknown;
  try {
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes); checkDepth(text); parsed = JSON.parse(text);
  } catch { return invalid(); }
  const document = boardFields(parsed, ['format', 'version', 'board', 'sources']);
  if (document.format !== 'omochi.board' || document.version !== 1) return invalid();
  const input = validateBoardInput(document.board), sources = array(document.sources, 200).map(source);
  const referenced = [...new Set(input.panels.map(panel => panel.reference.nodeId))].sort();
  if (sources.length !== referenced.length || sources.some((label, index) => label.nodeId !== referenced[index])) return invalid();
  return { input, sources };
}
export function exportBoard(board: Dashboard, labels: readonly SourceLabel[]): Uint8Array<ArrayBuffer> {
  const input = validateBoardInput({ name: board.name, panels: board.panels });
  const names = new Map<string, SourceLabel>();
  for (const label of array(labels, 200).map(source)) {
    if (names.has(label.nodeId)) return invalid(); names.set(label.nodeId, label);
  }
  const sources = [...new Set(input.panels.map(panel => panel.reference.nodeId))].sort().map(nodeId => names.get(nodeId) ?? { nodeId });
  const document: BoardExport = { format: 'omochi.board', version: 1, board: input, sources };
  const bytes = new TextEncoder().encode(JSON.stringify(document, null, 2) + '\n');
  if (bytes.length > BOARD_FILE_LIMIT) return invalid();
  return bytes;
}
