import {
  boardFields,
  validateBoardInput,
} from '../../../../packages/hub-ui/src/board-codec.js';
import {
  BoardFault,
  notifyBoardListeners,
  type BoardRepository,
} from '../../../../packages/hub-ui/src/board-repository.js';
import type {
  Dashboard,
  DashboardInput,
} from '../../../../packages/hub-ui/src/contracts.js';

interface StoredBoard extends Dashboard {
  createdAt: number;
  updatedAt: number;
}
const uuid = (value: unknown): value is string =>
  typeof value === 'string' &&
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value);
const time = (value: unknown): value is number =>
  Number.isSafeInteger(value) && Number(value) >= 0;
function stored(value: unknown): StoredBoard {
  try {
    const row = boardFields(value, [
      'id',
      'revision',
      'name',
      'panels',
      'createdAt',
      'updatedAt',
    ]);
    if (
      !uuid(row.id) ||
      !Number.isSafeInteger(row.revision) ||
      Number(row.revision) < 1 ||
      !time(row.createdAt) ||
      !time(row.updatedAt)
    )
      throw new BoardFault('StorageCorrupt');
    const input = validateBoardInput({ name: row.name, panels: row.panels });
    return {
      id: row.id,
      revision: row.revision as number,
      ...input,
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
    };
  } catch {
    throw new BoardFault('StorageCorrupt');
  }
}
function publicBoard({ id, revision, name, panels }: StoredBoard): Dashboard {
  return { id, revision, name, panels };
}

/** Board-only operations need no endpoint lock and never read identity, gateways or preferences. Owns its DB connection. */
export class BrowserBoardRepository implements BoardRepository {
  private readonly listeners = new Set<() => void>();
  private readonly channel: BroadcastChannel;
  private readonly db: IDBDatabase;
  private closed = false;
  constructor(db: IDBDatabase) {
    this.db = db;
    try {
      const store = db.transaction('boards', 'readonly').objectStore('boards');
      if (
        db.version !== 1 ||
        store.keyPath !== 'id' ||
        store.autoIncrement ||
        store.indexNames.length
      )
        throw new BoardFault('StorageCorrupt');
      this.channel = new BroadcastChannel('omochi.browser.v1/boards');
      this.channel.onmessage = ({ data }: MessageEvent<unknown>) => {
        if (
          data &&
          typeof data === 'object' &&
          Object.keys(data).length === 1 &&
          'kind' in data &&
          data.kind === 'changed'
        )
          notifyBoardListeners(this.listeners);
      };
    } catch (error) {
      throw error instanceof BoardFault
        ? error
        : new BoardFault('StorageUnavailable');
    }
  }
  subscribe(changed: () => void): () => void {
    this.listeners.add(changed);
    return () => this.listeners.delete(changed);
  }
  close(): void {
    this.closed = true;
    this.channel.close();
    this.listeners.clear();
    this.db.close();
  }
  async list(): Promise<readonly Dashboard[]> {
    const records = await this.transaction<StoredBoard[]>(
      'readonly',
      (tx, result, guard) => {
        const request = tx.objectStore('boards').getAll();
        request.onsuccess = guard(() => result(request.result.map(stored)));
      },
    );
    records.sort(
      (a, b) =>
        a.createdAt - b.createdAt || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0),
    );
    return records.map(publicBoard);
  }
  async get(id: string): Promise<Dashboard> {
    if (!uuid(id)) throw new BoardFault('NotFound');
    return this.transaction('readonly', (tx, result, guard) => {
      const request = tx.objectStore('boards').get(id);
      request.onsuccess = guard(() => {
        if (request.result === undefined) throw new BoardFault('NotFound');
        result(publicBoard(stored(request.result)));
      });
    });
  }
  async presets(): Promise<readonly []> {
    return [];
  }
  async create(input: DashboardInput): Promise<Dashboard> {
    const normalized = validateBoardInput(input, true),
      now = Date.now();
    if (!time(now)) throw new BoardFault('StorageUnavailable');
    const row = {
      id: crypto.randomUUID(),
      revision: 1,
      ...normalized,
      createdAt: now,
      updatedAt: now,
    };
    await this.transaction<void>('readwrite', (tx) => {
      tx.objectStore('boards').add(row);
    });
    this.changed();
    return publicBoard(row);
  }
  async update(
    id: string,
    revision: number,
    input: DashboardInput,
  ): Promise<Dashboard> {
    const normalized = validateBoardInput(input, true);
    if (!uuid(id)) throw new BoardFault('NotFound');
    const updated = await this.transaction<Dashboard>(
      'readwrite',
      (tx, result, guard) => {
        const store = tx.objectStore('boards'),
          request = store.get(id);
        request.onsuccess = guard(() => {
          if (request.result === undefined) throw new BoardFault('NotFound');
          const current = stored(request.result);
          if (
            current.revision !== revision ||
            !Number.isSafeInteger(revision) ||
            revision >= Number.MAX_SAFE_INTEGER
          )
            throw new BoardFault('Conflict');
          const now = Date.now();
          if (!time(now)) throw new BoardFault('StorageUnavailable');
          const next = {
            ...current,
            ...normalized,
            revision: revision + 1,
            updatedAt: now,
          };
          store.put(next);
          result(publicBoard(next));
        });
      },
    );
    this.changed();
    return updated;
  }
  async delete(id: string, revision: number): Promise<void> {
    if (!uuid(id)) throw new BoardFault('NotFound');
    await this.transaction<void>('readwrite', (tx, _result, guard) => {
      const store = tx.objectStore('boards'),
        request = store.get(id);
      request.onsuccess = guard(() => {
        if (request.result === undefined) throw new BoardFault('NotFound');
        if (
          stored(request.result).revision !== revision ||
          !Number.isSafeInteger(revision)
        )
          throw new BoardFault('Conflict');
        store.delete(id);
      });
    });
    this.changed();
  }
  private changed(): void {
    notifyBoardListeners(this.listeners);
    try {
      this.channel.postMessage({ kind: 'changed' });
    } catch {
      /* Commit already succeeded; a closing tab cannot roll it back. */
    }
  }
  private transaction<T>(
    mode: IDBTransactionMode,
    issue: (
      tx: IDBTransaction,
      result: (value: T) => void,
      guard: (callback: () => void) => () => void,
    ) => void,
  ): Promise<T> {
    if (this.closed)
      return Promise.reject(new BoardFault('StorageUnavailable'));
    return new Promise((resolve, reject) => {
      let tx: IDBTransaction;
      try {
        tx = this.db.transaction('boards', mode);
      } catch {
        reject(new BoardFault('StorageUnavailable'));
        return;
      }
      let value: T,
        failure: BoardFault | null = null;
      const guard = (callback: () => void) => () => {
        try {
          callback();
        } catch (error) {
          failure =
            error instanceof BoardFault
              ? error
              : new BoardFault('StorageUnavailable');
          tx.abort();
        }
      };
      tx.oncomplete = () => resolve(value);
      tx.onabort = () =>
        reject(failure ?? new BoardFault('StorageUnavailable'));
      guard(() =>
        issue(
          tx,
          (result) => {
            value = result;
          },
          guard,
        ),
      )();
    });
  }
}

