import assert from 'node:assert/strict';
import { test, waitFor, type TestContext } from '@omochi-ui/core/testing';
import { IDBFactory, IDBObjectStore } from 'fake-indexeddb';
import { SessionFault } from '@omochi-ui/core/browser';
import type { BrowserTransport, Connection, Endpoint, ReceivedFrame, Stream } from '@omochi-ui/browser-transport';
import { openBrowserProfile, type ProfileOwner } from '../src/profile.js';
import { defaultPreferences, openProfileDatabase, readProfile, transaction, type IdentityRecord } from '../src/storage.js';

const id = 'c'.repeat(64), a = 'a'.repeat(64), b = 'b'.repeat(64);
const identity: IdentityRecord = { id: 'default', endpointId: id, secretHex: '0'.repeat(64), createdAt: 1 };
function invite(nodeId = a, relays = ['https://relay.example/'], expires = Math.floor(Date.now() / 1000) + 180): string {
  return `omochi://invite/${Buffer.from(JSON.stringify({ version: 4, address: { id: nodeId, addrs: relays.map(Relay => ({ Relay })) }, secret: 'd'.repeat(64), expires })).toString('hex')}`;
}
function pending<T>(signal: AbortSignal): Promise<T> {
  return new Promise((_, reject) => {
    const abort = () => reject(new SessionFault('Cancelled'));
    if (signal.aborted) abort(); else signal.addEventListener('abort', abort, { once: true });
  });
}
function setup(t: TestContext) {
  let locked = false;
  const properties = new Map<string, PropertyDescriptor | undefined>();
  const set = (key: string, value: unknown) => { if (!properties.has(key)) properties.set(key, Object.getOwnPropertyDescriptor(globalThis, key)); Object.defineProperty(globalThis, key, { configurable: true, value }); };
  t.after(() => { for (const [key, previous] of properties) { if (previous) Object.defineProperty(globalThis, key, previous); else Reflect.deleteProperty(globalThis, key); } });
  set('indexedDB', new IDBFactory());
  set('navigator', { userAgent: 'Chromium/156 Linux', locks: { request: async (_name: string, _options: unknown, callback: (lock: object | null) => Promise<void>) => {
    if (locked) return callback(null);
    locked = true; try { await callback({}); } finally { locked = false; }
  } } });
  set('isSecureContext', true); set('createImageBitmap', () => {});
  const endpoints: { relays: string[]; closed: boolean; secret: string }[] = [];
  const alpns: string[] = [], headers: string[] = [];
  const modes = { pair: 'accept', probe: 'accept', failSave: false, abortSave: false, delayRead: false, proved: false };
  const originalPut = IDBObjectStore.prototype.put;
  IDBObjectStore.prototype.put = function(value: unknown, key?: IDBValidKey) {
    const accepted = this.name === 'gateways' && (value as { enrollment?: string }).enrollment === 'accepted';
    if (accepted && modes.failSave) throw new DOMException('fixture failure', 'QuotaExceededError');
    const request = key === undefined ? originalPut.call(this, value) : originalPut.call(this, value, key);
    if (accepted && modes.abortSave) request.onsuccess = () => this.transaction.abort();
    return request;
  };
  t.after(() => { IDBObjectStore.prototype.put = originalPut; });
  let generated = 0, verified = 0, bound = 0;
  const transport: BrowserTransport = {
    async generateIdentity() { generated++; return { secretHex: identity.secretHex, endpointId: id }; },
    async identityFromSecret(secretHex) { verified++; if (secretHex !== identity.secretHex) throw new SessionFault('StorageCorrupt'); return { secretHex, endpointId: id }; },
    async openEndpoint(secret, relays) {
      const db = await openProfileDatabase(); const raw = await readProfile(db); db.close();
      assert.equal((raw.identities[0] as IdentityRecord).endpointId, id, 'identity is committed before binding');
      assert.equal(secret, identity.secretHex);
      assert.equal(endpoints.every(endpoint => endpoint.closed), true, 'same-key endpoints cannot overlap');
      const record = { relays: [...relays], closed: false, secret }; endpoints.push(record); bound++;
      let closeConnection!: (result: { code: string; reason: string }) => void;
      const closed = new Promise<{ code: string; reason: string }>(resolve => { closeConnection = resolve; });
      const endpoint: Endpoint = {
        async connect(address, alpn, signal) {
          alpns.push(alpn); const node = JSON.parse(address).id as string;
          if (modes.pair === 'connect-fail' && alpn === 'omochi/pair/1') throw new SessionFault('NetworkUnavailable');
          if (signal.aborted) throw new SessionFault('Cancelled');
          const connection: Connection = { remoteId: modes.probe === 'wrong-id' ? b : node, closed: () => closed,
            async close() { closeConnection({ code: '0', reason: '' }); },
            async open(header, signal) {
              headers.push(header.kind);
              if (header.kind === 'control' && modes.probe === 'unauthorized') { closeConnection({ code: '1', reason: '' }); return pending(signal); }
              let received = 0;
              const stream: Stream = {
                async send(_text, signal) { if (signal.aborted) throw new SessionFault('Cancelled'); },
                async finish() {}, async cancel() {},
                async receive(signal): Promise<ReceivedFrame | null> {
                  if (header.kind === 'pair') {
                    if (modes.pair === 'lost' || modes.delayRead) return pending(signal);
                    modes.proved = true;
                    const json = JSON.stringify({ accepted: modes.pair !== 'reject' }); return { json, wireBytes: json.length };
                  }
                  if (modes.probe === 'stalled') return pending(signal);
                  if (received++ >= 2) return pending(signal);
                  const json = JSON.stringify(received === 1 ? { kind: 'hello', protocol: 4, nodeId: node, name: 'Gateway' } :
                    { kind: 'services', services: modes.probe === 'malformed' ? [{ id: 'bad/id' }] : [] });
                  return { json, wireBytes: json.length };
                },
              }; return stream;
            },
          }; return connection;
        },
        async close() { record.closed = true; closeConnection?.({ code: '0', reason: '' }); },
      }; return endpoint;
    },
  };
  const owners: ProfileOwner[] = [];
  t.after(async () => { for (const owner of owners) await owner.close(); });
  const open = async () => { const owner = await openBrowserProfile(transport); assert.equal(owner.kind, 'owner'); if (owner.kind !== 'owner') throw new Error('Expected owner'); owners.push(owner); return owner; };
  return { transport, open, modes, endpoints, alpns, headers, get generated() { return generated; }, get verified() { return verified; }, get bound() { return bound; }, get locked() { return locked; },
    set, async db() { return openProfileDatabase(); } };
}

