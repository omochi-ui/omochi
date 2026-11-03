import { SessionFault, type LiveEvent, type LiveSession } from '@omochi-ui/core/browser';

/** Board-only UI while ownership/recovery is unavailable. It never binds an identity or dials a source. */
export function createDisconnectedSession(): LiveSession {
  let attached = false, phase: 'idle' | 'started' | 'stopped' = 'idle';
  const listeners = new Set<(event: LiveEvent) => void>();
  return {
    imagePolicy: 'sdk-png',
    subscribe(listener) {
      listeners.add(listener);
      for (const event of [{ kind: 'images', enabled: true }, { kind: 'services', services: [] }] as const) { try { listener(event); } catch { /* Independent observer. */ } }
      return () => listeners.delete(listener);
    },
    attachRenderer() { if (attached || phase !== 'idle') throw new SessionFault('InvalidState'); attached = true; },
    async start() { if (!attached || phase === 'stopped') throw new SessionFault('InvalidState'); phase = 'started'; },
    setServices() {},
    async retry() { throw new SessionFault('SourceUnavailable'); }, async action() { throw new SessionFault('ScopeExpired'); },
    async image() { throw new SessionFault('UnsupportedMedia'); }, async stop() { phase = 'stopped'; },
  };
}
