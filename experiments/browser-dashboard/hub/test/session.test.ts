import assert from 'node:assert/strict';
import { test } from '@omochi-ui/core/testing';
import { applyToRenderer, BASIC_CATALOG_ID, SessionFault, type ApplyResult, type LiveEvent,
  type RenderEvent, type ClientMessage, type SurfaceLease } from '@omochi-ui/core/browser';
import type { BrowserTransport, Connection, Endpoint, OpenHeader, ReceivedFrame, Stream } from '@omochi-ui/browser-transport';
import { createBrowserSession, type ProfileSnapshot } from '../src/session.js';
import type { TestContext } from '@omochi-ui/core/testing';
import { jest } from 'bun:test';
import { SurfaceScopes, sameServiceScope, type ServiceScope } from '../src/scopes.js';

const task: ServiceScope = { gatewayId: 'a'.repeat(64), gatewayEpoch: 1, serviceId: 'app', generation: '18446744073709551615', serviceInstance: 1 };
const surface = (scope: ServiceScope, id = 'main') => `${scope.gatewayId}~${scope.serviceId}/${id}`;

test('immutable leases capture exact epochs/generations/instances and same-ID recreation gets a fresh ID', () => {
  const scopes = new SurfaceScopes();
  const mutable = { ...task };
  const first = scopes.create(mutable, surface(task));
  mutable.generation = '1';
  assert.equal(first.task.generation, task.generation);
  assert.equal(Object.isFrozen(first.lease), true);
  assert.equal(Object.isFrozen(first.task), true);
  for (const replacement of [{ ...task, gatewayEpoch: 2 }, { ...task, generation: '1' }, { ...task, serviceInstance: 2 }]) {
    assert.throws(() => scopes.require(first.lease.id, captured => sameServiceScope(captured, replacement)), { code: 'ScopeExpired' });
  }
  let expiredDuringAbort = false;
  first.lease.signal.addEventListener('abort', () => {
    try { scopes.require(first.lease.id, () => true); } catch { expiredDuringAbort = true; }
  });
  scopes.remove(first.lease.id);
  assert.equal(first.lease.signal.aborted, true); assert.equal(expiredDuringAbort, true);
  const recreated = scopes.create(task, surface(task));
  assert.notEqual(recreated.lease.id, first.lease.id);
  scopes.remove(first.lease.id);
  assert.equal(recreated.lease.signal.aborted, false);
  scopes.close(); scopes.close();
  assert.equal(scopes.retainedBytes, 0); assert.equal(scopes.size, 0);
});

test('revoking one task leaves another gateway and replacement service instance live', () => {
  const scopes = new SurfaceScopes();
  const other = { ...task, gatewayId: 'b'.repeat(64) };
  const replacement = { ...task, serviceInstance: 2 };
  const a = scopes.create(task, surface(task));
  const b = scopes.create(other, surface(other));
  const next = scopes.create(replacement, surface(replacement));
  scopes.removeTask(task);
  assert.equal(a.lease.signal.aborted, true);
  assert.equal(b.lease.signal.aborted, false);
  assert.equal(next.lease.signal.aborted, false);
  assert.equal(scopes.require(b.lease.id, () => true).task.gatewayId, other.gatewayId);
  scopes.close(); assert.equal(scopes.retainedBytes, 0);
});

test('lease-count and retained-ID capacity reject without displacing existing scopes', () => {
  const scopes = new SurfaceScopes();
  for (let i = 0; i < 4096; i++) scopes.create(task, surface(task, `s${i}`));
  assert.throws(() => scopes.create(task, surface(task, 'extra')), { code: 'CapacityExceeded' });
  assert.equal(scopes.size, 4096);
  scopes.close(); assert.equal(scopes.retainedBytes, 0);
  const long = 'x'.repeat(4096);
  const bytes = new TextEncoder().encode(surface(task, long)).length;
  while (scopes.retainedBytes + bytes <= 8 * 1024 * 1024) scopes.create(task, surface(task, long));
  const count = scopes.size;
  assert.throws(() => scopes.create(task, surface(task, long)), { code: 'CapacityExceeded' });
  assert.equal(scopes.size, count);
  scopes.close(); assert.equal(scopes.retainedBytes, 0);
});

