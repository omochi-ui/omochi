import type { Dashboard, DashboardInput, DashboardPreset } from './contracts.js';

export type BoardErrorCode = 'NotFound' | 'Conflict' | 'InvalidBoard' | 'StorageUnavailable' | 'StorageCorrupt';
const messages: Record<BoardErrorCode, string> = {
  NotFound: 'This dashboard was not found.',
  Conflict: 'This dashboard changed elsewhere. Reload before saving.',
  InvalidBoard: 'The board is invalid or exceeds the supported limits.',
  StorageUnavailable: 'Dashboard storage is unavailable. Please retry.',
  StorageCorrupt: 'Saved dashboard data is invalid. Repair or restore it before saving.',
};
export class BoardFault extends Error {
  readonly code: BoardErrorCode;
  constructor(code: BoardErrorCode) { super(messages[code]); this.name = 'BoardFault'; this.code = code; }
}
export interface BoardRepository {
  list(): Promise<readonly Dashboard[]>;
  get(id: string): Promise<Dashboard>;
  presets(): Promise<readonly DashboardPreset[]>;
  create(input: DashboardInput): Promise<Dashboard>;
  update(id: string, revision: number, input: DashboardInput): Promise<Dashboard>;
  delete(id: string, revision: number): Promise<void>;
  subscribe(changed: () => void): () => void;
}
export function notifyBoardListeners(listeners: ReadonlySet<() => void>): void {
  for (const listener of listeners) {
    try { const value: unknown = listener(); if (value instanceof Promise) void value.catch(() => {}); } catch { /* A committed write cannot be undone by an observer. */ }
  }
}