test('profile commits identity before returning, holds one lock, verifies without sockets and publishes detached views', async t => {
  const f = setup(t); const owner = await f.open();
  assert.equal(f.generated, 1); assert.equal(f.verified, 1); assert.equal(f.bound, 0);
  assert.equal(owner.view().endpointId, id); assert.equal(owner.session()?.imagePolicy, 'sdk-png');
  assert.equal(JSON.stringify(owner.view()).includes('secretHex'), false);
  assert.equal(owner.view(), owner.view()); assert.equal(Object.isFrozen(owner.view()), true);
  await assert.rejects(openBrowserProfile(f.transport), { code: 'OwnerBusy' });
  assert.equal(f.bound, 0); await owner.close(); assert.equal(f.locked, false);
  await assert.rejects(owner.connectSelected(), { code: 'InvalidState' });
  const restored = await f.open(); assert.equal(f.generated, 1); assert.equal(f.verified, 2);
  await restored.close();
});

test('pairing persists pending first, saves accepted before ordinary traffic, and requires explicit selection', async t => {
  const f = setup(t), owner = await f.open();
  let observed = 0;
  owner.subscribe(() => { observed++; throw new Error('unrelated observer'); });
  await owner.pair(invite());
  assert.equal(owner.view().gateways[0]?.record.enrollment, 'accepted'); assert.equal(owner.view().gateways[0]?.selected, false);
  assert.equal(owner.view().gateways[0]?.record.name, a.slice(0, 12)); assert.equal(owner.view().gateways[0]?.recovery, 'none');
  assert.deepEqual(f.alpns, ['omochi/pair/1']); assert.ok(observed > 1);
  const view = owner.view(); assert.equal(Object.isFrozen(view.gateways[0]?.record.relayUrls), true);
  await owner.selectGateway(a, true); assert.equal(owner.view().gateways[0]?.selected, true);
  const source = owner.session(); await owner.connectSelected(); assert.equal(owner.session(), source);
  assert.equal(f.bound, 1, 'selection constructs an idle source until the renderer attaches');
  await owner.close();
  const restored = await f.open(); assert.equal(restored.view().gateways[0]?.selected, true); assert.equal(f.bound, 1);
});

