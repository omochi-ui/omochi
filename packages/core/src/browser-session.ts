import type { A2uiClientMessage, A2uiMessage } from '@a2ui/web_core/v0_9';

export type Unsubscribe = () => void;
export type ServiceKey = string;
export type LeaseId = string;
export type ImagePolicy = 'native' | 'sdk-png' | 'hub';
export type ClientMessage = A2uiClientMessage & {
  metadata?: { a2uiClientDataModel?: unknown };
};
export interface ServiceView {
  id: ServiceKey;
  name: string;
  node: { id: string; name: string; local: boolean };
  status: 'connected' | 'disconnected' | 'error';
  detail?: string;
}
export interface SurfaceLease {
  readonly id: LeaseId;
  readonly surfaceId: string;
  readonly signal: AbortSignal;
}
export type SessionErrorCode =
  | 'UnsupportedBrowser' | 'StorageUnavailable' | 'StorageCorrupt' | 'StorageVersion'
  | 'OwnerBusy' | 'InvalidInvitation' | 'EnrollmentRejected' | 'EnrollmentUnknown'
  | 'EnrollmentUnsaved' | 'NotAuthorized' | 'NetworkUnavailable' | 'ProtocolError'
  | 'CapacityExceeded' | 'SourceUnavailable' | 'ScopeExpired' | 'UnsupportedMedia'
  | 'InvalidImage' | 'Timeout' | 'Cancelled' | 'InvalidState' | 'RenderError';
export interface SessionError { code: SessionErrorCode; message: string }

const errorMessages: Record<SessionErrorCode, string> = {
  UnsupportedBrowser: 'This browser does not support direct connections.',
  StorageUnavailable: 'Browser storage is unavailable. Please retry.',
  StorageCorrupt: 'The saved browser profile is invalid.',
  StorageVersion: 'The saved browser profile version is unsupported.',
  OwnerBusy: 'Another tab owns this browser connection.',
  InvalidInvitation: 'This invitation is invalid or expired.',
  EnrollmentRejected: 'The gateway rejected enrollment.',
  EnrollmentUnknown: 'Enrollment could not be confirmed. Retry with the same identity.',
  EnrollmentUnsaved: 'Enrollment succeeded but could not be saved. Retry saving.',
  NotAuthorized: 'This connection is not authorized.',
  NetworkUnavailable: 'The source connection is unavailable.',
  ProtocolError: 'The source sent invalid protocol data.',
  CapacityExceeded: 'Browser capacity exceeded. Release a source or retry.',
  SourceUnavailable: 'This source is unavailable.',
  ScopeExpired: 'This view is no longer connected.',
  UnsupportedMedia: 'This media is unavailable in direct mode.',
  InvalidImage: 'The image is invalid.',
  Timeout: 'The source operation timed out.',
  Cancelled: 'The source operation was cancelled.',
  InvalidState: 'This operation is unavailable in the current state.',
  RenderError: 'This source could not be rendered.',
};

/** Fixed public text: never propagate source payloads or credentials into errors. */
export class SessionFault extends Error implements SessionError {
  constructor(readonly code: SessionErrorCode) {
    super(errorMessages[code]);
    this.name = 'SessionFault';
  }
}

export type LiveEvent =
  | { kind: 'status'; sourceId: string; state: string; error?: SessionError }
  | { kind: 'images'; enabled: boolean }
  | { kind: 'services'; services: readonly ServiceView[] };
export interface RenderBudget { reserve(bytes: number): Unsubscribe }
export type ApplyResult = { ok: true } | { ok: false; code: 'RenderError' | 'CapacityExceeded' };
export type RenderEvent =
  | { kind: 'commit'; service: ServiceKey; epoch: number;
      messages: readonly A2uiMessage[]; leases: readonly SurfaceLease[] }
  | { kind: 'update'; service: ServiceKey; epoch: number; message: A2uiMessage;
      lease?: SurfaceLease; budget?: RenderBudget }
  | { kind: 'remove'; service: ServiceKey; epoch: number; reason: string };
export interface LiveSession {
  readonly imagePolicy: ImagePolicy;
  subscribe(listener: (event: LiveEvent) => void): Unsubscribe;
  attachRenderer(apply: (event: RenderEvent) => ApplyResult): void;
  start(): Promise<void>;
  setServices(ordered: readonly ServiceKey[]): void;
  retry(target: { gatewayId: string } | { service: ServiceKey }): Promise<void>;
  action(lease: LeaseId, message: ClientMessage): Promise<void>;
  image(lease: LeaseId, uri: string, signal: AbortSignal): Promise<Response>;
  stop(): Promise<void>;
}

/** A promise/throw/partial response must never make an epoch action-ready. */
export function applyToRenderer(apply: (event: RenderEvent) => ApplyResult, event: RenderEvent): ApplyResult {
  try {
    const result: unknown = apply(event);
    if (result instanceof Promise) { void result.catch(() => {}); return { ok: false, code: 'RenderError' }; }
    if (!result || typeof result !== 'object' || Array.isArray(result)) throw new SessionFault('RenderError');
    if (Object.getPrototypeOf(result) !== Object.prototype && Object.getPrototypeOf(result) !== null) throw new SessionFault('RenderError');
    const record = result as Record<string, unknown>;
    const keys = Reflect.ownKeys(record);
    if (record.ok === true && keys.length === 1 && keys[0] === 'ok') return { ok: true };
    if (record.ok === false && keys.length === 2 && Object.hasOwn(record, 'ok') && Object.hasOwn(record, 'code') &&
        (record.code === 'RenderError' || record.code === 'CapacityExceeded')) return { ok: false, code: record.code };
  } catch { /* Fixed result, never expose processor payloads/errors. */ }
  return { ok: false, code: 'RenderError' };
}