test('only exact synchronous authoritative apply results are accepted', () => {
  const event = { kind: 'remove' as const, service: 'app', epoch: 1, reason: 'test' };
  for (const result of [undefined, null, {}, true, Promise.resolve({ ok: true }), { ok: true, extra: 1 }, { ok: false }, { ok: false, code: 'other' }]) {
    assert.deepEqual(applyToRenderer(() => result as ApplyResult, event), { ok: false, code: 'RenderError' });
  }
  const disguised = Promise.resolve(); Object.assign(disguised, { ok: true });
  assert.deepEqual(applyToRenderer(() => disguised as unknown as ApplyResult, event), { ok: false, code: 'RenderError' });
  assert.deepEqual(applyToRenderer(() => { throw new Error('untrusted processor detail'); }, event), { ok: false, code: 'RenderError' });
  assert.deepEqual(applyToRenderer(() => ({ ok: true }), event), { ok: true });
  assert.deepEqual(applyToRenderer(() => ({ ok: false, code: 'CapacityExceeded' }), event), { ok: false, code: 'CapacityExceeded' });
});

function facilities(t: TestContext) {
  const values = {
    navigator: { userAgent: 'Mozilla/5.0 (X11; Linux x86_64) Chrome/156.0.0.0', locks: { request() {} } },
    isSecureContext: true, indexedDB: {}, createImageBitmap: async () => ({}),
  };
  for (const [key, value] of Object.entries(values)) {
    const previous = Object.getOwnPropertyDescriptor(globalThis, key);
    Object.defineProperty(globalThis, key, { configurable: true, value });
    t.after(() => { if (previous) Object.defineProperty(globalThis, key, previous); else Reflect.deleteProperty(globalThis, key); });
  }
}
const A = 'a'.repeat(64), B = 'b'.repeat(64), identityId = 'c'.repeat(64);
const key = (id: string, service = 'app') => `${id}~${service}`;
const action = (surfaceId: string, model: Record<string, unknown> = {}): ClientMessage => ({
  version: 'v0.9', action: { name: 'save', surfaceId, sourceComponentId: 'root', timestamp: '2026-11-03T00:00:00Z', context: {} },
  metadata: { a2uiClientDataModel: { version: 'v0.9', surfaces: model } },
});
async function until(condition: () => boolean) {
  for (let i = 0; i < 1000; i++) { if (condition()) return; await Bun.sleep(1); }
  assert.fail('condition did not become true');
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(yes => { resolve = yes; });
  return { promise, resolve };
}
class WireStream implements Stream {
  private readonly incoming: (ReceivedFrame | null | SessionFault)[] = [];
  private waiting: { resolve(value: ReceivedFrame | null): void; reject(error: unknown): void } | null = null;
  cancelled = false;
  reads = 0;
  cancels = 0;
  readonly sent: unknown[] = [];
  writeGate: ReturnType<typeof deferred<void>> | null = null;
  constructor(private readonly onSend?: (value: unknown) => void) {}
  push(value: unknown, bytes?: number) {
    const json = JSON.stringify(value);
    this.feed({ json, wireBytes: bytes ?? new TextEncoder().encode(json).length });
  }
  feed(value: ReceivedFrame | null | SessionFault) {
    if (this.waiting) {
      const waiting = this.waiting; this.waiting = null;
      if (value instanceof SessionFault) waiting.reject(value); else waiting.resolve(value);
    } else this.incoming.push(value);
  }
  async send(json: string, signal: AbortSignal) {
    if (signal.aborted || this.cancelled) throw new SessionFault('Cancelled');
    this.sent.push(JSON.parse(json));
    if (this.writeGate) await new Promise<void>((resolve, reject) => {
      const abort = () => { signal.removeEventListener('abort', abort); reject(new SessionFault('Cancelled')); };
      signal.addEventListener('abort', abort, { once: true });
      void this.writeGate!.promise.then(() => { signal.removeEventListener('abort', abort); resolve(); });
    });
    this.onSend?.(JSON.parse(json));
  }
  receive(signal: AbortSignal): Promise<ReceivedFrame | null> {
    this.reads++;
    if (signal.aborted || this.cancelled) return Promise.reject(new SessionFault('Cancelled'));
    if (this.waiting) return Promise.reject(new SessionFault('ProtocolError'));
    if (this.incoming.length) {
      const next = this.incoming.shift()!;
      return next instanceof SessionFault ? Promise.reject(next) : Promise.resolve(next);
    }
    return new Promise((resolve, reject) => {
      const abort = () => { this.waiting = null; signal.removeEventListener('abort', abort); reject(new SessionFault('Cancelled')); };
      signal.addEventListener('abort', abort, { once: true });
      this.waiting = {
        resolve: value => { signal.removeEventListener('abort', abort); resolve(value); },
        reject: error => { signal.removeEventListener('abort', abort); reject(error); },
      };
    });
  }
  async finish() {}
  async cancel() {
    if (this.cancelled) return;
    this.cancelled = true; this.cancels++;
    this.waiting?.reject(new SessionFault('Cancelled')); this.waiting = null;
  }
}
class WireGateway {
  readonly connections: WireConnection[] = [];
  records = [{ id: 'app', name: 'App', generation: '18446744073709551615' }];
  mode: 'complete' | 'silent' | 'partial' = 'complete';
  helloId: string;
  pong = true;
  constructor(readonly id: string) { this.helloId = id; }
  get latest() { return this.connections.at(-1)!; }
}
class WireConnection implements Connection {
  readonly remoteId: string;
  readonly headers: OpenHeader[] = [];
  readonly services = new Map<string, WireStream[]>();
  control: WireStream | null = null;
  private readonly ended = deferred<{ code: string; reason: string }>();
  closes = 0;
  constructor(readonly gateway: WireGateway) { this.remoteId = gateway.id; }
  async open(header: OpenHeader, signal: AbortSignal) {
    if (signal.aborted) throw new SessionFault('Cancelled');
    this.headers.push(header);
    if (header.kind === 'control') {
      const control = new WireStream(value => {
        const frame = value as { kind: string; id: number };
        if (frame.kind === 'ping' && this.gateway.pong) control.push({ kind: 'pong', id: String(frame.id) });
      });
      this.control = control;
      control.push({ kind: 'hello', protocol: 4, nodeId: this.gateway.helloId, name: 'Gateway' });
      control.push({ kind: 'services', services: this.gateway.records });
      return control;
    }
    if (header.kind !== 'service') throw new SessionFault('ProtocolError');
    const stream = new WireStream();
    const streams = this.services.get(header.service) ?? [];
    streams.push(stream); this.services.set(header.service, streams);
    if (this.gateway.mode !== 'silent') {
      stream.push({ kind: 'snapshotStart' });
      stream.push({ kind: 'message', message: { version: 'v0.9', createSurface: { surfaceId: 'main/nested', catalogId: BASIC_CATALOG_ID, sendDataModel: true } } });
      stream.push({ kind: 'message', message: { version: 'v0.9', updateDataModel: { surfaceId: 'main/nested', value: { server: 1 } } } });
      if (this.gateway.mode === 'complete') stream.push({ kind: 'snapshotEnd' });
    }
    return stream;
  }
  closed() { return this.ended.promise; }
  lose(code: string) {
    this.ended.resolve({ code, reason: 'raw untrusted reason' });
    void this.control?.cancel();
    for (const streams of this.services.values()) for (const stream of streams) void stream.cancel();
  }
  async close() { this.closes++; this.lose('0'); }
  service(id = 'app') { return this.services.get(id)?.at(-1); }
}
function setup(t: TestContext, ids = [A, B]) {
  facilities(t);
  const servers = new Map(ids.map(id => [id, new WireGateway(id)]));
  const opened: readonly string[][] = [];
  let endpoints = 0, closes = 0, verified = '';
  const endpoint: Endpoint = {
    async connect(json, alpn, signal) {
      assert.equal(alpn, 'omochi/gateway/4');
      if (signal.aborted) throw new SessionFault('Cancelled');
      const server = servers.get(JSON.parse(json).id)!;
      const connection = new WireConnection(server); server.connections.push(connection); return connection;
    },
    async close() { closes++; for (const server of servers.values()) for (const connection of server.connections) await connection.close(); },
  };
  const transport: BrowserTransport = {
    async generateIdentity() { return { secretHex: '0'.repeat(64), endpointId: identityId }; },
    async identityFromSecret(secretHex) { verified = secretHex; return { secretHex, endpointId: identityId }; },
    async openEndpoint(_secret, relays) { endpoints++; (opened as string[][]).push([...relays]); return endpoint; },
  };
  const profile: ProfileSnapshot = { identity: { secretHex: '0'.repeat(64), endpointId: identityId },
    gateways: ids.map((nodeId, index) => ({ nodeId, name: `Gateway ${index}`, relayUrls: [`https://relay${index}.example/`], enrollment: 'accepted', createdAt: 0 })) };
  const source = createBrowserSession(profile, transport);
  t.after(() => source.stop());
  const events: LiveEvent[] = [];
  const renders: RenderEvent[] = [];
  const leases = new Map<string, SurfaceLease>();
  let sink: (event: RenderEvent) => ApplyResult = () => ({ ok: true });
  source.attachRenderer(event => {
    if (event.kind === 'commit') {
      for (const lease of event.leases) leases.set(event.service, lease);
      renders.push({ ...event, messages: structuredClone(event.messages) });
    } else renders.push(event);
    return sink(event);
  });
  source.subscribe(event => events.push(event));
  return { source, servers, transport, profile, opened, events, renders, leases, setSink: (apply: typeof sink) => { sink = apply; },
    get endpoints() { return endpoints; }, get closes() { return closes; }, get verified() { return verified; } };
}
const ready = (f: ReturnType<typeof setup>, gateway: string, service = 'app') => f.leases.get(key(gateway, service));

