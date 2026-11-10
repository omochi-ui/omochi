import type { LiveSession } from '@omochi-ui/core/browser';
import type { BoardRepository } from './board-repository.js';

/** Storage and live data are independent: a board never grants or selects source access. */
export interface DashboardEnvironment {
  session: LiveSession;
  boards: BoardRepository;
  authority:
    | { kind: 'owner' }
    | {
        kind: 'share';
        permissions: { interact: boolean; editLayout: boolean };
      };
}