test('acceptance save failure suspends connections; expired proof retries only the local transaction and needs explicit connect', async t => {
  const f = setup(t), owner = await f.open(); f.modes.failSave = true;
  const expiry = Math.floor(Date.now() / 1000) + 1;
  await assert.rejects(owner.pair(invite(a, undefined, expiry)), { code: 'EnrollmentUnsaved' });
  assert.equal(owner.view().gateways[0]?.record.enrollment, 'pending'); assert.equal(owner.view().gateways[0]?.recovery, 'unsaved');
  assert.equal(owner.view().connections, 'suspended'); assert.equal(owner.session(), null);
  await assert.rejects(owner.pair(invite(b)), { code: 'EnrollmentUnsaved' });
  await assert.rejects(owner.selectGateway(a, true), { code: 'EnrollmentUnsaved' });
  await assert.rejects(owner.connectSelected(), { code: 'EnrollmentUnsaved' });
  await assert.rejects(owner.forgetLocalGateway(b), { code: 'EnrollmentUnsaved' });
  await Bun.sleep(1100);
  assert.equal(owner.view().invitation?.state, 'expired'); f.modes.failSave = false;
  await owner.retryEnrollmentSave(a);
  assert.equal(f.bound, 1); assert.deepEqual(f.alpns, ['omochi/pair/1']); assert.equal(owner.session(), null);
  assert.equal(owner.view().gateways[0]?.record.enrollment, 'accepted');
  await owner.connectSelected(); assert.ok(owner.session()); assert.equal(owner.view().connections, 'available');
});

test('transaction abort after a successful accepted put still reports unsaved and keeps durable pending on reload', async t => {
  const f = setup(t), owner = await f.open(); f.modes.abortSave = true;
  await assert.rejects(owner.pair(invite()), { code: 'EnrollmentUnsaved' });
  await owner.close(); f.modes.abortSave = false;
  const restored = await f.open();
  assert.equal(restored.view().gateways[0]?.recovery, 'pending'); assert.equal(f.bound, 1);
  await assert.rejects(restored.retryEnrollmentSave(a), { code: 'InvalidState' });
  await restored.pair(invite()); assert.equal(f.generated, 1); assert.equal(restored.view().gateways[0]?.record.enrollment, 'accepted');
});

test('lost reply and rejection retain the same durable identity; closing joins cancellation before releasing ownership', async t => {
  const f = setup(t), owner = await f.open(); f.modes.pair = 'lost';
  const pairing = owner.pair(invite(a, undefined, Math.floor(Date.now() / 1000) + 1));
  await assert.rejects(pairing, { code: 'EnrollmentUnknown' });
  assert.equal(owner.view().gateways[0]?.recovery, 'unknown'); assert.equal(owner.view().endpointId, id);
  await owner.close(); const restored = await f.open(); assert.equal(restored.view().gateways[0]?.recovery, 'pending');
  f.modes.pair = 'reject'; await assert.rejects(restored.pair(invite()), { code: 'EnrollmentRejected' });
  f.modes.pair = 'accept'; await restored.pair(invite());
  assert.equal(restored.view().gateways[0]?.recovery, 'none'); assert.equal(f.generated, 1);
  f.modes.pair = 'lost'; const pendingPair = restored.pair(invite(b));
  while (f.alpns.length < 4) await Bun.sleep(1);
  const result = assert.rejects(pendingPair, { code: 'EnrollmentUnknown' }); await restored.close(); await result;
  assert.equal(f.locked, false); assert.equal(f.endpoints.every(endpoint => endpoint.closed), true);
});

test('accepted link refresh opens no pairing stream, changes only hints, and explicit redemption follows NotAuthorized', async t => {
  const f = setup(t), owner = await f.open(); await owner.pair(invite()); await owner.selectGateway(a, true);
  const original = owner.view().gateways[0]!.record;
  await owner.refreshInvitation(invite(a, ['https://new.example/']));
  assert.deepEqual(f.alpns, ['omochi/pair/1', 'omochi/gateway/4']); assert.deepEqual(f.headers, ['pair', 'control']);
  assert.deepEqual(owner.view().gateways[0]!.record, { ...original, relayUrls: ['https://new.example/'] });
  assert.equal(owner.view().invitation?.state, 'refreshed');
  f.modes.probe = 'unauthorized';
  await assert.rejects(owner.refreshInvitation(invite(a, ['https://other.example/'])), { code: 'NotAuthorized' });
  assert.equal(owner.view().invitation?.state, 'can-redeem'); assert.equal(f.alpns.filter(alpn => alpn === 'omochi/pair/1').length, 1);
  assert.deepEqual(owner.view().gateways[0]!.record.relayUrls, ['https://new.example/']);
  f.modes.probe = 'accept'; await owner.redeemInvitation(a);
  assert.equal(f.alpns.filter(alpn => alpn === 'omochi/pair/1').length, 2);
  assert.deepEqual(owner.view().gateways[0]!.record.relayUrls, ['https://other.example/']);
});

