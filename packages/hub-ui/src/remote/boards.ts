import {
  BoardFault,
  notifyBoardListeners,
  type BoardRepository,
} from '../board-repository.js';
import { validateBoardInput } from '../board-codec.js';
import type { Dashboard, DashboardInput } from '../contracts.js';
import type { RemoteHubSession, SharedView } from './session.js';

/** A shared board is hub-owned. Only an explicitly permitted layout save is writable. */
export class SharedBoardRepository implements BoardRepository {
  private readonly listeners = new Set<() => void>();
  private view: SharedView | null = null;
  private session: RemoteHubSession | null = null;
  bind(session: RemoteHubSession) {
    this.session = session;
  }
  publish(view: SharedView) {
    this.view = structuredClone(view);
    notifyBoardListeners(this.listeners);
  }
  subscribe(listener: () => void) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  async list(): Promise<readonly Dashboard[]> {
    return this.view ? [structuredClone(this.view.board)] : [];
  }
  async get(id: string): Promise<Dashboard> {
    if (!this.view || this.view.board.id !== id)
      throw new BoardFault('NotFound');
    return structuredClone(this.view.board);
  }
  async presets(): Promise<readonly []> {
    return [];
  }
  async create(_input: DashboardInput): Promise<Dashboard> {
    throw new BoardFault('InvalidBoard');
  }
  async delete(_id: string, _revision: number): Promise<void> {
    throw new BoardFault('InvalidBoard');
  }
  async update(
    id: string,
    revision: number,
    input: DashboardInput,
  ): Promise<Dashboard> {
    if (
      !this.session ||
      !this.view ||
      !this.view.permissions.editLayout ||
      this.view.board.id !== id
    )
      throw new BoardFault('InvalidBoard');
    const normalized = validateBoardInput(input, true),
      current = this.view.board;
    if (
      normalized.panels.length !== current.panels.length ||
      normalized.panels.some((panel) => {
        const old = current.panels.find((p) => p.id === panel.id);
        return (
          !old ||
          old.reference.nodeId !== panel.reference.nodeId ||
          old.reference.serviceId !== panel.reference.serviceId ||
          old.reference.surfaceId !== panel.reference.surfaceId
        );
      })
    )
      throw new BoardFault('InvalidBoard');
    const board = await this.session.updateBoard(revision, normalized);
    this.view = { ...this.view, board };
    notifyBoardListeners(this.listeners);
    return structuredClone(board);
  }
}
