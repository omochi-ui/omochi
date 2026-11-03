import assert from 'node:assert/strict';
import { test, waitFor } from '@omochi-ui/core/testing';
import { BASIC_CATALOG_ID, SessionFault, type RenderEvent } from '@omochi-ui/core/browser';
import { createA2uiSession } from '@omochi-ui/renderer';
import type { BrowserTransport, Stream, Connection, Endpoint, ReceivedFrame } from '@omochi-ui/hub-transport';
import { RemoteHubSession, type SharedView } from '../src/remote/session.js';
import { SharedBoardRepository } from '../src/remote/boards.js';
import { parseShareLink } from '../src/remote/link.js';
import type { Dashboard } from '../src/contracts.js';

const hubId = 'a'.repeat(64), grantId = '11111111-1111-4111-8111-111111111111', surface = `${hubId}~app/main`;
const board = (): Dashboard => ({ id: 'b', revision: 1, name: 'Shared', panels: [{ id: 'panel', title: 'View', x: 0, y: 0, w: 4, h: 4, reference: { nodeId: hubId, serviceId: 'app', surfaceId: 'main' } }] });
class FakeStream implements Stream {
  readonly sent: Record<string, unknown>[] = [];
  private readonly queue: ReceivedFrame[] = [];
  private wake: (() => void) | null = null;
  dead = false;
  onSend: (value: Record<string, unknown>) => void = () => {};
  push(value: unknown) { const json = JSON.stringify(value); this.queue.push({ json, wireBytes: new TextEncoder().encode(json).length }); this.wake?.(); }
  async send(json: string, signal: AbortSignal) { if (signal.aborted || this.dead) throw new SessionFault('Cancelled'); const value = JSON.parse(json) as Record<string, unknown>; this.sent.push(value); this.onSend(value); }
  async receive(signal: AbortSignal): Promise<ReceivedFrame | null> {
    if (!this.queue.length && !this.dead && !signal.aborted) await new Promise<void>(resolve => { const wake = () => { signal.removeEventListener('abort', wake); this.wake = null; resolve(); }; this.wake = wake; signal.addEventListener('abort', wake, { once: true }); });
    if (signal.aborted || this.dead) throw new SessionFault('Cancelled'); return this.queue.shift() ?? null;
  }
  async finish() {}
  async cancel() { this.dead = true; this.wake?.(); }
}
class FakeConnection implements Connection {
  readonly remoteId = hubId;
  readonly control = new FakeStream();
  readonly requests: Record<string, unknown>[] = [];
  readonly streams: FakeStream[] = [];
  private finish!: (value: { code: string; reason: string }) => void;
  private readonly end = new Promise<{ code: string; reason: string }>(resolve => { this.finish = resolve; });
  response: (request: Record<string, unknown>, stream: FakeStream) => void = (_request, stream) => stream.push({ kind: 'ok' });
  constructor(public view: SharedView, messages: unknown[] = []) {
    this.control.onSend = value => {
      if (value.version !== 1) return;
      this.control.push({ kind: 'welcome', ...view });
      messages.forEach((message, i) => this.control.push({ kind: 'message', message, lease: 'lease1', sequence: String(i + 1) }));
      this.control.push({ kind: 'snapshotEnd', sequence: String(messages.length + 1) });
    };
  }
  async open(signal: AbortSignal) {
    signal.throwIfAborted();
    if (!this.streams.length) { this.streams.push(this.control); return this.control; }
    const stream = new FakeStream(); this.streams.push(stream);
    stream.onSend = value => { this.requests.push(value); this.response(value, stream); }; return stream;
  }
  denyOnHello() { this.control.onSend = () => { this.finish({ code: '1', reason: 'Not authorized' }); throw new SessionFault('NetworkUnavailable'); }; }
  closed() { return this.end; }
  async close() { this.finish({ code: '2', reason: '' }); await Promise.all(this.streams.map(s => s.cancel())); }
}
function setup(interact = false, editLayout = false) {
  const create = { version: 'v0.9', createSurface: { surfaceId: surface, catalogId: BASIC_CATALOG_ID } };
  const connection = new FakeConnection({ board: board(), permissions: { interact, editLayout } }, [create]);
  const connections = [connection]; let opened = 0, active = 0;
  const transport: BrowserTransport = {
    async generateIdentity() { return { secretHex: '0'.repeat(64), endpointId: 'b'.repeat(64) }; },
    async identityFromSecret(secretHex) { return { secretHex, endpointId: 'b'.repeat(64) }; },
    async openEndpoint() {
      active++; let closed = false;
      const endpoint: Endpoint = { async connect() { const conn = connections[opened++]; if (!conn) throw new SessionFault('NetworkUnavailable'); return conn; }, async close() { if (!closed) { active--; closed = true; } await Promise.all(connections.map(c => c.close())); } }; return endpoint;
    },
  };
  let view: SharedView | null = null;
  const repo = new SharedBoardRepository();
  const source = new RemoteHubSession({ source: { hubId, grantId, relayUrls: ['https://relay.example/'] }, secretHex: '0'.repeat(64), onView(value) { view = value; repo.publish(value); } }, transport);
  repo.bind(source);
  return { source, connection, connections, repo, get view() { return view; }, get active() { return active; } };
}

