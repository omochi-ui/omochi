import { SessionFault, isValidNodeId, type SessionErrorCode } from '@omochi-ui/core/browser';
import type { RawConnection, RawEndpoint, RawStream, WasmModule } from './ffi.js';

export interface BrowserIdentity { secretHex: string; endpointId: string }
export type OpenHeader = { kind: 'control'; protocol: 4 } |
  { kind: 'service' | 'resource'; service: string; generation: string } | { kind: 'pair' };
export interface ReceivedFrame { json: string; wireBytes: number }
export interface Stream {
  send(json: string, signal: AbortSignal): Promise<void>;
  receive(signal: AbortSignal): Promise<ReceivedFrame | null>;
  finish(): Promise<void>;
  cancel(): Promise<void>;
}
export interface Connection {
  readonly remoteId: string;
  open(header: OpenHeader, signal: AbortSignal): Promise<Stream>;
  closed(): Promise<{ code: string; reason: string }>;
  close(): Promise<void>;
}
export interface Endpoint {
  connect(addressJson: string, alpn: 'omochi/pair/1' | 'omochi/gateway/4', signal: AbortSignal): Promise<Connection>;
  close(): Promise<void>;
}
export interface BrowserTransport {
  generateIdentity(): Promise<BrowserIdentity>;
  identityFromSecret(secretHex: string): Promise<BrowserIdentity>;
  openEndpoint(secretHex: string, relays: readonly string[]): Promise<Endpoint>;
}
const codes = new Set<SessionErrorCode>(['ProtocolError', 'CapacityExceeded', 'Cancelled', 'Timeout', 'StorageCorrupt', 'NetworkUnavailable']);
function fault(error: unknown): SessionFault {
  if (error instanceof SessionFault) return error;
  return new SessionFault(typeof error === 'string' && codes.has(error as SessionErrorCode) ? error as SessionErrorCode : 'NetworkUnavailable');
}
function parsed(json: string): Record<string, unknown> {
  try {
    const value: unknown = JSON.parse(json);
    if (value && typeof value === 'object' && !Array.isArray(value)) return value as Record<string, unknown>;
  } catch { /* Never expose raw WASM text. */ }
  throw new SessionFault('ProtocolError');
}
function identity(json: string): BrowserIdentity {
  const value = parsed(json);
  if (Object.keys(value).length !== 2 || typeof value.secretHex !== 'string' || !isValidNodeId(value.secretHex) ||
      typeof value.endpointId !== 'string' || !isValidNodeId(value.endpointId)) throw new SessionFault('StorageCorrupt');
  return { secretHex: value.secretHex, endpointId: value.endpointId };
}
function track<T>(set: Set<Promise<unknown>>, promise: Promise<T>): Promise<T> {
  set.add(promise);
  // Use both branches instead of an unobserved rejecting finally promise.
  void promise.then(() => set.delete(promise), () => set.delete(promise));
  return promise;
}