test('refresh save failure retains the old durable hints and local proof; forgetting that node discards it', async t => {
  const f = setup(t), owner = await f.open(); await owner.pair(invite());
  f.modes.failSave = true;
  await assert.rejects(owner.refreshInvitation(invite(a, ['https://new.example/'])), { code: 'EnrollmentUnsaved' });
  assert.deepEqual(owner.view().gateways[0]!.record.relayUrls, ['https://relay.example/']);
  assert.equal(owner.view().gateways[0]!.recovery, 'unsaved');
  f.modes.failSave = false; await owner.retryEnrollmentSave(a);
  assert.deepEqual(owner.view().gateways[0]!.record.relayUrls, ['https://new.example/']); assert.equal(owner.view().invitation?.state, 'refreshed');
  assert.equal(f.bound, 2); assert.equal(owner.session(), null);
  await owner.connectSelected();
  f.modes.failSave = true; await assert.rejects(owner.refreshInvitation(invite(a)), { code: 'EnrollmentUnsaved' });
  await owner.forgetLocalGateway(a); assert.equal(owner.view().gateways.length, 0); assert.equal(owner.session(), null);
  await assert.rejects(owner.retryEnrollmentSave(a), { code: 'InvalidState' });
});

test('malformed/expired invitations have zero effects; malformed refresh restores selection without pairing', async t => {
  const f = setup(t), owner = await f.open(); const source = owner.session();
  await assert.rejects(owner.pair('invalid'), { code: 'InvalidInvitation' });
  await assert.rejects(owner.pair(invite(a, undefined, 1)), { code: 'InvalidInvitation' });
  await assert.rejects(owner.refreshInvitation(invite()), { code: 'InvalidState' });
  assert.equal(owner.session(), source); assert.equal(f.bound, 0); assert.equal(owner.view().gateways.length, 0);
  await owner.pair(invite()); await owner.selectGateway(a, true); f.modes.probe = 'malformed';
  await assert.rejects(owner.refreshInvitation(invite()), { code: 'ProtocolError' });
  assert.equal(owner.view().invitation?.state, 'refresh-failed'); assert.equal(owner.view().gateways[0]!.selected, true);
  assert.ok(owner.session()); assert.equal(f.alpns.filter(alpn => alpn === 'omochi/pair/1').length, 1);
});

test('corrupt profile enters public recovery without generating keys, while reset retains board bytes and selected board', async t => {
  const f = setup(t), db = await f.db();
  const boardId = crypto.randomUUID(), board = { id: boardId, invalidBoardData: 'kept byte for byte' };
  await transaction<void>(db, ['identity', 'preferences', 'boards'], 'readwrite', tx => {
    tx.objectStore('identity').put({ ...identity, secretHex: '1'.repeat(64) });
    tx.objectStore('preferences').put({ ...defaultPreferences(), selectedBoardId: boardId });
    tx.objectStore('boards').put(board);
  }); db.close();
  const recovery = await openBrowserProfile(f.transport); assert.equal(recovery.kind, 'recovery');
  if (recovery.kind !== 'recovery') throw new Error('Expected recovery');
  t.after(() => recovery.close());
  assert.equal(recovery.error.code, 'StorageCorrupt'); assert.equal(recovery.storedEndpointId, id);
  assert.equal(f.generated, 0); assert.equal(f.bound, 0); await assert.rejects(f.open(), { code: 'OwnerBusy' });
  await recovery.resetIdentity(); assert.equal(f.locked, false);
  const reopened = await f.db(); const raw = await readProfile(reopened);
  const saved = await transaction<unknown>(reopened, ['boards'], 'readonly', (tx, result) => { const request = tx.objectStore('boards').get(boardId); request.onsuccess = () => result(request.result); });
  assert.deepEqual(saved, board); assert.deepEqual(raw.identities, []);
  assert.deepEqual(raw.preferences, [{ ...defaultPreferences(), selectedBoardId: boardId }]); reopened.close();
  const owner = await f.open(); assert.equal(owner.view().endpointId, id); assert.equal(f.generated, 1);
});