test('two selected sources authenticate, commit atomically and route full form metadata to one captured service', async t => {
  const f = setup(t);
  f.source.subscribe(() => { throw new Error('independent observer failure'); });
  f.profile.identity.secretHex = 'changed after capture';
  f.profile.gateways[0]!.name = 'Changed after capture';
  f.source.setServices([key(A), key(B)]);
  let early: Promise<unknown> | undefined;
  f.setSink(event => {
    if (event.kind === 'commit' && event.service === key(A)) early = f.source.action(event.leases[0]!.id, action(event.leases[0]!.surfaceId)).catch(error => error.code);
    return { ok: true };
  });
  const started = f.source.start(); assert.equal(f.source.start(), started); await started;
  await until(() => !!ready(f, A) && !!ready(f, B));
  assert.equal(await early, 'ScopeExpired');
  assert.equal(f.endpoints, 1);
  assert.equal(f.verified, '0'.repeat(64));
  const roster = f.events.filter(event => event.kind === 'services').at(-1)!;
  assert(roster.kind === 'services'); assert.equal(roster.services[0]!.node.name, 'Gateway', 'live names come from authenticated hello, not saved labels');
  assert.deepEqual(f.opened, [['https://relay0.example/', 'https://relay1.example/']]);
  const a = ready(f, A)!, b = ready(f, B)!;
  assert.equal(a.surfaceId, `${key(A)}/main/nested`);
  assert.equal(b.surfaceId, `${key(B)}/main/nested`);
  assert.equal(f.renders.filter(event => event.kind === 'commit').length, 2);
  const openedService = f.servers.get(A)!.latest.headers.find(header => header.kind === 'service')!;
  assert('generation' in openedService);
  assert.equal(openedService.generation, '18446744073709551615');
  await f.source.action(a.id, action(a.surfaceId, { [a.surfaceId]: { draft: 'unfinished' }, [b.surfaceId]: { private: 'other gateway' } }));
  const sent = f.servers.get(A)!.latest.service()!.sent[0] as ClientMessage;
  assert('action' in sent);
  assert.equal(sent.action.surfaceId, 'main/nested');
  assert.deepEqual(sent.metadata, { a2uiClientDataModel: { version: 'v0.9', surfaces: { 'main/nested': { draft: 'unfinished' } } } });
  assert.equal(f.servers.get(B)!.latest.service()!.sent.length, 0);
  await assert.rejects(f.source.action(a.id, action(b.surfaceId)), { code: 'ScopeExpired' });
  await f.source.stop();
  assert.equal(a.signal.aborted, true); assert.equal(b.signal.aborted, true); assert.equal(f.closes, 1);
  await assert.rejects(f.source.start(), { code: 'InvalidState' });
});