class OwnedStream implements Stream {
  private dead = false;
  private sending = false;
  private receiving = false;
  private finished = false;
  private readonly pending = new Set<Promise<unknown>>();
  private resetting: Promise<void> | null = null;
  private closing: Promise<void> | null = null;
  constructor(private readonly raw: RawStream, private readonly removed: () => void) {}
  send(json: string, signal: AbortSignal): Promise<void> {
    if (this.dead || this.finished) return Promise.reject(new SessionFault('Cancelled'));
    if (this.sending) return Promise.reject(new SessionFault('ProtocolError'));
    this.sending = true;
    return track(this.pending, this.run(signal, () => this.raw.send(json)).finally(() => { this.sending = false; }));
  }
  receive(signal: AbortSignal): Promise<ReceivedFrame | null> {
    if (this.dead) return Promise.reject(new SessionFault('Cancelled'));
    if (this.receiving) return Promise.reject(new SessionFault('ProtocolError'));
    this.receiving = true;
    const operation = this.run(signal, async () => {
      const received = await this.raw.receive();
      if (received === null) return null;
      const value = parsed(received);
      if (Object.keys(value).length !== 2 || typeof value.json !== 'string' || !Number.isSafeInteger(value.wireBytes) ||
          Number(value.wireBytes) < 1 || Number(value.wireBytes) > 4 * 1024 * 1024) throw new SessionFault('ProtocolError');
      return { json: value.json, wireBytes: value.wireBytes as number };
    }).finally(() => { this.receiving = false; });
    return track(this.pending, operation);
  }
  finish(): Promise<void> {
    if (this.dead) return Promise.reject(new SessionFault('Cancelled'));
    if (this.sending) return Promise.reject(new SessionFault('ProtocolError'));
    if (this.finished) return Promise.resolve();
    this.finished = true;
    return track(this.pending, (async () => {
      try { await this.raw.finish(); }
      catch (error) { await this.reset(); throw fault(error); }
    })());
  }
  cancel(): Promise<void> {
    if (this.closing) return this.closing;
    this.dead = true;
    this.closing = (async () => {
      await this.reset();
      await Promise.allSettled([...this.pending]);
      this.raw.free(); this.removed();
    })();
    return this.closing;
  }
  private reset(): Promise<void> {
    this.dead = true;
    if (!this.resetting) this.resetting = Promise.resolve(this.raw.cancel());
    return this.resetting;
  }
  private async run<T>(signal: AbortSignal, begin: () => Promise<T>): Promise<T> {
    const abort = () => { void this.reset().catch(() => {}); };
    signal.addEventListener('abort', abort, { once: true });
    try {
      if (signal.aborted) { await this.reset(); throw new SessionFault('Cancelled'); }
      const value = await begin();
      if (signal.aborted || this.dead) throw new SessionFault('Cancelled');
      return value;
    } catch (error) {
      await this.reset();
      throw signal.aborted ? new SessionFault('Cancelled') : fault(error);
    } finally { signal.removeEventListener('abort', abort); }
  }
}

class OwnedConnection implements Connection {
  readonly remoteId: string;
  private dead = false;
  private sequence = 0;
  private readonly streams = new Set<OwnedStream>();
  private readonly opening = new Map<number, Promise<unknown>>();
  private readonly ended: Promise<{ code: string; reason: string }>;
  private closing: Promise<void> | null = null;
  constructor(private readonly raw: RawConnection, private readonly removed: () => void) {
    this.remoteId = raw.remote_id();
    this.ended = raw.closed().then(json => {
      const value = parsed(json);
      if (typeof value.code !== 'string' || !/^(0|[1-9]\d*)$/.test(value.code)) throw new SessionFault('ProtocolError');
      return { code: value.code, reason: value.code === '1' ? 'Not authorized' : 'Connection closed' };
    });
    void this.ended.catch(() => {});
  }
  open(header: OpenHeader, signal: AbortSignal): Promise<Stream> {
    if (this.dead || signal.aborted) return Promise.reject(new SessionFault('Cancelled'));
    if (this.sequence === 0xffffffff) return Promise.reject(new SessionFault('CapacityExceeded'));
    const operation = ++this.sequence;
    const abort = () => this.raw.cancel_open(operation);
    signal.addEventListener('abort', abort, { once: true });
    const pending = (async () => {
      try {
        const raw = await this.raw.open(JSON.stringify(header), operation);
        const stream = new OwnedStream(raw, () => this.streams.delete(stream));
        this.streams.add(stream);
        if (this.dead || signal.aborted) { await stream.cancel(); throw new SessionFault('Cancelled'); }
        return stream;
      } catch (error) { throw signal.aborted ? new SessionFault('Cancelled') : fault(error); }
      finally { signal.removeEventListener('abort', abort); this.opening.delete(operation); }
    })();
    this.opening.set(operation, pending);
    void pending.then(() => this.opening.delete(operation), () => this.opening.delete(operation));
    // A synchronous mock rejection also has a rejection observer until its caller awaits.
    void pending.catch(() => {});
    return pending;
  }
  closed(): Promise<{ code: string; reason: string }> { return this.ended; }
  close(): Promise<void> {
    if (this.closing) return this.closing;
    this.dead = true;
    this.raw.close();
    for (const operation of this.opening.keys()) this.raw.cancel_open(operation);
    this.closing = (async () => {
      await Promise.all([...this.streams].map(stream => stream.cancel()));
      await Promise.allSettled([...this.opening.values()]);
      await this.ended.catch(() => {});
      this.raw.free(); this.removed();
    })();
    return this.closing;
  }
}

