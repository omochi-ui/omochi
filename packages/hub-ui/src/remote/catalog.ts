import { SessionFault } from '@omochi-ui/core/browser';
import { parseShareLink, type ShareLink } from './link.js';

export interface ConnectionPreference {
  id: string;
  name?: string;
  link?: ShareLink;
  location?: string;
}
export const CATALOG_DB = 'omochi.connections.v1';
export const CATALOG_CHANNEL = 'omochi.connections.changed';
export const connectionId = (link: Pick<ShareLink, 'hubId' | 'grantId'>) =>
  `${link.hubId}/${link.grantId}`;
export function checkedLink(value: unknown): ShareLink {
  const encoded = btoa(
    String.fromCharCode(...new TextEncoder().encode(JSON.stringify(value))),
  )
    .replaceAll('+', '-')
    .replaceAll('/', '_')
    .replaceAll('=', '');
  const link = parseShareLink(`#share:${encoded}`);
  if (link.mode !== 'share') throw new SessionFault('StorageCorrupt');
  return link;
}
function valid(value: ConnectionPreference) {
  return (
    value &&
    typeof value.id === 'string' &&
    /^[a-f0-9]{64}\/[a-f0-9-]{36}$/.test(value.id) &&
    Object.keys(value).every((key) =>
      ['id', 'name', 'link', 'location'].includes(key),
    ) &&
    (value.name === undefined ||
      (typeof value.name === 'string' && value.name.length <= 200)) &&
    (value.location === undefined ||
      (typeof value.location === 'string' && value.location.length <= 1024)) &&
    (!value.link || connectionId(checkedLink(value.link)) === value.id)
  );
}
async function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(CATALOG_DB, 1);
    let failed = false;
    request.onupgradeneeded = () =>
      request.result.createObjectStore('connections', { keyPath: 'id' });
    request.onerror = request.onblocked = () => {
      failed = true;
      reject(new SessionFault('StorageUnavailable'));
    };
    request.onsuccess = () => {
      if (failed) {
        request.result.close();
        return;
      }
      request.result.onversionchange = () => request.result.close();
      resolve(request.result);
    };
  });
}
export function announceConnections() {
  if (typeof BroadcastChannel === 'undefined') return;
  try {
    const channel = new BroadcastChannel(CATALOG_CHANNEL);
    channel.postMessage('changed');
    channel.close();
  } catch {
    /* Persistence succeeds even when cross-tab notifications are unavailable. */
  }
}
export async function readCatalog(): Promise<ConnectionPreference[]> {
  const db = await open();
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction('connections', 'readonly');
      const request = tx.objectStore('connections').getAll();
      tx.oncomplete = () => {
        try {
          if (request.result.length > 1000 || !request.result.every(valid))
            throw 0;
          resolve(request.result);
        } catch {
          reject(new SessionFault('StorageCorrupt'));
        }
      };
      tx.onabort = () => reject(new SessionFault('StorageUnavailable'));
    });
  } finally {
    db.close();
  }
}
/** Read-modify-write under the IDB transaction, never rewrite a cached catalog. */
export async function changeCatalog(
  id: string,
  patch: Partial<Omit<ConnectionPreference, 'id'>> | null,
) {
  const db = await open();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction('connections', 'readwrite');
      const store = tx.objectStore('connections');
      tx.oncomplete = () => resolve();
      tx.onabort = () => reject(new SessionFault('StorageUnavailable'));
      if (!patch) {
        store.delete(id);
        return;
      }
      const request = store.get(id);
      request.onsuccess = () => {
        try {
          if (request.result && !valid(request.result)) throw 0;
          const next = { ...request.result, ...patch, id };
          if (!valid(next)) throw 0;
          const count = store.count();
          count.onsuccess = () => {
            if (!request.result && count.result >= 1000) tx.abort();
            else store.put(next);
          };
        } catch {
          tx.abort();
        }
      };
    });
  } finally {
    db.close();
  }
  announceConnections();
}
export async function clearCatalog() {
  const db = await open();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction('connections', 'readwrite');
      tx.objectStore('connections').clear();
      tx.oncomplete = () => resolve();
      tx.onabort = () => reject(new SessionFault('StorageUnavailable'));
    });
  } finally {
    db.close();
  }
  announceConnections();
}