test('generation replacement and same-ID recreation expire old leases without touching another gateway', async t => {
  const f = setup(t); f.source.setServices([key(A), key(B)]); await f.source.start();
  await until(() => !!ready(f, A) && !!ready(f, B));
  const old = ready(f, A)!, other = ready(f, B)!;
  const server = f.servers.get(A)!;
  server.records = [{ id: 'app', name: 'App', generation: '1' }];
  server.latest.control!.push({ kind: 'services', services: server.records });
  await until(() => ready(f, A)!.id !== old.id);
  assert.equal(old.signal.aborted, true); assert.equal(other.signal.aborted, false);
  await assert.rejects(f.source.action(old.id, action(old.surfaceId)), { code: 'ScopeExpired' });
  const current = ready(f, A)!;
  const stream = server.latest.service()!;
  stream.push({ kind: 'message', message: { version: 'v0.9', deleteSurface: { surfaceId: 'main/nested' } } });
  stream.push({ kind: 'message', message: { version: 'v0.9', createSurface: { surfaceId: 'main/nested', catalogId: BASIC_CATALOG_ID } } });
  await until(() => f.renders.some(event => event.kind === 'update' && 'createSurface' in event.message));
  assert.equal(current.signal.aborted, true);
  const recreated = f.renders.findLast(event => event.kind === 'update' && 'createSurface' in event.message)!;
  assert.equal(recreated.kind, 'update');
  if (recreated.kind === 'update') assert.notEqual(recreated.lease!.id, current.id);
  await f.source.action(other.id, action(other.surfaceId));
});

