import { ResourceContext } from './resources.js';
import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactNode } from 'react';
import type { LiveSession } from '@omochi-ui/core/browser';
import { createA2uiSession, type A2uiSession, type RendererSession } from './session.js';
import { createLocalHubSession } from './local-source.js';
import { A2uiSurface, MarkdownContext } from '@a2ui/react/v0_9';
import { renderMarkdown } from '@a2ui/markdown-it';

export type { A2uiSession } from './session.js';
const emptySession: Readonly<A2uiSession> = {
  processor: null, surfaceIds: [], services: [], status: 'Connecting…', error: null,
  images: null, endpoint: 'browser:gateway',
};
const noSubscription = () => () => {};
const emptySnapshot = () => emptySession;

/** The hook owns its controller and closes the source on final cleanup. */
export function useA2uiSource(source: LiveSession): Readonly<A2uiSession> {
  type Owner = { source: LiveSession; controller: RendererSession; owners: number; closing: Promise<void> | null };
  const active = useRef<Owner | null>(null);
  const lifecycle = useRef<Promise<void>>(Promise.resolve());
  const [controller, setController] = useState<RendererSession | null>(null);
  useEffect(() => {
    let disposed = false;
    let captured: Owner | null = null;
    const close = (record: Owner) => record.closing ??= record.controller.close();
    const previous = active.current;
    if (previous?.source === source && !previous.closing) {
      captured = previous; captured.owners++; setController(captured.controller);
    } else {
      setController(null);
      const job = lifecycle.current.then(async () => {
        if (disposed) return;
        if (active.current) await close(active.current);
        if (disposed) return;
        captured = { source, controller: createA2uiSession(source), owners: 1, closing: null };
        active.current = captured; setController(captured.controller);
      });
      lifecycle.current = job.catch(() => {});
    }
    return () => {
      disposed = true;
      if (!captured) return;
      const record = captured; record.owners--;
      // StrictMode immediately reattaches effects; don't restart a one-shot source.
      queueMicrotask(() => {
        if (record.owners === 0) void close(record).catch(() => {});
      });
    };
  }, [source]);
  return useSyncExternalStore(controller?.subscribe ?? noSubscription,
    controller?.getSnapshot ?? emptySnapshot, emptySnapshot);
}

/** Compatible URL hook; direct consumers supply a LiveSession explicitly. */
export function useA2uiSession(url: string, resourceBaseUrl?: string): Readonly<A2uiSession> {
  const source = useMemo(() => createLocalHubSession(url), [url]);
  const snapshot = useA2uiSource(source);
  return useMemo(() => ({ ...snapshot, endpoint: url, resourceBaseUrl }), [snapshot, url, resourceBaseUrl]);
}

export function SurfaceView({ session, surfaceId, fallback = null }: {
  session: A2uiSession; surfaceId: string; fallback?: ReactNode;
}) {
  const surface = session.processor?.model.surfacesMap.get(surfaceId);
  return <div className="omochi-surface a2ui-light">
    <ResourceContext.Provider value={{ endpoint: session.endpoint, surfaceId, baseUrl: session.resourceBaseUrl, scopedImages: session.images !== null, imageScope: session.images?.scope(surfaceId) }}><MarkdownContext.Provider value={renderMarkdown}>
      {surface ? <A2uiSurface key={surfaceId} surface={surface} /> : fallback}
    </MarkdownContext.Provider></ResourceContext.Provider>
  </div>;
}