test('denied/unsupported/failed-upgrade storage releases ownership and never binds or generates keys', async t => {
  const f = setup(t);
  f.set('indexedDB', { open() { throw new DOMException('denied', 'SecurityError'); } });
  await assert.rejects(f.open(), { code: 'StorageUnavailable' }); assert.equal(f.locked, false);
  const factory = new IDBFactory(); f.set('indexedDB', factory);
  const request = factory.open('omochi.browser.v1', 2);
  const db = await new Promise<IDBDatabase>((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = reject; }); db.close();
  await assert.rejects(f.open(), { code: 'StorageVersion' }); assert.equal(f.locked, false);
  assert.equal(f.generated, 0); assert.equal(f.bound, 0);
});

test('eight live relay hints and four disjoint invitation hints never overlap or truncate, and restore on success/failure', async t => {
  const f = setup(t), owner = await f.open();
  const relays = (prefix: string) => Array.from({ length: 4 }, (_, index) => `https://${prefix}${index}.example/`);
  await owner.pair(invite(a, relays('a'))); await owner.pair(invite(b, relays('b')));
  await owner.selectGateway(a, true); await owner.selectGateway(b, true);
  const activate = async () => {
    const source = owner.session()!; const bound = f.bound;
    source.attachRenderer(() => ({ ok: true })); await source.start();
    await waitFor(() => f.bound > bound);
    return source;
  };
  await activate(); assert.deepEqual(f.endpoints.at(-1)!.relays, [...relays('a'), ...relays('b')]);
  await owner.refreshInvitation(invite(a, relays('c')));
  assert.deepEqual(f.endpoints.at(-1)!.relays, relays('c')); assert.equal(f.endpoints.at(-1)!.closed, true);
  await activate(); assert.deepEqual(f.endpoints.at(-1)!.relays, [...relays('c'), ...relays('b')]);
  f.modes.probe = 'malformed'; await assert.rejects(owner.refreshInvitation(invite(a, relays('d'))), { code: 'ProtocolError' });
  assert.deepEqual(f.endpoints.at(-1)!.relays, relays('d')); assert.equal(f.endpoints.at(-1)!.closed, true);
  await activate(); assert.deepEqual(f.endpoints.at(-1)!.relays, [...relays('c'), ...relays('b')]);
  assert.equal(owner.view().gateways.every(gateway => gateway.selected), true);
});

test('expiry aborts a healthy-control refresh probe, joins its endpoint and restores the selected source', async t => {
  const f = setup(t), owner = await f.open(); await owner.pair(invite()); await owner.selectGateway(a, true);
  f.modes.probe = 'stalled';
  await assert.rejects(owner.refreshInvitation(invite(a, ['https://new.example/'], Math.floor(Date.now() / 1000) + 1)), { code: 'InvalidInvitation' });
  assert.equal(owner.view().invitation?.state, 'expired'); assert.ok(owner.session());
  assert.equal(f.endpoints.every(endpoint => endpoint.closed), true); assert.deepEqual(owner.view().gateways[0]!.record.relayUrls, ['https://relay.example/']);
  const count = f.bound;
  await assert.rejects(owner.retryInvitationRefresh(a), { code: 'InvalidInvitation' });
  await assert.rejects(owner.redeemInvitation(a), { code: 'InvalidInvitation' }); assert.equal(f.bound, count);
});

test('failed reset is atomic, retains ownership for retry and does not generate a new identity', async t => {
  const f = setup(t), owner = await f.open(); await owner.pair(invite());
  const original = IDBObjectStore.prototype.clear;
  IDBObjectStore.prototype.clear = function() { if (this.name === 'identity') throw new DOMException('denied', 'QuotaExceededError'); return original.call(this); };
  t.after(() => { IDBObjectStore.prototype.clear = original; });
  // Fault in a request callback must abort the transaction and reject, rather than hanging.
  await assert.rejects(owner.resetIdentity(), { code: 'StorageUnavailable' });
  assert.equal(f.locked, true); assert.equal(owner.view().connections, 'suspended'); assert.equal(f.generated, 1);
  const db = await f.db(), raw = await readProfile(db); db.close();
  assert.equal(raw.identities.length, 1); assert.equal(raw.gateways.length, 1);
  IDBObjectStore.prototype.clear = original; await owner.resetIdentity(); assert.equal(f.locked, false);
});