test('scoped Retry coalesces and rejoins only its gateway/service, retaining the shared endpoint and B lease', async t => {
  const f = setup(t); f.source.setServices([key(A), key(B)]); await f.source.start();
  await until(() => !!ready(f, A) && !!ready(f, B));
  const old = ready(f, A)!, other = ready(f, B)!;
  const retry = f.source.retry({ gatewayId: A });
  assert.equal(f.source.retry({ gatewayId: A }), retry); await retry;
  await until(() => ready(f, A)!.id !== old.id);
  assert.equal(f.servers.get(A)!.connections.length, 2); assert.equal(f.servers.get(B)!.connections.length, 1);
  assert.equal(f.endpoints, 1); assert.equal(other.signal.aborted, false);
  const previous = ready(f, A)!;
  await f.source.retry({ service: key(A) });
  await until(() => ready(f, A)!.id !== previous.id);
  assert.equal(f.servers.get(A)!.connections.length, 2); assert.equal(other.signal.aborted, false);
  await assert.rejects(f.source.retry({ gatewayId: 'd'.repeat(64) }), { code: 'SourceUnavailable' });
  await assert.rejects(f.source.retry({ service: key(A, 'missing') }), { code: 'SourceUnavailable' });
  await assert.rejects(f.source.retry({ gatewayId: 'invalid' }), { code: 'ProtocolError' });
});

