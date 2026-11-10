import { boardFields, validateBoardInput } from './board-codec.js';
import {
  watchBoardChanges,
  BoardFault,
  notifyBoardListeners,
  type BoardRepository,
} from './board-repository.js';
import type {
  Dashboard,
  DashboardInput,
  DashboardPreset,
} from './contracts.js';

export function dashboard(value: unknown): Dashboard {
  try {
    const row = boardFields(value, ['id', 'revision', 'name', 'panels']);
    if (
      typeof row.id !== 'string' ||
      !row.id.trim() ||
      row.id.length > 100 ||
      !Number.isSafeInteger(row.revision) ||
      Number(row.revision) < 1
    )
      throw new BoardFault('StorageCorrupt');
    return {
      id: row.id,
      revision: row.revision as number,
      ...validateBoardInput({ name: row.name, panels: row.panels }),
    };
  } catch {
    throw new BoardFault('StorageCorrupt');
  }
}
export class HttpBoardRepository implements BoardRepository {
  private readonly listeners = new Set<() => void>();
  private stopWatching: (() => void) | null = null;
  private readonly origin: string;
  constructor(origin = '') {
    this.origin = origin;
  }
  subscribe(changed: () => void): () => void {
    this.listeners.add(changed);
    this.stopWatching ??= watchBoardChanges(
      () => this.list(),
      () => notifyBoardListeners(this.listeners),
    );
    return () => {
      this.listeners.delete(changed);
      if (!this.listeners.size) {
        this.stopWatching?.();
        this.stopWatching = null;
      }
    };
  }
  async list(): Promise<readonly Dashboard[]> {
    const value = await this.request('/api/dashboards');
    if (!Array.isArray(value)) throw new BoardFault('StorageCorrupt');
    const list = value.map(dashboard);
    if (new Set(list.map((board) => board.id)).size !== list.length)
      throw new BoardFault('StorageCorrupt');
    return list;
  }
  async get(id: string): Promise<Dashboard> {
    const saved = dashboard(
      await this.request(`/api/dashboards/${encodeURIComponent(id)}`),
    );
    if (saved.id !== id) throw new BoardFault('StorageCorrupt');
    return saved;
  }
  async presets(): Promise<readonly DashboardPreset[]> {
    const value = await this.request('/api/presets');
    if (!Array.isArray(value)) throw new BoardFault('StorageCorrupt');
    const ids = new Set<string>();
    return value.map((item) => {
      try {
        const row = boardFields(item, ['id', 'name', 'panels']);
        if (
          typeof row.id !== 'string' ||
          !row.id.trim() ||
          row.id.length > 100 ||
          ids.has(row.id)
        )
          throw new BoardFault('StorageCorrupt');
        ids.add(row.id);
        return {
          id: row.id,
          ...validateBoardInput({ name: row.name, panels: row.panels }),
        };
      } catch {
        throw new BoardFault('StorageCorrupt');
      }
    });
  }
  async create(input: DashboardInput): Promise<Dashboard> {
    const saved = dashboard(
      await this.request(
        '/api/dashboards',
        'POST',
        validateBoardInput(input, true),
      ),
    );
    notifyBoardListeners(this.listeners);
    return saved;
  }
  async update(
    id: string,
    revision: number,
    input: DashboardInput,
  ): Promise<Dashboard> {
    if (
      !Number.isSafeInteger(revision) ||
      revision < 1 ||
      revision >= Number.MAX_SAFE_INTEGER
    )
      throw new BoardFault('Conflict');
    const saved = dashboard(
      await this.request(`/api/dashboards/${encodeURIComponent(id)}`, 'PUT', {
        ...validateBoardInput(input, true),
        revision,
      }),
    );
    if (saved.id !== id) throw new BoardFault('StorageCorrupt');
    notifyBoardListeners(this.listeners);
    return saved;
  }
  async delete(id: string, revision: number): Promise<void> {
    if (!Number.isSafeInteger(revision) || revision < 1)
      throw new BoardFault('Conflict');
    await this.request(`/api/dashboards/${encodeURIComponent(id)}`, 'DELETE', {
      revision,
    });
    notifyBoardListeners(this.listeners);
  }
  private async request(
    path: string,
    method = 'GET',
    body?: unknown,
  ): Promise<unknown> {
    let response: Response;
    try {
      response = await fetch(this.origin + path, {
        method,
        headers: { 'Content-Type': 'application/json' },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
    } catch {
      throw new BoardFault('StorageUnavailable');
    }
    if (!response.ok) {
      await response.body?.cancel().catch(() => {});
      throw new BoardFault(
        response.status === 404
          ? 'NotFound'
          : response.status === 409
            ? 'Conflict'
            : response.status === 400 || response.status === 413
              ? 'InvalidBoard'
              : 'StorageUnavailable',
      );
    }
    if (method === 'DELETE') {
      await response.body?.cancel();
      return;
    }
    try {
      return await response.json();
    } catch {
      throw new BoardFault('StorageCorrupt');
    }
  }
}
