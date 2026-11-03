import assert from 'node:assert/strict';
import { IDBFactory, IDBObjectStore } from 'fake-indexeddb';
import { test, type TestContext } from '@omochi-ui/core/testing';
import { SessionFault } from '@omochi-ui/core/browser';
import type { BrowserTransport } from '@omochi-ui/hub-transport';
import { openHubProfile, resetHubProfile, PROFILE_DB } from '../src/remote/profile.js';

function environment(t: TestContext) {
  const oldDB = Object.getOwnPropertyDescriptor(globalThis, 'indexedDB'), oldNav = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
  const factory = new IDBFactory(); let held = false, generated = 0, derived = 0, endpoints = 0;
  Object.defineProperty(globalThis, 'indexedDB', { configurable: true, value: factory });
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { locks: {
    async request(_name: string, _options: unknown, callback: (lock: object | null) => Promise<unknown>) { if (held) return callback(null); held = true; try { return await callback({}); } finally { held = false; } },
  } } });
  t.after(() => { if (oldDB) Object.defineProperty(globalThis, 'indexedDB', oldDB); else Reflect.deleteProperty(globalThis, 'indexedDB'); if (oldNav) Object.defineProperty(globalThis, 'navigator', oldNav); else Reflect.deleteProperty(globalThis, 'navigator'); });
  const transport: BrowserTransport = {
    async generateIdentity() { generated++; return { secretHex: '0'.repeat(64), endpointId: 'b'.repeat(64) }; },
    async identityFromSecret(secretHex) { derived++; if (!/^[a-f0-9]{64}$/.test(secretHex)) throw new SessionFault('StorageCorrupt'); return { secretHex, endpointId: 'b'.repeat(64) }; },
    async openEndpoint() { endpoints++; throw new Error('Storage never opens endpoints'); },
  };
  const database = async () => {
    const request = factory.open(PROFILE_DB, 1); const db = await new Promise<IDBDatabase>((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
    t.after(() => db.close()); return db;
  };
  return { factory, transport, database, get held() { return held; }, get generated() { return generated; }, get derived() { return derived; }, get endpoints() { return endpoints; } };
}
const share = () => ({ hubId: 'a'.repeat(64), grantId: '11111111-1111-4111-8111-111111111111', relayUrls: ['https://relay.example/'], name: 'Shared' });
function commit(db: IDBDatabase, store: string, issue: (store: IDBObjectStore) => void) { return new Promise<void>((resolve, reject) => { const tx = db.transaction(store, 'readwrite'); tx.oncomplete = () => resolve(); tx.onabort = () => reject(tx.error); issue(tx.objectStore(store)); }); }

test('hub profile persists identity before use, owns one endpoint lock and reloads without a socket', async t => {
  const f = environment(t), profile = await openHubProfile(f.transport); t.after(() => profile.close());
  assert.equal(f.held, true); assert.equal(f.generated, 1);
  await assert.rejects(openHubProfile(f.transport), { code: 'OwnerBusy' });
  await profile.save(share(), 'pending'); assert.equal((await profile.shares())[0]!.state, 'pending');
  await profile.save(share(), 'accepted'); await profile.save(share(), 'pending');
  assert.equal((await profile.shares())[0]!.state, 'accepted');
  const first = profile.identity.endpointId; await profile.close(); assert.equal(f.held, false);
  const reopened = await openHubProfile(f.transport); t.after(() => reopened.close());
  assert.equal(reopened.identity.endpointId, first); assert.equal(f.generated, 1); assert.equal(f.derived, 1); assert.equal(f.endpoints, 0);
  assert.equal((await reopened.shares())[0]!.state, 'accepted');
  await reopened.forget(`${share().hubId}/${share().grantId}`); assert.deepEqual(await reopened.shares(), []);
});

test('failed acceptance save leaves pending durable and retries locally with the same identity', async t => {
  const f = environment(t), profile = await openHubProfile(f.transport); t.after(() => profile.close());
  await profile.save(share(), 'pending');
  const original = IDBObjectStore.prototype.put;
  IDBObjectStore.prototype.put = function(value: unknown, key?: IDBValidKey) { if (this.name === 'shares') throw new DOMException('quota', 'QuotaExceededError'); return key === undefined ? original.call(this, value) : original.call(this, value, key); };
  t.after(() => { IDBObjectStore.prototype.put = original; });
  await assert.rejects(profile.save(share(), 'accepted'), { code: 'StorageUnavailable' });
  assert.equal((await profile.shares())[0]!.state, 'pending');
  IDBObjectStore.prototype.put = original; await profile.save(share(), 'accepted');
  assert.equal((await profile.shares())[0]!.state, 'accepted'); assert.equal(f.endpoints, 0); assert.equal(f.generated, 1);
});

test('corrupted identity fails without replacement; explicit reset does not touch hub boards or grant authority', async t => {
  const f = environment(t), profile = await openHubProfile(f.transport); await profile.save(share(), 'accepted'); await profile.close();
  const db = await f.database(); await commit(db, 'identity', store => { store.put({ id: 'default', secretHex: 'invalid', endpointId: 'b'.repeat(64) }); }); db.close();
  await assert.rejects(openHubProfile(f.transport), { code: 'StorageCorrupt' }); assert.equal(f.generated, 1); assert.equal(f.held, false); assert.equal(f.endpoints, 0);
  await resetHubProfile(); assert.equal(f.endpoints, 0);
  const fresh = await openHubProfile(f.transport); t.after(() => fresh.close()); assert.deepEqual(await fresh.shares(), []); assert.equal(f.generated, 2);
});

test('denied storage releases ownership before generating keys or opening network endpoints', async t => {
  const f = environment(t);
  Object.defineProperty(globalThis, 'indexedDB', { configurable: true, value: { open() { throw new DOMException('denied', 'SecurityError'); } } });
  await assert.rejects(openHubProfile(f.transport), { code: 'StorageUnavailable' });
  assert.equal(f.held, false); assert.equal(f.generated, 0); assert.equal(f.endpoints, 0);
});