test('shared sessions commit before acknowledgement, expose only the board, and reject readonly operations', async t => {
  const f = setup(); t.after(() => f.source.stop()); const events: RenderEvent[] = [];
  f.source.attachRenderer(event => { events.push(event); if (event.kind === 'commit') assert(!f.connection.control.sent.some(v => v.ack === '2')); return { ok: true }; });
  await f.source.start(); await waitFor(() => !!f.view);
  assert.equal(events.filter(e => e.kind === 'commit').length, 1);
  assert.deepEqual(await f.repo.list(), [board()]);
  await assert.rejects(f.source.action('lease1', { version: 'v0.9', action: { surfaceId: surface, name: 'click', context: {}, sourceComponentId: 'button', timestamp: new Date(0).toISOString() } }), { code: 'NotAuthorized' });
  await assert.rejects(f.repo.create({ name: 'No', panels: [] }), { code: 'InvalidBoard' });
  await assert.rejects(f.repo.delete('b', 1), { code: 'InvalidBoard' });
  assert.equal(f.connection.requests.length, 0);
  await f.source.stop(); assert.equal(f.active, 0);
});

test('permitted operations use captured leases, preserve conflicts and assemble scoped image responses', async t => {
  const f = setup(true, true); t.after(() => f.source.stop());
  f.source.attachRenderer(() => ({ ok: true })); await f.source.start(); await waitFor(() => !!f.view);
  await f.source.action('lease1', { version: 'v0.9', action: { surfaceId: surface, name: 'click', context: {}, sourceComponentId: 'button', timestamp: new Date(0).toISOString() } });
  assert.equal(f.connection.requests[0]!.lease, 'lease1');
  const invalid = board(); invalid.panels[0]!.reference.surfaceId = 'private';
  await assert.rejects(f.repo.update('b', 1, invalid), { code: 'InvalidBoard' });
  f.connection.response = (_value, stream) => stream.push({ kind: 'error', code: 'Conflict' });
  await assert.rejects(f.repo.update('b', 1, { name: 'Shared', panels: board().panels }), { code: 'Conflict' });
  f.connection.response = (_value, stream) => { stream.push({ kind: 'imageChunk', mime: 'image/png', width: 1, height: 1, data: btoa('pixels') }); stream.push({ kind: 'imageEnd' }); };
  const response = await f.source.image('lease1', 'omochi://resources/' + '0'.repeat(32), new AbortController().signal);
  assert.equal(await response.text(), 'pixels'); assert.equal(response.headers.get('x-omochi-image-width'), '1');
  f.connection.response = () => {};
  const cancel = new AbortController(), pending = f.source.image('lease1', 'uri', cancel.signal); cancel.abort();
  await assert.rejects(pending); assert.equal(f.connection.streams.at(-1)!.dead, true);
});

test('the installed renderer consumes hub snapshots and same-ID recreation expires the previous image lease', async t => {
  const f = setup(); const controller = createA2uiSession(f.source); t.after(() => controller.close());
  await waitFor(() => controller.getSnapshot().surfaceIds.includes(surface));
  assert.equal(controller.getSnapshot().status, 'Connected');
  const oldImage = controller.getSnapshot().images!.scope(surface)!;
  f.connection.control.push({ kind: 'message', sequence: '3', lease: null, message: { version: 'v0.9', deleteSurface: { surfaceId: surface } } });
  await waitFor(() => !controller.getSnapshot().surfaceIds.includes(surface)); assert.equal(oldImage.signal.aborted, true);
  f.connection.control.push({ kind: 'message', sequence: '4', lease: 'lease2', message: { version: 'v0.9', createSurface: { surfaceId: surface, catalogId: BASIC_CATALOG_ID } } });
  await waitFor(() => controller.getSnapshot().surfaceIds.includes(surface));
  await assert.rejects(f.source.image('lease1', 'old', new AbortController().signal), { code: 'ScopeExpired' });
});

test('an unauthorized Surface retracts partial state and never requests an HTTP fallback', async t => {
  const f = setup(); t.after(() => f.source.stop()); const events: RenderEvent[] = []; let code = '';
  f.source.attachRenderer(event => { events.push(event); return { ok: true }; });
  f.source.subscribe(event => { if (event.kind === 'status' && event.error) code = event.error.code; });
  await f.source.start(); await waitFor(() => !!f.view);
  f.connection.control.push({ kind: 'message', sequence: '3', lease: 'stolen', message: { version: 'v0.9', createSurface: { surfaceId: `${hubId}~private/secret`, catalogId: BASIC_CATALOG_ID } } });
  await waitFor(() => code === 'ProtocolError'); await waitFor(() => f.active === 0);
  assert(events.some(event => event.kind === 'remove')); assert.equal(f.connection.requests.length, 0);
});

test('sharing links reject unknown authority, invalid relay hints and malformed wrappers before connection', () => {
  const value = { version: 1, hubId, grantId, mode: 'public', secret: '0'.repeat(64), relayUrls: ['https://relay.example/'] };
  const encode = (v: unknown) => '#share:' + btoa(JSON.stringify(v)).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '');
  assert.deepEqual(parseShareLink(encode(value)), value);
  for (const invalid of [{ ...value, endpoint: 'elsewhere' }, { ...value, relayUrls: ['http://relay.example'] }, { ...value, mode: 'owner' }, { ...value, secret: 'short' }, { ...value, hubId: 'old-id' }]) assert.throws(() => parseShareLink(encode(invalid)), { code: 'InvalidInvitation' });
});

test('authorization closure during the first hello is terminal and does not become a reconnect loop', async t => {
  const f = setup(); t.after(() => f.source.stop()); f.connection.denyOnHello(); let code = '';
  f.source.attachRenderer(() => ({ ok: true }));
  f.source.subscribe(event => { if (event.kind === 'status' && event.error) code = event.error.code; });
  await f.source.start(); await waitFor(() => code === 'NotAuthorized'); await waitFor(() => f.active === 0);
  assert.equal(f.view, null); assert.equal(f.connection.requests.length, 0);
});