const frameReaders = new WeakMap<Endpoint, () => number>();
/** Numeric ownership accounting only; never returns endpoint handles or identity material. */
export function endpointFrameBytes(endpoint: Endpoint): number { return frameReaders.get(endpoint)?.() ?? 0; }

class OwnedEndpoint implements Endpoint {
  private dead = false;
  private sequence = 0;
  private readonly connections = new Set<OwnedConnection>();
  private readonly connecting = new Map<number, Promise<unknown>>();
  private closing: Promise<void> | null = null;
  constructor(private readonly raw: RawEndpoint) { frameReaders.set(this, () => raw.frame_bytes()); }
  connect(addressJson: string, alpn: 'omochi/pair/1' | 'omochi/gateway/4', signal: AbortSignal): Promise<Connection> {
    if (this.dead || signal.aborted) return Promise.reject(new SessionFault('Cancelled'));
    if (this.sequence === 0xffffffff) return Promise.reject(new SessionFault('CapacityExceeded'));
    const operation = ++this.sequence;
    const abort = () => this.raw.cancel_connect(operation);
    signal.addEventListener('abort', abort, { once: true });
    const pending = (async () => {
      try {
        const raw = await this.raw.connect(addressJson, alpn, operation);
        const connection = new OwnedConnection(raw, () => this.connections.delete(connection));
        this.connections.add(connection);
        if (this.dead || signal.aborted) { await connection.close(); throw new SessionFault('Cancelled'); }
        if (!isValidNodeId(connection.remoteId)) { await connection.close(); throw new SessionFault('ProtocolError'); }
        return connection;
      } catch (error) { throw signal.aborted ? new SessionFault('Cancelled') : fault(error); }
      finally { signal.removeEventListener('abort', abort); this.connecting.delete(operation); }
    })();
    this.connecting.set(operation, pending);
    void pending.then(() => this.connecting.delete(operation), () => this.connecting.delete(operation));
    void pending.catch(() => {});
    return pending;
  }
  close(): Promise<void> {
    if (this.closing) return this.closing;
    this.dead = true;
    for (const operation of this.connecting.keys()) this.raw.cancel_connect(operation);
    this.closing = (async () => {
      await Promise.all([...this.connections].map(connection => connection.close()));
      await Promise.allSettled([...this.connecting.values()]);
      await this.raw.close();
      frameReaders.delete(this);
      this.raw.free();
    })();
    return this.closing;
  }
}

/** Injectable FFI seam for lifecycle tests; production uses loadBrowserTransport. */
export function createTransportFacade(module: WasmModule): BrowserTransport {
  return {
    async generateIdentity() { try { return identity(module.generate_identity()); } catch (error) { throw fault(error); } },
    async identityFromSecret(secretHex) {
      if (!isValidNodeId(secretHex)) throw new SessionFault('StorageCorrupt');
      try {
        const derived = identity(module.identity_from_secret(secretHex));
        if (derived.secretHex !== secretHex) throw new SessionFault('StorageCorrupt');
        return derived;
      } catch (error) { throw fault(error); }
    },
    async openEndpoint(secretHex, relays) {
      if (!isValidNodeId(secretHex)) throw new SessionFault('StorageCorrupt');
      try { return new OwnedEndpoint(await module.RawEndpoint.create(secretHex, JSON.stringify(relays))); }
      catch (error) { throw fault(error); }
    },
  };
}

const modules = new Map<string, Promise<BrowserTransport>>();
export function loadBrowserTransport(assetBaseUrl: string | URL): Promise<BrowserTransport> {
  const base = new URL(assetBaseUrl, globalThis.location?.href);
  if (!base.pathname.endsWith('/')) base.pathname += '/';
  const key = base.href;
  let loaded = modules.get(key);
  if (!loaded) {
    loaded = (async () => {
      const module = await import(/* @vite-ignore */ new URL('iroh.js', base).href) as WasmModule;
      await module.default({ module_or_path: new URL('iroh_bg.wasm', base) });
      return createTransportFacade(module);
    })().catch(() => { modules.delete(key); throw new SessionFault('UnsupportedBrowser'); });
    modules.set(key, loaded);
  }
  return loaded;
}
