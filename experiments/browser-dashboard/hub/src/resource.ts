import { SessionFault, type SurfaceLease } from '@omochi-ui/core/browser';
import type { Connection, Stream } from '@omochi-ui/browser-transport';
import { BytePool, LIMITS, type Reservation } from './budget.js';
import type { ServiceScope } from './scopes.js';
import { validatePng } from './png.js';

export interface ResourceScope {
  readonly lease: SurfaceLease;
  readonly task: Readonly<ServiceScope>;
  readonly connection: Connection;
  /** Revalidate the original lease/task. Never return another connection or generation. */
  current(): void;
}
interface ReadOperation {
  scope: ResourceScope; caller: AbortSignal; controller: AbortController; signal: AbortSignal;
  expires: number; timer: ReturnType<typeof setTimeout>; abort(): void;
  stream: Stream | null; data: Uint8Array<ArrayBuffer> | null; charge: Reservation | null;
  response: ReadableStreamDefaultController<Uint8Array> | null; delivered: boolean; finished: boolean;
  done: Promise<void>; resolve(): void;
}
const cancelled = () => new SessionFault('Cancelled');
function fault(error: unknown): SessionFault { return error instanceof SessionFault ? error : new SessionFault('NetworkUnavailable'); }
function reply(value: unknown, op: 'stat' | 'read'): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new SessionFault('ProtocolError');
  const result = value as Record<string, unknown>;
  const base = ['omochi', 'kind', 'id', 'status'];
  const fields = result.status === 200 ? [...base, ...(op === 'stat' ? ['size', 'mime', 'etag'] : ['data'])] : base;
  if (Object.keys(result).length !== fields.length || fields.some(key => !Object.hasOwn(result, key)) ||
      result.omochi !== 'resources/1' || result.kind !== 'response' || typeof result.id !== 'string' || !/^[A-Za-z0-9_-]{1,80}$/.test(result.id) ||
      !Number.isSafeInteger(result.status) || Number(result.status) < 100 || Number(result.status) > 599) throw new SessionFault('ProtocolError');
  if (result.status !== 200) throw new SessionFault('SourceUnavailable');
  return result;
}
/** Strict padding, pad bits and exact chunk length before copying into the reserved buffer. */
function copyBase64(value: unknown, target: Uint8Array, offset: number, length: number): void {
  if (typeof value !== 'string' || value.length !== Math.ceil(length / 3) * 4 ||
      !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value)) throw new SessionFault('ProtocolError');
  let decoded: string;
  try { decoded = atob(value); } catch { throw new SessionFault('ProtocolError'); }
  if (decoded.length !== length || btoa(decoded) !== value) throw new SessionFault('ProtocolError');
  for (let index = 0; index < length; index++) target[offset + index] = decoded.charCodeAt(index);
}

