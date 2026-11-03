import { MessageProcessor, type A2uiMessage } from '@a2ui/web_core/v0_9';
import type { ReactComponentImplementation } from '@a2ui/react/v0_9';
import { SessionFault, type ApplyResult, type SessionErrorCode, type LiveEvent, type LiveSession, type RenderEvent, type ServiceView, type SurfaceLease, type Unsubscribe } from '@omochi-ui/core/browser';
import { BrowserImages } from './images.js';
import { createResourceCatalog } from './resources.js';
import { createActionMessage, messageSurfaceId } from './messages.js';
import { applyDirectCreate, applyDirectDataPatch } from './data-patch.js';

export type Processor = MessageProcessor<ReactComponentImplementation>;
export interface A2uiSession {
  processor: Processor | null;
  surfaceIds: readonly string[];
  services: readonly ServiceView[];
  status: string;
  error: string | null;
  errorCode?: SessionErrorCode | null;
  images: BrowserImages | null;
  endpoint: string;
  resourceBaseUrl?: string;
}
export interface RendererSession {
  getSnapshot(): Readonly<A2uiSession>;
  subscribe(changed: () => void): Unsubscribe;
  close(): Promise<void>;
}

/** One authoritative synchronous processor sink; observers never apply source data. */
export function createA2uiSession(source: LiveSession): RendererSession {
  const listeners = new Set<() => void>();
  const services = new Map<string, Set<string>>();
  const leases = new Map<string, SurfaceLease>();
  const states = new Map<string, Extract<LiveEvent, { kind: 'status' }>>();
  let images: BrowserImages | null = null;
  let applying = false;
  let closed = false;
  let stopping = false;
  let closing: Promise<void> | null = null;
  let roster: readonly ServiceView[] = [];
  let endpoint = source.imagePolicy === 'sdk-png' ? 'browser:gateway' : '';
  let status = source.imagePolicy === 'sdk-png' ? 'Disconnected' : 'Connecting…';
  let error: string | null = null;
  let errorCode: SessionErrorCode | null = null;
  const catalog = createResourceCatalog(source.imagePolicy);
  const processor: Processor = new MessageProcessor([catalog], action => {
    if (applying || stopping || closed) return;
    const lease = leases.get(action.surfaceId);
    if (!lease || lease.signal.aborted) return;
    void source.action(lease.id, createActionMessage(action, processor.getClientDataModel())).catch(() => {
      if (!closed && !lease.signal.aborted) { error = new SessionFault('SourceUnavailable').message; publish(); }
    });
  });
  let snapshot: Readonly<A2uiSession> = {
    processor, surfaceIds: [], services: roster, status, error, errorCode, images, endpoint,
  };
  function publish(): void {
    if (applying) return;
    snapshot = { processor: closed ? null : processor, surfaceIds: [...processor.model.surfacesMap.keys()],
      services: roster, status, error, errorCode, images, endpoint };
    for (const listener of listeners) { try { listener(); } catch { /* Independent React/legacy observer. */ } }
  }
  function forgetSurface(id: string): void {
    leases.delete(id);
    images?.update(id, null);
    // The model removes its entry before disposal callbacks, even when one throws.
    if (processor.model.surfacesMap.has(id)) {
      try { processor.model.deleteSurface(id); }
      catch { error = new SessionFault('RenderError').message; }
    }
  }
  function retract(service: string): void {
    for (const id of services.get(service) ?? []) forgetSurface(id);
    services.delete(service);
  }
  function bind(lease: SurfaceLease): void {
    leases.set(lease.surfaceId, lease);
    images?.update(lease.surfaceId, lease.id);
  }
  function checkCatalog(message: A2uiMessage): void {
    if (source.imagePolicy !== 'sdk-png' || !('updateComponents' in message)) return;
    for (const component of message.updateComponents.components) {
      if (!catalog.components.has(component.component)) throw new SessionFault('RenderError');
    }
  }
  function apply(event: RenderEvent): ApplyResult {
    if (closed) return { ok: false, code: 'RenderError' };
    applying = true;
    try {
      if (event.kind === 'remove') retract(event.service);
      else if (event.kind === 'commit') {
        const previous = services.get(event.service) ?? new Set<string>();
        const attempted = event.messages.filter((message): message is A2uiMessage & { createSurface: { surfaceId: string } } => 'createSurface' in message)
          .map(messageSurfaceId);
        // Keep the union before applying: a later remove can clean a partial commit.
        services.set(event.service, new Set([...previous, ...attempted]));
        for (const id of previous) forgetSurface(id);
        for (const lease of event.leases) bind(lease);
        for (const message of event.messages) checkCatalog(message);
        processor.processMessages([...event.messages]);
        services.set(event.service, new Set(attempted));
      } else {
        const message = event.message;
        if (source.imagePolicy === 'sdk-png' && !event.budget) throw new SessionFault('RenderError');
        checkCatalog(message);
        const id = messageSurfaceId(message);
        let ids = services.get(event.service);
        if (!ids) services.set(event.service, ids = new Set());
        if ('createSurface' in message) ids.add(id);
        if ('deleteSurface' in message) { images?.update(id, null); leases.delete(id); }
        else if (event.lease) bind(event.lease);
        if (source.imagePolicy === 'sdk-png' && event.budget && 'updateDataModel' in message) applyDirectDataPatch(processor, message, event.budget);
        else if (source.imagePolicy === 'sdk-png' && event.budget && 'createSurface' in message) applyDirectCreate(processor, message, event.budget);
        else processor.processMessages([message]);
        if ('deleteSurface' in message) ids.delete(id);
      }
    } catch (failure) {
      return { ok: false, code: failure instanceof SessionFault && failure.code === 'CapacityExceeded' ? 'CapacityExceeded' : 'RenderError' };
    } finally { applying = false; }
    publish();
    return { ok: true };
  }
  const created = processor.onSurfaceCreated(() => { if (!applying) publish(); });
  const deleted = processor.onSurfaceDeleted(() => { if (!applying) publish(); });
  let unsubscribe: Unsubscribe;
  try {
    source.attachRenderer(apply);
    unsubscribe = source.subscribe(event => {
      if (closed) return;
      if (event.kind === 'images') {
        if (!event.enabled) { images?.close(); images = null; }
        else if (!images) {
          images = new BrowserImages((lease, uri, signal) => source.image(lease, uri, signal));
          for (const lease of leases.values()) images.update(lease.surfaceId, lease.id);
        }
      } else if (event.kind === 'services') roster = event.services;
      else {
        states.set(event.sourceId, event);
        if (source.imagePolicy !== 'sdk-png') { endpoint = event.sourceId; status = event.state; }
        else {
          const values = [...states.values()];
          status = values.some(value => value.state === 'online') ? 'Connected' :
            values.some(value => ['connecting', 'control', 'backoff'].includes(value.state)) ? 'Connecting…' : 'Disconnected';
        }
        const failure = [...states.values()].find(value => value.error)?.error;
        error = failure?.message ?? null; errorCode = failure?.code ?? null;
      }
      publish();
    });
  } catch (failure) {
    processor.model.dispose();
    throw failure;
  }
  void source.start().catch(failure => {
    if (!closed) {
      error = failure instanceof SessionFault ? failure.message : new SessionFault('InvalidState').message;
      errorCode = failure instanceof SessionFault ? failure.code : 'InvalidState';
      status = 'Disconnected'; publish();
    }
  });
  return {
    getSnapshot: () => snapshot,
    subscribe(changed) { listeners.add(changed); return () => listeners.delete(changed); },
    close() {
      if (closing) return closing;
      stopping = true;
      closing = (async () => {
        try { await source.stop(); }
        finally {
          // The sink remains attached through stop's final remove callbacks.
          unsubscribe();
          applying = true;
          for (const service of services.keys()) retract(service);
          for (const id of processor.model.surfacesMap.keys()) forgetSurface(id);
          images?.close(); images = null;
          created.unsubscribe(); deleted.unsubscribe();
          processor.model.dispose();
          closed = true; applying = false; status = 'Disconnected';
          publish(); listeners.clear();
        }
      })();
      return closing;
    },
  };
}
