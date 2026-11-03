import { dashboard } from '../http-boards.js';
import { watchBoardChanges, BoardFault, notifyBoardListeners, type BoardRepository } from '../board-repository.js';
import { boardFields, validateBoardInput } from '../board-codec.js';
import type { Dashboard, DashboardInput, DashboardPreset } from '../contracts.js';
import type { RemoteHubSession } from './session.js';
import type { SharingRepository } from '../sharing-repository.js';
export class OwnerBoardRepository implements BoardRepository {
  private listeners = new Set<() => void>();
  private stopWatching: (() => void) | null = null;
  readonly session: Pick<RemoteHubSession, 'control'>;
  constructor(session: Pick<RemoteHubSession, 'control'>) { this.session = session; }
  subscribe(listener: () => void) {
    this.listeners.add(listener);
    this.stopWatching ??= watchBoardChanges(() => this.list(), () => notifyBoardListeners(this.listeners));
    return () => { this.listeners.delete(listener); if (!this.listeners.size) { this.stopWatching?.(); this.stopWatching = null; } };
  }

  async list(): Promise<readonly Dashboard[]> { const value = await this.session.control({ method: 'boardsList' }); if (!Array.isArray(value)) throw new BoardFault('StorageCorrupt'); return value.map(dashboard); }
  async get(id: string): Promise<Dashboard> { const result = dashboard(await this.session.control({ method: 'boardGet', id })); if (result.id !== id) throw new BoardFault('StorageCorrupt'); return result; }
  async presets(): Promise<readonly DashboardPreset[]> {
    const value = await this.session.control({ method: 'presets' });
    if (!Array.isArray(value)) throw new BoardFault('StorageCorrupt');
    const ids = new Set<string>();
    return value.map(item => { const row = boardFields(item, ['id', 'name', 'panels']); if (typeof row.id !== 'string' || !row.id.trim() || row.id.length > 100 || ids.has(row.id)) throw new BoardFault('StorageCorrupt'); ids.add(row.id); return { id: row.id, ...validateBoardInput({ name: row.name, panels: row.panels }) }; });
  }
  async create(input: DashboardInput): Promise<Dashboard> { const board = dashboard(await this.session.control({ method: 'boardCreate', value: validateBoardInput(input, true) })); notifyBoardListeners(this.listeners); return board; }
  async update(id: string, revision: number, input: DashboardInput): Promise<Dashboard> { if (!Number.isSafeInteger(revision) || revision < 1) throw new BoardFault('Conflict'); const board = dashboard(await this.session.control({ method: 'boardUpdate', id, value: { ...validateBoardInput(input, true), revision } })); notifyBoardListeners(this.listeners); return board; }
  async delete(id: string, revision: number): Promise<void> { if (!Number.isSafeInteger(revision) || revision < 1) throw new BoardFault('Conflict'); await this.session.control({ method: 'boardDelete', id, value: { revision } }); notifyBoardListeners(this.listeners); }
}
export class OwnerSharingRepository implements SharingRepository {
  readonly session: Pick<RemoteHubSession, 'control'>;
  constructor(session: Pick<RemoteHubSession, 'control'>) { this.session = session; }
  request(path: string, method = 'GET', body?: unknown): Promise<unknown> {
    if (path === '/api/sharing' && method === 'GET') return this.session.control({ method: 'sharing' });
    if (path === '/api/shares' && method === 'GET') return this.session.control({ method: 'sharesList' });
    if (path === '/api/shares' && method === 'POST') {
      const value = body as { boardId: string; public: boolean; permissions: unknown };
      return this.session.control({ method: 'shareCreate', board_id: value.boardId, public: value.public, permissions: value.permissions });
    }
    if (path.startsWith('/api/shares/') && method === 'DELETE') return this.session.control({ method: 'shareRevoke', id: decodeURIComponent(path.slice('/api/shares/'.length)) });
    return Promise.reject(new Error('Owner operation unavailable'));
  }
}