/** Profile-wide admission with no wait queue. Returned bodies retain their reservation until consumed/cancelled. */
export class PngResources {
  private readonly root = new AbortController();
  private readonly reads = new Set<ReadOperation>();
  private readonly encoded = new BytePool(LIMITS.pngBytes);
  private closing: Promise<void> | null = null;
  private readsStarted = 0;
  private peakReads = 0;
  get budgetDiagnostics() {
    return { ...this.diagnostics, peakEncodedBytes: this.encoded.peakBytes, readsStarted: this.readsStarted, peakReads: this.peakReads };
  }
  get diagnostics(): Readonly<{ activeReads: number; encodedBytes: number }> {
    return { activeReads: this.reads.size, encodedBytes: this.encoded.usedBytes };
  }
  load(scope: ResourceScope, uri: string, caller: AbortSignal): Promise<Response> {
    try {
      scope.current();
      if (caller.aborted || this.root.signal.aborted) throw cancelled();
      const resource = typeof uri === 'string' ? /^omochi:\/\/resources\/([a-f0-9]{32})$/.exec(uri)?.[1] : null;
      if (!resource) throw new SessionFault('UnsupportedMedia');
      if (this.reads.size >= LIMITS.resources) throw new SessionFault('CapacityExceeded');
      const controller = new AbortController(), expires = Date.now() + 20000;
      const signal = AbortSignal.any([caller, scope.lease.signal, this.root.signal, controller.signal]);
      let resolve!: () => void;
      const done = new Promise<void>(finish => { resolve = finish; });
      const operation: ReadOperation = {
        scope, caller, controller, signal, expires, done, resolve, stream: null, data: null, charge: null,
        response: null, delivered: false, finished: false,
        timer: setTimeout(() => controller.abort(), 20000),
        abort: () => {
          void operation.stream?.cancel().catch(() => {});
          if (operation.delivered) {
            try { this.check(operation); } catch (error) { operation.response?.error(error); }
            this.finish(operation);
          }
        },
      };
      this.reads.add(operation); this.readsStarted++; this.peakReads = Math.max(this.peakReads, this.reads.size);
      signal.addEventListener('abort', operation.abort, { once: true });
      return this.read(operation, resource);
    } catch (error) { return Promise.reject(fault(error)); }
  }
  close(): Promise<void> {
    if (this.closing) return this.closing;
    const pending = [...this.reads]; this.root.abort();
    return this.closing = Promise.all(pending.map(operation => operation.done)).then(() => {});
  }
  private check(operation: ReadOperation): void {
    operation.scope.current();
    if (operation.scope.lease.signal.aborted) throw new SessionFault('ScopeExpired');
    if (operation.caller.aborted || this.root.signal.aborted) throw cancelled();
    if (operation.controller.signal.aborted || Date.now() >= operation.expires) throw new SessionFault('Timeout');
  }
  private finish(operation: ReadOperation): void {
    if (operation.finished) return;
    operation.finished = true; clearTimeout(operation.timer); operation.signal.removeEventListener('abort', operation.abort);
    operation.data = null; operation.charge?.release(); operation.charge = null;
    this.reads.delete(operation); operation.resolve();
  }
  private async exchange(operation: ReadOperation, request: Record<string, unknown>): Promise<Record<string, unknown>> {
    this.check(operation);
    const controller = new AbortController(), expires = Date.now() + 15000;
    const signal = AbortSignal.any([operation.signal, controller.signal]);
    const timer = setTimeout(() => controller.abort(), 15000);
    let stream: Stream | null = null;
    try {
      const { serviceId, generation } = operation.scope.task;
      stream = await operation.scope.connection.open({ kind: 'resource', service: serviceId, generation }, signal);
      operation.stream = stream; this.check(operation);
      await stream.send(JSON.stringify({ omochi: 'resources/1', kind: 'request', id: crypto.randomUUID(), ...request }), signal);
      this.check(operation); await stream.finish(); this.check(operation);
      const frame = await stream.receive(signal); this.check(operation);
      if (controller.signal.aborted || Date.now() >= expires) throw new SessionFault('Timeout');
      if (!frame) throw new SessionFault('SourceUnavailable');
      if (!Number.isSafeInteger(frame.wireBytes) || frame.wireBytes < 1 || frame.wireBytes > LIMITS.frame) throw new SessionFault('ProtocolError');
      let value: unknown;
      try { value = JSON.parse(frame.json); } catch { throw new SessionFault('ProtocolError'); }
      return reply(value, request.op as 'stat' | 'read');
    } catch (error) {
      this.check(operation);
      if (controller.signal.aborted || Date.now() >= expires) throw new SessionFault('Timeout');
      throw fault(error);
    } finally {
      clearTimeout(timer); await stream?.cancel(); operation.stream = null;
    }
  }
  private async read(operation: ReadOperation, resource: string): Promise<Response> {
    try {
      const metadata = await this.exchange(operation, { op: 'stat', resource }); this.check(operation);
      if (metadata.mime !== 'image/png') throw new SessionFault('UnsupportedMedia');
      if (!Number.isSafeInteger(metadata.size) || Number(metadata.size) < 33 || metadata.etag !== `"${resource}"`) throw new SessionFault('InvalidImage');
      const size = metadata.size as number;
      if (size > LIMITS.png) throw new SessionFault('CapacityExceeded');
      operation.charge = this.encoded.reserve(size);
      operation.data = new Uint8Array(size);
      for (let offset = 0; offset < size;) {
        this.check(operation);
        const length = Math.min(24576, size - offset);
        const chunk = await this.exchange(operation, { op: 'read', resource, offset, length }); this.check(operation);
        copyBase64(chunk.data, operation.data, offset, length); offset += length;
      }
      const dimensions = await validatePng(operation.data, operation.signal, () => this.check(operation)); this.check(operation);
      const body = new ReadableStream<Uint8Array>({
        start: controller => { operation.response = controller; },
        pull: controller => {
          try { this.check(operation); controller.enqueue(operation.data!); controller.close(); }
          catch (error) { controller.error(fault(error)); }
          finally { this.finish(operation); }
        },
        cancel: () => { operation.controller.abort(); this.finish(operation); },
      }, { highWaterMark: 0 });
      operation.delivered = true;
      return new Response(body, { headers: {
        'content-type': 'image/png', 'content-length': String(size),
        'x-omochi-image-width': String(dimensions.width), 'x-omochi-image-height': String(dimensions.height),
      } });
    } catch (error) {
      this.finish(operation); throw fault(error);
    }
  }
}