test('terminal protocol/render errors retract only their service and explicit Retry is required', async t => {
  const f = setup(t); f.source.setServices([key(A), key(B)]);
  f.setSink(event => event.kind === 'commit' && event.service === key(A) ? { ok: false, code: 'RenderError' } : { ok: true });
  await f.source.start(); await until(() => !!ready(f, A) && !!ready(f, B));
  await until(() => f.events.some(event => event.kind === 'services' && event.services.some(service => service.id === key(A) && service.status === 'error')));
  const a = ready(f, A)!, b = ready(f, B)!;
  assert.equal(a.signal.aborted, true); assert.equal(b.signal.aborted, false);
  assert(f.renders.some(event => event.kind === 'remove' && event.service === key(A)));
  f.source.setServices([key(A), key(B)]);
  await Bun.sleep(5);
  assert.equal(f.servers.get(A)!.latest.services.get('app')!.length, 1);
  f.setSink(() => ({ ok: true })); await f.source.retry({ service: key(A) });
  await until(() => ready(f, A)!.id !== a.id);
  await f.source.action(ready(f, A)!.id, action(ready(f, A)!.surfaceId));
});

test('service slots include unavailable sources; removing/readding is fresh and invalid routes are synchronous errors', async t => {
  const f = setup(t, [A]);
  f.servers.get(A)!.records = Array.from({ length: 9 }, (_, i) => ({ id: `s${i}`, name: `Service ${i}`, generation: '1' }));
  const desired = Array.from({ length: 9 }, (_, i) => key(A, `s${i}`));
  f.source.setServices([key(B, 'unavailable'), ...desired]); await f.source.start();
  await until(() => f.renders.filter(event => event.kind === 'commit').length === 7);
  assert.equal(f.servers.get(A)!.latest.headers.filter(header => header.kind === 'service').length, 7);
  const roster = f.events.findLast(event => event.kind === 'services')!;
  if (roster.kind === 'services') assert.equal(roster.services.find(service => service.id === desired[7])!.detail, 'Service limit');
  assert.throws(() => f.source.setServices(['legacy~app']), { code: 'ProtocolError' });
  const before = ready(f, A, 's0')!;
  f.source.setServices(desired.slice(1));
  await until(() => f.servers.get(A)!.latest.headers.filter(header => header.kind === 'service').length === 9);
  assert.equal(before.signal.aborted, true);
  f.source.setServices([desired[0]!]);
  await until(() => ready(f, A, 's0')!.id !== before.id);
});

test('revocation cancels in-flight/queued actions and never dispatches them through another source', async t => {
  const f = setup(t); f.source.setServices([key(A), key(B)]); await f.source.start();
  await until(() => !!ready(f, A) && !!ready(f, B));
  const a = ready(f, A)!, b = ready(f, B)!;
  const stream = f.servers.get(A)!.latest.service()!; stream.writeGate = deferred<void>();
  const pending = Array.from({ length: 8 }, () => f.source.action(a.id, action(a.surfaceId)).then(() => 'sent', error => error.code));
  await assert.rejects(f.source.action(a.id, action(a.surfaceId)), { code: 'CapacityExceeded' });
  await until(() => stream.sent.length === 1);
  f.servers.get(A)!.latest.lose('1');
  const results = await Promise.all(pending);
  assert(results.every(result => result !== 'sent'));
  assert.equal(stream.sent.length, 1); assert.equal(a.signal.aborted, true); assert.equal(b.signal.aborted, false);
  await until(() => f.events.some(event => event.kind === 'status' && event.sourceId === A && event.error?.code === 'NotAuthorized'));
  await f.source.action(b.id, action(b.surfaceId));
  assert.equal(f.servers.get(B)!.latest.service()!.sent.length, 1);
});

test('start enforces facilities/sink once, profiles are detached, and empty selections create no endpoint', async t => {
  facilities(t);
  let opened = 0;
  const transport: BrowserTransport = {
    async generateIdentity() { return { secretHex: '0'.repeat(64), endpointId: identityId }; },
    async identityFromSecret(secretHex) { return { secretHex, endpointId: identityId }; },
    async openEndpoint(): Promise<Endpoint> { opened++; throw new Error('not expected'); },
  };
  const source = createBrowserSession({ identity: { secretHex: '0'.repeat(64), endpointId: identityId }, gateways: [] }, transport);
  await assert.rejects(source.start(), { code: 'InvalidState' });
  source.attachRenderer(() => ({ ok: true }));
  assert.throws(() => source.attachRenderer(() => ({ ok: true })), { code: 'InvalidState' });
  await source.start(); assert.equal(opened, 0); await source.stop();
  const invalid = createBrowserSession({ identity: { secretHex: '0'.repeat(64), endpointId: identityId }, gateways: [] }, transport);
  invalid.attachRenderer(() => ({ ok: true }));
  Object.defineProperty(globalThis, 'isSecureContext', { configurable: true, value: false });
  await assert.rejects(invalid.start(), { code: 'UnsupportedBrowser' }); await invalid.stop();
});

