import { A2uiClientMessageSchema, A2uiMessageSchema, type A2uiMessage } from '@a2ui/web_core/v0_9';
import { SessionFault, type ClientMessage } from './browser-session.js';
import { dataPath } from './data-path.js';
import { jsonBytes, jsonEntries, measureJson } from './json-size.js';
import { isValidNodeId, isValidServiceId, isValidSurfaceId } from './service-id.js';

export const BASIC_CATALOG_ID = 'https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json';
export const MAX_U64_DECIMAL = '18446744073709551615';
export interface GatewayService { id: string; name: string; generation: string; icon?: string }
export type ControlFrame =
  | { kind: 'hello'; protocol: 4; nodeId: string; name: string }
  | { kind: 'services'; services: GatewayService[] }
  | { kind: 'pong'; id: string }
  | { kind: 'bye'; reason: string };
export type ServiceFrame = { kind: 'snapshotStart' | 'snapshotEnd' } | { kind: 'message'; message: A2uiMessage };

export function isU64Decimal(value: unknown, nonzero = true): value is string {
  return typeof value === 'string' && /^(0|[1-9]\d*)$/.test(value) &&
    (!nonzero || value !== '0') && (value.length < 20 || (value.length === 20 && value <= MAX_U64_DECIMAL));
}
function invalid(): never { throw new SessionFault('ProtocolError'); }
function record(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return invalid();
  return value as Record<string, unknown>;
}
function fields(value: Record<string, unknown>, required: readonly string[], optional: readonly string[] = []): void {
  if (required.some(key => !Object.hasOwn(value, key)) || Object.keys(value).some(key => !required.includes(key) && !optional.includes(key))) invalid();
}
function text(value: unknown): value is string { return typeof value === 'string' && value.trim().length > 0; }
function wireJson(value: unknown): void {
  measureJson(value);
  function visit(node: unknown): void {
    if (node === undefined) invalid();
    if (node !== null && typeof node === 'object') {
      if (Array.isArray(node) && Object.keys(node).length !== node.length) invalid();
      for (const [, child] of jsonEntries(node)) visit(child);
    }
  }
  visit(value);
}

/** Validate the normalized WASM DTO, never parse native u64 JSON numbers in JS. */
export function parseControlFrame(value: unknown): ControlFrame {
  wireJson(value);
  const frame = record(value);
  switch (frame.kind) {
    case 'hello':
      fields(frame, ['kind', 'protocol', 'nodeId', 'name']);
      if (frame.protocol !== 4 || typeof frame.nodeId !== 'string' || !isValidNodeId(frame.nodeId) || !text(frame.name)) invalid();
      break;
    case 'services': {
      fields(frame, ['kind', 'services']);
      if (!Array.isArray(frame.services) || frame.services.length > 256) return invalid();
      const ids = new Set<string>();
      for (const value of frame.services) {
        const service = record(value);
        fields(service, ['id', 'name', 'generation'], ['icon']);
        if (typeof service.id !== 'string' || !isValidServiceId(service.id) || ids.has(service.id) ||
            !text(service.name) || !isU64Decimal(service.generation) ||
            (Object.hasOwn(service, 'icon') && typeof service.icon !== 'string')) return invalid();
        ids.add(service.id);
      }
      // The bridge adds two quotes per generation; account native encoded metadata.
      if (jsonBytes(frame) - 2 * frame.services.length > 1024 * 1024) throw new SessionFault('CapacityExceeded');
      break;
    }
    case 'pong':
      fields(frame, ['kind', 'id']);
      if (!isU64Decimal(frame.id, false)) invalid();
      break;
    case 'bye':
      fields(frame, ['kind', 'reason']);
      if (typeof frame.reason !== 'string') invalid();
      break;
    default: return invalid();
  }
  return frame as unknown as ControlFrame;
}

export function validateServerMessage(value: unknown): A2uiMessage {
  wireJson(value);
  const frame = record(value);
  const operations = ['createSurface', 'updateComponents', 'updateDataModel', 'deleteSurface'];
  if (frame.version !== 'v0.9' || operations.filter(key => Object.hasOwn(frame, key)).length !== 1) return invalid();
  // Validate sparse paths before the schema or processor can materialize data.
  if (Object.hasOwn(frame, 'updateDataModel')) {
    const update = record(frame.updateDataModel);
    if (Object.hasOwn(update, 'path') && typeof update.path !== 'string') return invalid();
    dataPath((update.path as string | undefined) ?? '/');
  }
  const parsed = A2uiMessageSchema.safeParse(value);
  if (!parsed.success) return invalid();
  const message = parsed.data;
  const operation = record(frame[operations.find(key => Object.hasOwn(frame, key))!]);
  if (typeof operation.surfaceId !== 'string' || !isValidSurfaceId(operation.surfaceId)) return invalid();
  if ('createSurface' in message && message.createSurface.catalogId !== BASIC_CATALOG_ID) return invalid();
  return message;
}

export function parseServiceFrame(value: unknown): ServiceFrame {
  wireJson(value);
  const frame = record(value);
  if (frame.kind === 'snapshotStart' || frame.kind === 'snapshotEnd') {
    fields(frame, ['kind']);
    return { kind: frame.kind };
  }
  if (frame.kind !== 'message') return invalid();
  fields(frame, ['kind', 'message']);
  return { kind: 'message', message: validateServerMessage(frame.message) };
}

/** The schema discards metadata; validate with it but retain the raw form model. */
export function validateClientMessage(value: unknown): ClientMessage {
  // The renderer may have missing roots/undefined form entries; normalize at write.
  measureJson(value);
  const frame = record(value);
  if (frame.version !== 'v0.9' || Number(Object.hasOwn(frame, 'action')) + Number(Object.hasOwn(frame, 'error')) !== 1 ||
      !A2uiClientMessageSchema.safeParse(value).success) return invalid();
  const body = record(frame.action ?? frame.error);
  if (typeof body.surfaceId !== 'string' || !body.surfaceId.trim()) return invalid();
  if (Object.hasOwn(frame, 'metadata')) {
    const metadata = record(frame.metadata);
    fields(metadata, [], ['a2uiClientDataModel']);
  }
  return value as ClientMessage;
}