/** Opening failures can be retried from the board UI without replacing identity or losing drafts. */
export class LazyBrowserBoardRepository implements BoardRepository {
  private readonly openDatabase: () => Promise<IDBDatabase>;
  private readonly listeners = new Set<() => void>();
  private repository: BrowserBoardRepository | null = null;
  private opening: Promise<BrowserBoardRepository> | null = null;
  private closed = false;
  constructor(openDatabase: () => Promise<IDBDatabase>) {
    this.openDatabase = openDatabase;
  }
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  async list() {
    return (await this.ensure()).list();
  }
  async get(id: string) {
    return (await this.ensure()).get(id);
  }
  async presets(): Promise<readonly []> {
    return [];
  }
  async create(input: DashboardInput) {
    const owned = validateBoardInput(input, true);
    return (await this.ensure()).create(owned);
  }
  async update(id: string, revision: number, input: DashboardInput) {
    const owned = validateBoardInput(input, true);
    return (await this.ensure()).update(id, revision, owned);
  }
  async delete(id: string, revision: number) {
    return (await this.ensure()).delete(id, revision);
  }
  close(): void {
    this.closed = true;
    this.repository?.close();
    this.listeners.clear();
  }
  private ensure(): Promise<BrowserBoardRepository> {
    if (this.closed)
      return Promise.reject(new BoardFault('StorageUnavailable'));
    if (this.repository) return Promise.resolve(this.repository);
    return (this.opening ??= Promise.resolve()
      .then(() => this.openDatabase())
      .then((db) => {
        if (this.closed) {
          db.close();
          throw new BoardFault('StorageUnavailable');
        }
        try {
          this.repository = new BrowserBoardRepository(db);
          this.repository.subscribe(() => notifyBoardListeners(this.listeners));
          return this.repository;
        } catch (error) {
          db.close();
          throw error;
        }
      })
      .catch((error) => {
        this.opening = null;
        throw error instanceof BoardFault
          ? error
          : new BoardFault(
              error &&
                typeof error === 'object' &&
                'code' in error &&
                error.code === 'StorageCorrupt'
                ? 'StorageCorrupt'
                : 'StorageUnavailable',
            );
      }));
  }
}