async function flushClock(ms = 0) {
  jest.advanceTimersByTime(ms);
  for (let pass = 0; pass < 8; pass++) {
    for (let i = 0; i < 30; i++) await Promise.resolve();
    jest.advanceTimersByTime(0);
  }
}

test('silent first snapshot, stalled snapshot and partial-frame timeouts affect only their service while control pongs remain healthy', async t => {
  jest.useFakeTimers(); t.after(() => { jest.useRealTimers(); });
  const f = setup(t); f.source.setServices([key(A), key(B)]);
  f.servers.get(A)!.mode = 'silent';
  await f.source.start(); await flushClock();
  assert(ready(f, B));
  const other = ready(f, B)!;
  const first = f.servers.get(A)!.latest.service()!;
  assert.equal(f.renders.filter(event => event.kind === 'commit').length, 1, 'staging is invisible until snapshotEnd');
  await flushClock(10000);
  assert.equal(first.cancelled, true);
  assert.equal(other.signal.aborted, false);
  assert(f.events.some(event => event.kind === 'services' && event.services.some(service => service.id === key(A) && service.detail?.includes('timed out'))));
  f.servers.get(A)!.mode = 'partial';
  await f.source.retry({ service: key(A) }); await flushClock();
  const partial = f.servers.get(A)!.latest.service()!;
  // Matching heartbeats continue across the 30-second snapshot deadline.
  await flushClock(10000); await flushClock(10000); await flushClock(10000);
  assert.equal(partial.cancelled, true);
  assert.equal(other.signal.aborted, false);
  f.servers.get(A)!.mode = 'complete';
  await f.source.retry({ service: key(A) }); await flushClock();
  const current = ready(f, A)!;
  f.servers.get(A)!.latest.service()!.feed(new SessionFault('Timeout'));
  await flushClock();
  assert.equal(current.signal.aborted, true);
  assert.equal(other.signal.aborted, false);
  assert.equal(f.servers.get(A)!.connections.length, 1, 'service timeout does not reconnect control');
});

test('quiet ready streams survive beyond 30 seconds and heartbeat failure/backoff retract only their gateway', async t => {
  jest.useFakeTimers(); t.after(() => { jest.useRealTimers(); });
  const f = setup(t); f.source.setServices([key(A), key(B)]); await f.source.start(); await flushClock();
  const a = ready(f, A)!, b = ready(f, B)!;
  for (let second = 0; second < 4; second++) await flushClock(10000);
  assert.equal(a.signal.aborted, false); assert.equal(b.signal.aborted, false);
  f.servers.get(A)!.latest.service()!.push({ kind: 'message', message: { version: 'v0.9', updateDataModel: { surfaceId: 'main/nested', path: '/server', value: 2 } } });
  await flushClock();
  assert(f.renders.some(event => event.kind === 'update' && event.service === key(A)));
  f.servers.get(A)!.pong = false;
  await flushClock(10000); await flushClock(10000);
  assert.equal(a.signal.aborted, true); assert.equal(b.signal.aborted, false);
  assert(f.events.some(event => event.kind === 'status' && event.sourceId === A && event.error?.code === 'Timeout'));
  f.servers.get(A)!.pong = true;
  await f.source.retry({ gatewayId: A }); await flushClock();
  assert.notEqual(ready(f, A)!.id, a.id); assert.equal(b.signal.aborted, false);
});

