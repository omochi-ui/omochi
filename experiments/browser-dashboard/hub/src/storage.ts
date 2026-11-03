import { isValidNodeId, SessionFault } from '@omochi-ui/core/browser';
import type { GatewayRecord } from './session.js';
import { validRelayUrl } from './bootstrap.js';

export const PROFILE_DATABASE = 'omochi.browser.v1';
export const ENDPOINT_LOCK = `${PROFILE_DATABASE}/default/endpoint`;
export interface IdentityRecord { id: 'default'; secretHex: string; endpointId: string; createdAt: number }
export interface Preferences { id: 'ui'; selectedGateways: string[]; selectedBoardId: string | null }
export const defaultPreferences = (): Preferences => ({ id: 'ui', selectedGateways: [], selectedBoardId: null });
export function validUuid(value: unknown): value is string {
  return typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value);
}
function fields(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value) && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
}
export function safeTime(value: unknown): value is number { return Number.isSafeInteger(value) && Number(value) >= 0; }
export function validIdentity(value: unknown): value is IdentityRecord {
  return fields(value, ['id', 'secretHex', 'endpointId', 'createdAt']) && value.id === 'default' &&
    typeof value.secretHex === 'string' && isValidNodeId(value.secretHex) &&
    typeof value.endpointId === 'string' && isValidNodeId(value.endpointId) && safeTime(value.createdAt);
}
export function validGateway(value: unknown): value is GatewayRecord {
  return fields(value, ['nodeId', 'name', 'relayUrls', 'enrollment', 'createdAt']) &&
    typeof value.nodeId === 'string' && isValidNodeId(value.nodeId) && typeof value.name === 'string' &&
    value.name === value.name.trim() && value.name.length >= 1 && value.name.length <= 256 &&
    Array.isArray(value.relayUrls) && value.relayUrls.length >= 1 && value.relayUrls.length <= 4 && value.relayUrls.every(validRelayUrl) &&
    (value.enrollment === 'pending' || value.enrollment === 'accepted') && safeTime(value.createdAt);
}
export function validPreferences(value: unknown): value is Preferences {
  return fields(value, ['id', 'selectedGateways', 'selectedBoardId']) && value.id === 'ui' &&
    Array.isArray(value.selectedGateways) && value.selectedGateways.length <= 2 &&
    value.selectedGateways.every(id => typeof id === 'string' && isValidNodeId(id)) && new Set(value.selectedGateways).size === value.selectedGateways.length &&
    (value.selectedBoardId === null || validUuid(value.selectedBoardId));
}

/** All request chains are synchronous/callback-driven; only transaction completion resolves. */
export function transaction<T>(db: IDBDatabase, stores: string[], mode: IDBTransactionMode,
  issue: (tx: IDBTransaction, result: (value: T) => void) => void): Promise<T> {
  return new Promise((resolve, reject) => {
    let tx: IDBTransaction;
    try { tx = db.transaction(stores, mode); } catch { reject(new SessionFault('StorageUnavailable')); return; }
    let value: T;
    let error: unknown;
    tx.oncomplete = () => resolve(value);
    tx.onabort = tx.onerror = () => reject(error instanceof SessionFault ? error : new SessionFault('StorageUnavailable'));
    try { issue(tx, result => { value = result; }); }
    catch (cause) { error = cause; try { tx.abort(); } catch { reject(new SessionFault('StorageUnavailable')); } }
  });
}

export function openProfileDatabase(): Promise<IDBDatabase> {
  if (typeof indexedDB !== 'object') return Promise.reject(new SessionFault('StorageUnavailable'));
  return new Promise((resolve, reject) => {
    let request: IDBOpenDBRequest;
    let failed = false;
    const fail = (code: 'StorageUnavailable' | 'StorageVersion' | 'StorageCorrupt') => { failed = true; reject(new SessionFault(code)); };
    try { request = indexedDB.open(PROFILE_DATABASE, 1); } catch { fail('StorageUnavailable'); return; }
    request.onblocked = () => fail('StorageUnavailable');
    request.onerror = () => fail(request.error?.name === 'VersionError' ? 'StorageVersion' : 'StorageUnavailable');
    request.onupgradeneeded = event => {
      if (failed || event.oldVersion !== 0) { request.transaction!.abort(); return; }
      for (const [name, keyPath] of [['identity', 'id'], ['gateways', 'nodeId'], ['boards', 'id'], ['preferences', 'id']]) {
        request.result.createObjectStore(name!, { keyPath });
      }
    };
    request.onsuccess = () => {
      const db = request.result;
      if (failed) { db.close(); return; }
      try {
        const names = ['identity', 'gateways', 'boards', 'preferences'];
        if (db.objectStoreNames.length !== 4 || names.some(name => !db.objectStoreNames.contains(name))) throw new SessionFault('StorageCorrupt');
        const tx = db.transaction(names, 'readonly');
        for (const name of names) {
          const store = tx.objectStore(name);
          if (store.keyPath !== (name === 'gateways' ? 'nodeId' : 'id') || store.autoIncrement || store.indexNames.length) throw new SessionFault('StorageCorrupt');
        }
      } catch { db.close(); fail('StorageCorrupt'); return; }
      db.onversionchange = () => db.close();
      resolve(db);
    };
  });
}

export function readProfile(db: IDBDatabase): Promise<{ identities: unknown[]; gateways: unknown[]; preferences: unknown[] }> {
  return transaction(db, ['identity', 'gateways', 'preferences'], 'readonly', (tx, result) => {
    const profile = { identities: [] as unknown[], gateways: [] as unknown[], preferences: [] as unknown[] };
    tx.objectStore('identity').getAll().onsuccess = event => { profile.identities = (event.target as IDBRequest<unknown[]>).result; };
    tx.objectStore('gateways').getAll().onsuccess = event => { profile.gateways = (event.target as IDBRequest<unknown[]>).result; };
    tx.objectStore('preferences').getAll().onsuccess = event => { profile.preferences = (event.target as IDBRequest<unknown[]>).result; };
    result(profile);
  });
}

export async function resetProfile(db: IDBDatabase): Promise<void> {
  await transaction<void>(db, ['identity', 'gateways', 'preferences'], 'readwrite', tx => {
    const preferences = tx.objectStore('preferences');
    const request = preferences.get('ui');
    request.onsuccess = () => {
      const previous = request.result as { selectedBoardId?: unknown } | undefined;
      const selectedBoardId = validUuid(previous?.selectedBoardId) ? previous.selectedBoardId : null;
      tx.objectStore('identity').clear(); tx.objectStore('gateways').clear(); preferences.clear();
      preferences.put({ ...defaultPreferences(), selectedBoardId });
    };
  });
}

/** The lock callback stays pending until its owner has joined endpoint shutdown. */
export function acquireProfileLock(): Promise<() => Promise<void>> {
  if (typeof globalThis.navigator?.locks?.request !== 'function') return Promise.reject(new SessionFault('UnsupportedBrowser'));
  return new Promise((resolve, reject) => {
    let release!: () => void;
    const released = new Promise<void>(done => { release = done; });
    let held: Promise<void>;
    try {
      held = navigator.locks.request(ENDPOINT_LOCK, { mode: 'exclusive', ifAvailable: true }, async lock => {
        if (!lock) throw new SessionFault('OwnerBusy');
        resolve(async () => { release(); await held; });
        await released;
      });
    } catch { reject(new SessionFault('StorageUnavailable')); return; }
    void held.catch(error => reject(error instanceof SessionFault ? error : new SessionFault('StorageUnavailable')));
  });
}
