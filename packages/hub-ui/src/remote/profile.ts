import { SessionFault } from '@omochi-ui/core/browser';
import type { BrowserIdentity, BrowserTransport } from '@omochi-ui/hub-transport';
import { validSavedShare, type SavedShare } from './link.js';
export const PROFILE_DB = 'omochi.hub-client.v1';
export const PROFILE_LOCK = `${PROFILE_DB}/endpoint`;
export interface ShareRecord extends SavedShare { id: string; state: 'pending' | 'accepted' }
interface StoredIdentity extends BrowserIdentity { id: 'default' }
function failed() { return new SessionFault('StorageUnavailable'); }
async function ownerLock(): Promise<() => Promise<void>> {
  if (typeof navigator !== 'object' || typeof navigator.locks?.request !== 'function') throw new SessionFault('UnsupportedBrowser');
  return new Promise((resolve, reject) => {
    let release!: () => void;
    const done = new Promise<void>(resolve => { release = resolve; });
    let task: Promise<unknown>;
    try {
      task = navigator.locks.request(PROFILE_LOCK, { mode: 'exclusive', ifAvailable: true }, async lock => {
        if (!lock) { reject(new SessionFault('OwnerBusy')); return; }
        resolve(async () => { release(); await task; }); await done;
      });
      void task.catch(() => reject(failed()));
    } catch { reject(failed()); }
  });
}
function open(): Promise<IDBDatabase> {
  if (typeof indexedDB !== 'object') return Promise.reject(failed());
  return new Promise((resolve, reject) => {
    let request: IDBOpenDBRequest, done = false;
    const fail = (code: 'StorageUnavailable' | 'StorageVersion' = 'StorageUnavailable') => { done = true; reject(new SessionFault(code)); };
    try { request = indexedDB.open(PROFILE_DB, 1); } catch { fail(); return; }
    request.onblocked = () => fail('StorageVersion');
    request.onerror = () => fail(request.error?.name === 'VersionError' ? 'StorageVersion' : 'StorageUnavailable');
    request.onupgradeneeded = () => { try { request.result.createObjectStore('identity', { keyPath: 'id' }); request.result.createObjectStore('shares', { keyPath: 'id' }); } catch { request.transaction?.abort(); } };
    request.onsuccess = () => {
      const db = request.result;
      if (done) { db.close(); return; }
      try {
        if (db.version !== 1 || Array.from(db.objectStoreNames).sort().join(',') !== 'identity,shares') throw 0;
        const tx = db.transaction(['identity', 'shares'], 'readonly');
        for (const name of ['identity', 'shares']) { const store = tx.objectStore(name); if (store.keyPath !== 'id' || store.autoIncrement || store.indexNames.length) throw 0; }
        db.onversionchange = () => db.close(); resolve(db);
      } catch { db.close(); done = true; reject(new SessionFault('StorageCorrupt')); }
    };
  });
}
function tx<T>(db: IDBDatabase, stores: string[], mode: IDBTransactionMode, issue: (transaction: IDBTransaction, result: (value: T) => void) => void): Promise<T> {
  return new Promise((resolve, reject) => {
    let transaction: IDBTransaction;
    try { transaction = db.transaction(stores, mode); } catch { reject(failed()); return; }
    let value: T;
    transaction.oncomplete = () => resolve(value); transaction.onabort = () => reject(failed());
    try { issue(transaction, result => { value = result; }); } catch { transaction.abort(); }
  });
}
function validRecord(value: unknown): value is ShareRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const v = value as ShareRecord;
  if (Object.keys(v).sort().join(',') !== 'grantId,hubId,id,name,relayUrls,state') return false;
  const { id, state, ...source } = v;
  return validSavedShare(source) && id === `${v.hubId}/${v.grantId}` && ['pending', 'accepted'].includes(state);
}
function read(db: IDBDatabase): Promise<{ identities: unknown[]; shares: unknown[] }> {
  return tx(db, ['identity', 'shares'], 'readonly', (transaction, result) => {
    const identities = transaction.objectStore('identity').getAll(), shares = transaction.objectStore('shares').getAll();
    const value = { identities: [] as unknown[], shares: [] as unknown[] }; result(value);
    identities.onsuccess = () => { value.identities = identities.result; }; shares.onsuccess = () => { value.shares = shares.result; };
  });
}
export class HubProfile {
  private closing = false;
  private readonly writes = new Set<Promise<unknown>>();
  private closed: Promise<void> | null = null;
  readonly identity: BrowserIdentity;
  private readonly db: IDBDatabase;
  private readonly release: () => Promise<void>;
  constructor(db: IDBDatabase, identity: BrowserIdentity, release: () => Promise<void>) { this.db = db; this.release = release; this.identity = Object.freeze({ ...identity }); }
  async shares(): Promise<ShareRecord[]> {
    if (this.closing) throw new SessionFault('InvalidState');
    const { shares } = await read(this.db);
    if (!shares.every(validRecord) || shares.length > 1000) throw new SessionFault('StorageCorrupt');
    return structuredClone(shares);
  }
  async save(source: SavedShare, state: 'pending' | 'accepted'): Promise<void> {
    if (this.closing || !validSavedShare(source)) throw new SessionFault('InvalidState');
    const id = `${source.hubId}/${source.grantId}`;
    const owned = structuredClone(source);
    const write = tx<void>(this.db, ['shares'], 'readwrite', transaction => {
      const store = transaction.objectStore('shares'), request = store.get(id);
      request.onsuccess = () => {
        try {
          if (request.result !== undefined && !validRecord(request.result)) { transaction.abort(); return; }
          const count = store.count();
          count.onsuccess = () => { try { if (request.result === undefined && count.result >= 1000) { transaction.abort(); return; } store.put({ ...owned, id, state: request.result?.state === 'accepted' ? 'accepted' : state }); } catch { transaction.abort(); } };
        } catch { transaction.abort(); }
      };
    });
    this.writes.add(write); try { await write; } finally { this.writes.delete(write); }
  }
  async forget(id: string): Promise<void> {
    if (this.closing) throw new SessionFault('InvalidState');
    const write = tx<void>(this.db, ['shares'], 'readwrite', t => { t.objectStore('shares').delete(id); });
    this.writes.add(write); try { await write; } finally { this.writes.delete(write); }
  }
  /** Call after the live endpoint has stopped; lock ownership spans its whole lifetime. */
  close(): Promise<void> {
    return this.closed ??= (async () => { this.closing = true; await Promise.allSettled(this.writes); this.db.close(); await this.release(); })();
  }
}
export async function openHubProfile(transport: BrowserTransport): Promise<HubProfile> {
  const release = await ownerLock(); let db: IDBDatabase | undefined;
  try {
    db = await open(); const data = await read(db);
    if (data.identities.length > 1 || !data.shares.every(validRecord) || data.shares.length > 1000) throw new SessionFault('StorageCorrupt');
    let identity: BrowserIdentity;
    if (!data.identities.length) {
      if (data.shares.length) throw new SessionFault('StorageCorrupt');
      identity = await transport.generateIdentity();
      await tx<void>(db, ['identity'], 'readwrite', transaction => { transaction.objectStore('identity').add({ id: 'default', ...identity }); });
    } else {
      const row = data.identities[0] as StoredIdentity;
      if (!row || typeof row !== 'object' || Object.keys(row).sort().join(',') !== 'endpointId,id,secretHex' || row.id !== 'default' || typeof row.secretHex !== 'string' || typeof row.endpointId !== 'string') throw new SessionFault('StorageCorrupt');
      identity = await transport.identityFromSecret(row.secretHex);
      if (identity.endpointId !== row.endpointId) throw new SessionFault('StorageCorrupt');
    }
    return new HubProfile(db, identity, release);
  } catch (error) { db?.close(); await release(); throw error instanceof SessionFault ? error : failed(); }
}
/** Explicit recovery clears only local browser identity/connection records, never hub boards or grants. */
export async function resetHubProfile(): Promise<void> {
  const release = await ownerLock();
  try {
    if (typeof indexedDB !== 'object') throw failed();
    await new Promise<void>((resolve, reject) => {
      let request: IDBOpenDBRequest;
      try { request = indexedDB.deleteDatabase(PROFILE_DB); } catch { reject(failed()); return; }
      request.onsuccess = () => resolve(); request.onerror = () => reject(failed()); request.onblocked = () => reject(failed());
    });
  } finally { await release(); }
}