test('render queue overflow allows one fresh snapshot, then becomes terminal; explicit Retry clears it', async t => {
  jest.useFakeTimers(); t.after(() => { jest.useRealTimers(); });
  const f = setup(t, [A]); f.source.setServices([key(A)]); await f.source.start(); await flushClock();
  const flood = () => {
    const stream = f.servers.get(A)!.latest.service()!;
    for (let i = 0; i < 33; i++) stream.push({ kind: 'message', message: { version: 'v0.9', updateDataModel: { surfaceId: 'main/nested', path: '/server', value: i } } });
  };
  const first = ready(f, A)!;
  flood();
  // Let the reader admit its burst before advancing the render macrotask.
  for (let i = 0; i < 250; i++) await Promise.resolve();
  assert.equal(first.signal.aborted, true);
  await flushClock(1000);
  assert.notEqual(ready(f, A)!.id, first.id);
  const second = ready(f, A)!;
  flood(); for (let i = 0; i < 250; i++) await Promise.resolve(); await flushClock();
  assert.equal(second.signal.aborted, true);
  const opens = f.servers.get(A)!.latest.headers.length;
  await flushClock(1000);
  assert.equal(f.servers.get(A)!.latest.headers.length, opens);
  await f.source.retry({ service: key(A) }); await flushClock();
  assert.notEqual(ready(f, A)!.id, second.id);
});

test('all retained snapshot, delivery, queue, action and lease charges reach zero on stop', async t => {
  const f = setup(t); f.source.setServices([key(A), key(B)]); await f.source.start();
  await until(() => !!ready(f, A) && !!ready(f, B));
  const a = ready(f, A)!;
  const stream = f.servers.get(A)!.latest.service()!; stream.writeGate = deferred<void>();
  const actionResult = f.source.action(a.id, action(a.surfaceId)).catch(error => error.code);
  stream.push({ kind: 'message', message: { version: 'v0.9', updateDataModel: { surfaceId: 'main/nested', path: '/queued', value: 'not delivered' } } });
  await until(() => stream.sent.length === 1);
  await f.source.stop(); await actionResult;
  // Inspect only counters, not the manager's private identity/profile values.
  const counters = f.source as unknown as { snapshots: { usedBytes: number }; delivery: { usedBytes: number };
    renderBytes: { usedBytes: number }; actionBytes: { usedBytes: number }; scopes: { size: number; retainedBytes: number }; renderCount: number; actionCount: number };
  assert.deepEqual([counters.snapshots.usedBytes, counters.delivery.usedBytes, counters.renderBytes.usedBytes, counters.actionBytes.usedBytes,
    counters.scopes.size, counters.scopes.retainedBytes, counters.renderCount, counters.actionCount], Array(8).fill(0));
});

test('action and render queues enforce their shared byte budgets independently of message counts', async t => {
  const f = setup(t, [A]); f.source.setServices([key(A)]); await f.source.start();
  await until(() => !!ready(f, A));
  const lease = ready(f, A)!;
  const stream = f.servers.get(A)!.latest.service()!; stream.writeGate = deferred<void>();
  const large = action(lease.surfaceId); assert('action' in large);
  large.action.context.payload = 'x'.repeat(3 * 1024 * 1024);
  const first = f.source.action(lease.id, large).catch(error => error.code);
  const second = f.source.action(lease.id, large).catch(error => error.code);
  await assert.rejects(f.source.action(lease.id, large), { code: 'CapacityExceeded' });
  // Only two messages consume >6 MiB; the count limit of eight is not involved.
  await f.source.stop(); await Promise.all([first, second]);

  const other = setup(t, [B]); other.source.setServices([key(B)]); await other.source.start();
  await until(() => !!ready(other, B));
  const before = ready(other, B)!;
  const input = other.servers.get(B)!.latest.service()!;
  for (let i = 0; i < 5; i++) input.push({ kind: 'message', message: { version: 'v0.9', updateDataModel: { surfaceId: 'main/nested', path: '/server', value: i } } }, 4 * 1024 * 1024);
  for (let i = 0; i < 200; i++) await Promise.resolve();
  assert.equal(before.signal.aborted, true, 'fifth pending 4 MiB wire frame exceeds the 16 MiB queue');
});