test('aborted database initialization and denied lock requests release all ownership without identity generation', async t => {
  const f = setup(t), factory = new IDBFactory();
  const open = factory.open.bind(factory);
  factory.open = (name, version) => {
    const request = open(name, version);
    request.addEventListener('upgradeneeded', () => queueMicrotask(() => request.transaction?.abort()));
    return request;
  };
  f.set('indexedDB', factory);
  await assert.rejects(f.open(), { code: 'StorageUnavailable' });
  assert.equal(f.locked, false); assert.equal(f.generated, 0); assert.equal(f.bound, 0);
  f.set('navigator', { locks: { request() { throw new DOMException('denied', 'SecurityError'); } } });
  await assert.rejects(f.open(), { code: 'StorageUnavailable' });
  f.set('navigator', {}); await assert.rejects(f.open(), { code: 'UnsupportedBrowser' });
});

test('all profile record shapes reject unknown fields, invalid preferences and unsafe retained identity times', async t => {
  const f = setup(t), db = await f.db();
  t.after(() => db.close());
  const record = { nodeId: a, name: 'Source', relayUrls: ['https://relay.example/'], enrollment: 'accepted', createdAt: 1 };
  for (const [savedIdentity, gateway, preferences] of [
    [{ ...identity, extra: true }, record, defaultPreferences()],
    [{ ...identity, createdAt: Number.MAX_SAFE_INTEGER + 1 }, record, defaultPreferences()],
    [identity, { ...record, name: ' untrimmed ' }, defaultPreferences()],
    [identity, { ...record, relayUrls: ['http://relay.example/'] }, defaultPreferences()],
    [identity, { ...record, credential: 'not permitted' }, defaultPreferences()],
    [identity, record, { ...defaultPreferences(), selectedGateways: [a, a] }],
    [identity, record, { ...defaultPreferences(), selectedGateways: [b] }],
    [identity, record, { ...defaultPreferences(), selectedBoardId: 'not a UUID' }],
    [identity, record, { ...defaultPreferences(), extra: true }],
  ]) {
    await transaction<void>(db, ['identity', 'gateways', 'preferences'], 'readwrite', tx => {
      tx.objectStore('identity').put(savedIdentity); tx.objectStore('gateways').put(gateway); tx.objectStore('preferences').put(preferences);
    });
    const recovered = await openBrowserProfile(f.transport); assert.equal(recovered.kind, 'recovery');
    await recovered.close(); assert.equal(f.locked, false);
  }
  assert.equal(f.bound, 0); assert.equal(f.generated, 0);
});

test('slow endpoint closure publishes Closing pending after 2s and keeps exclusive ownership until join', async t => {
  const f = setup(t); const owner = await f.open();
  const originalOpen = f.transport.openEndpoint.bind(f.transport);
  let release!: () => void, entered = false, fire!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  t.after(() => release());
  f.transport.openEndpoint = async (...args) => {
    const endpoint = await originalOpen(...args), close = endpoint.close.bind(endpoint);
    endpoint.close = async () => { entered = true; await gate; await close(); };
    return endpoint;
  };
  const timeout = globalThis.setTimeout;
  f.set('setTimeout', ((callback: () => void, milliseconds?: number, ...args: unknown[]) => {
    if (milliseconds === 2000) { fire = callback; return timeout(() => {}, 1000000); }
    return timeout(callback, milliseconds, ...args);
  }) as typeof setTimeout);
  const pairing = owner.pair(invite());
  await waitFor(() => entered);
  assert.equal(owner.view().closingPending, false);
  assert.equal(f.locked, true);
  fire();
  assert.equal(owner.view().closingPending, true);
  await assert.rejects(openBrowserProfile(f.transport), { code: 'OwnerBusy' });
  release(); await pairing;
  assert.equal(owner.view().closingPending, false);
  assert.equal(f.endpoints.every(endpoint => endpoint.closed), true);
  assert.equal(f.locked, true, 'ordinary owner retains the lock after temporary endpoint closes');
  await owner.close(); assert.equal(f.locked, false);
});
