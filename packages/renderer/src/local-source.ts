import {
  SessionFault,
  applyToRenderer,
  splitSurfaceId,
  type ApplyResult,
  type ClientMessage,
  type LiveEvent,
  type LiveSession,
  type RenderEvent,
  type ServiceView,
  type SurfaceLease,
  type Unsubscribe,
} from '@omochi-ui/core/browser';
import { isA2uiMessage, messageSurfaceId } from './messages.js';
import { NativeImages } from './native-images.js';

interface LocalLease {
  lease: SurfaceLease;
  controller: AbortController;
  service: string;
  handle: string | null;
}

/** Existing same-origin WS, SSE roster and native media policy, without direct-mode limits. */
class LocalHubSession implements LiveSession {
  readonly imagePolicy = 'native' as const;
  private phase: 'idle' | 'started' | 'stopped' = 'idle';
  private apply: ((event: RenderEvent) => ApplyResult) | null = null;
  private readonly listeners = new Set<(event: LiveEvent) => void>();
  private readonly leases = new Map<string, LocalLease>();
  private readonly surfaceLeases = new Map<string, string>();
  private readonly services = new Set<string>();
  private roster: readonly ServiceView[] = [];
  private status: LiveEvent & { kind: 'status' };
  private socket: WebSocket | null = null;
  private events: EventSource | null = null;
  private images: NativeImages | null = null;
  private epoch = 0;
  private sequence = 0;
  private resyncs = 0;
  private reconnect: ReturnType<typeof setTimeout> | null = null;
  constructor(private readonly url: string) {
    this.status = { kind: 'status', sourceId: url, state: 'Connecting…' };
  }
  subscribe(listener: (event: LiveEvent) => void): Unsubscribe {
    this.listeners.add(listener);
    for (const event of [
      this.status,
      { kind: 'images', enabled: this.images !== null } as const,
      { kind: 'services', services: this.roster } as const,
    ])
      this.observe(listener, event);
    return () => this.listeners.delete(listener);
  }
  attachRenderer(apply: (event: RenderEvent) => ApplyResult): void {
    if (this.phase !== 'idle' || this.apply)
      throw new SessionFault('InvalidState');
    this.apply = apply;
  }
  async start(): Promise<void> {
    if (this.phase === 'stopped' || !this.apply)
      throw new SessionFault('InvalidState');
    if (this.phase === 'started') return;
    this.phase = 'started';
    this.connect();
    // Plain/stdio servers have no roster channel; lack of SSE keeps standalone mode.
    if (typeof EventSource === 'function') {
      const url = new URL(
        this.url,
        globalThis.location?.href ?? 'http://localhost',
      );
      url.protocol = url.protocol === 'wss:' ? 'https:' : 'http:';
      url.pathname = '/events';
      url.search = '';
      url.hash = '';
      this.events = new EventSource(url.href);
      this.events.addEventListener('services', (event) => {
        try {
          const value: unknown = JSON.parse(
            (event as MessageEvent<string>).data,
          );
          if (!Array.isArray(value)) return;
          const roster: ServiceView[] = [];
          for (const entry of value) {
            if (
              !entry ||
              typeof entry !== 'object' ||
              typeof entry.id !== 'string' ||
              typeof entry.name !== 'string' ||
              !['connected', 'disconnected', 'error'].includes(entry.status)
            )
              return;
            const node = entry.node ?? {
              id: this.url,
              name: 'Local',
              local: true,
            };
            if (
              !node ||
              typeof node.id !== 'string' ||
              typeof node.name !== 'string' ||
              typeof node.local !== 'boolean'
            )
              return;
            roster.push({
              id: entry.id,
              name: entry.name,
              node,
              status: entry.status,
              ...(typeof entry.detail === 'string'
                ? { detail: entry.detail }
                : {}),
            });
          }
          this.roster = roster;
          this.emit({ kind: 'services', services: roster });
        } catch {
          /* Wait for the next complete roster. */
        }
      });
    }
  }
  setServices(): void {
    /* Legacy WS already subscribes to all services. */
  }
  async retry(
    target: { gatewayId: string } | { service: string },
  ): Promise<void> {
    if (this.phase !== 'started' || 'service' in target)
      throw new SessionFault('InvalidState');
    if (target.gatewayId !== this.url)
      throw new SessionFault('SourceUnavailable');
    this.disconnect();
    this.resyncs = 0;
    this.connect();
  }
  async action(id: string, message: ClientMessage): Promise<void> {
    const captured = this.leases.get(id);
    if (
      !captured ||
      captured.lease.signal.aborted ||
      this.socket?.readyState !== WebSocket.OPEN
    )
      throw new SessionFault('ScopeExpired');
    const body = 'action' in message ? message.action : message.error;
    if (body.surfaceId !== captured.lease.surfaceId)
      throw new SessionFault('ScopeExpired');
    // Keep original native/standalone Surface IDs and raw client form metadata.
    this.socket.send(JSON.stringify(message));
  }
  async image(id: string, uri: string, signal: AbortSignal): Promise<Response> {
    const captured = this.leases.get(id);
    if (
      !captured ||
      captured.lease.signal.aborted ||
      !captured.handle ||
      !this.images
    )
      throw new SessionFault('ScopeExpired');
    const response = await this.images.fetch(
      captured.handle,
      uri,
      AbortSignal.any([signal, captured.lease.signal]),
    );
    if (captured.lease.signal.aborted) {
      await response.body?.cancel();
      throw new SessionFault('ScopeExpired');
    }
    return response;
  }
  async stop(): Promise<void> {
    if (this.phase === 'stopped') return;
    this.phase = 'stopped';
    this.disconnect();
    this.events?.close();
    this.events = null;
    this.setStatus('Disconnected');
  }
  private connect(): void {
    if (this.socket || this.phase !== 'started') return;
    this.setStatus('Connecting…');
    const url = new URL(
      this.url,
      globalThis.location?.href ?? 'http://localhost',
    );
    url.searchParams.set('images', '1');
    const socket = new WebSocket(url.href);
    this.socket = socket;
    this.epoch++;
    socket.onopen = () => {
      this.sequence = 0;
      this.setStatus('Connected');
    };
    socket.onmessage = (event) => {
      try {
        const message: unknown = JSON.parse(String(event.data));
        if (this.images) {
          if (!message || typeof message !== 'object' || Array.isArray(message))
            throw new SessionFault('ProtocolError');
          const envelope = message as Record<string, unknown>;
          if (envelope.omochi !== 'images/1')
            throw new SessionFault('ProtocolError');
          if (envelope.kind === 'image') {
            this.images.response(envelope);
            return;
          }
          if (
            envelope.kind !== 'update' ||
            envelope.sequence !== this.sequence + 1
          )
            throw new SessionFault('ProtocolError');
          this.sequence++;
          try {
            if (envelope.message !== null) {
              if (
                !isA2uiMessage(envelope.message) ||
                (envelope.handle !== null &&
                  typeof envelope.handle !== 'string')
              )
                throw new SessionFault('ProtocolError');
              this.update(envelope.message, envelope.handle as string | null);
            }
          } finally {
            // ACK application of each incremental frame, never wait for global snapshotEnd.
            socket.send(
              JSON.stringify({
                omochi: 'images/1',
                kind: 'ack',
                sequence: this.sequence,
              }),
            );
          }
          return;
        }
        if (
          !isA2uiMessage(message) &&
          message &&
          typeof message === 'object' &&
          'omochi' in message &&
          'kind' in message &&
          message.kind === 'hello'
        ) {
          if (message.omochi === 'resources/1') return;
          if (message.omochi === 'images/1') {
            this.images = new NativeImages(this.url, (value) => {
              if (socket.readyState === WebSocket.OPEN)
                socket.send(JSON.stringify(value));
            });
            this.emit({ kind: 'images', enabled: true });
            return;
          }
        }
        if (!isA2uiMessage(message)) {
          this.setStatus(this.status.state, {
            code: 'ProtocolError',
            message: 'expected one A2UI v0.9 server message',
          });
          return;
        }
        this.update(message, null);
      } catch (error) {
        this.setStatus(
          this.status.state,
          error instanceof SessionFault
            ? error
            : new SessionFault('ProtocolError'),
        );
      }
    };
    socket.onerror = () =>
      this.setStatus(this.status.state, new SessionFault('NetworkUnavailable'));
    socket.onclose = (event) => {
      const scoped = this.images !== null;
      this.socket = null;
      this.retract();
      const terminal =
        scoped &&
        (event.code === 1009 || (event.code === 1013 && ++this.resyncs > 1));
      this.setStatus(
        `Disconnected${event.reason ? `: ${event.reason}` : ''}`,
        terminal
          ? {
              code: 'CapacityExceeded',
              message: 'Rendering capacity exceeded. Reload to retry.',
            }
          : undefined,
      );
      if (!terminal && this.phase === 'started')
        this.reconnect = setTimeout(() => {
          this.reconnect = null;
          this.connect();
        }, 1000);
    };
  }
  private update(
    message: Parameters<typeof messageSurfaceId>[0],
    handle: string | null,
  ): void {
    // Service-owned fields cannot become transport envelope metadata. The latest
    // processor also rejects these extra fields instead of stripping them.
    if ('createSurface' in message)
      message = {
        version: message.version,
        createSurface: message.createSurface,
      };
    else if ('updateComponents' in message)
      message = {
        version: message.version,
        updateComponents: message.updateComponents,
      };
    else if ('updateDataModel' in message)
      message = {
        version: message.version,
        updateDataModel: message.updateDataModel,
      };
    else
      message = {
        version: message.version,
        deleteSurface: message.deleteSurface,
      };
    const surfaceId = messageSurfaceId(message);
    if (typeof surfaceId !== 'string') throw new SessionFault('ProtocolError');
    const service = splitSurfaceId(surfaceId)?.channelId ?? '__standalone__';
    this.services.add(service);
    let id = this.surfaceLeases.get(surfaceId);
    let captured = id ? this.leases.get(id) : undefined;
    if (
      'deleteSurface' in message ||
      'createSurface' in message ||
      (captured && captured.handle !== handle)
    ) {
      if (captured) {
        this.leases.delete(captured.lease.id);
        captured.controller.abort();
      }
      this.surfaceLeases.delete(surfaceId);
      captured = undefined;
      id = undefined;
    }
    if (!('deleteSurface' in message) && !captured) {
      const controller = new AbortController();
      const lease = Object.freeze({
        id: crypto.randomUUID(),
        surfaceId,
        signal: controller.signal,
      });
      captured = { lease, controller, service, handle };
      this.leases.set(lease.id, captured);
      this.surfaceLeases.set(surfaceId, lease.id);
    }
    const result = applyToRenderer(this.apply!, {
      kind: 'update',
      service,
      epoch: this.epoch,
      message,
      ...(captured ? { lease: captured.lease } : {}),
    });
    if (!result.ok) {
      this.removeService(service);
      throw new SessionFault('RenderError');
    }
  }
  private disconnect(): void {
    if (this.reconnect) clearTimeout(this.reconnect);
    this.reconnect = null;
    const socket = this.socket;
    this.socket = null;
    if (socket) {
      socket.onopen = null;
      socket.onmessage = null;
      socket.onerror = null;
      socket.onclose = null;
      socket.close();
    }
    this.retract();
  }
  private removeService(service: string): void {
    for (const [id, captured] of this.leases)
      if (captured.service === service) {
        this.leases.delete(id);
        this.surfaceLeases.delete(captured.lease.surfaceId);
        captured.controller.abort();
      }
    if (
      this.apply &&
      !applyToRenderer(this.apply, {
        kind: 'remove',
        service,
        epoch: this.epoch,
        reason: 'Source disconnected',
      }).ok
    ) {
      this.setStatus(this.status.state, new SessionFault('RenderError'));
    }
    this.services.delete(service);
  }
  private retract(): void {
    for (const service of this.services) this.removeService(service);
    this.images?.close();
    this.images = null;
    this.emit({ kind: 'images', enabled: false });
  }
  private setStatus(
    state: string,
    error?: { code: SessionFault['code']; message: string },
  ): void {
    this.status = {
      kind: 'status',
      sourceId: this.url,
      state,
      ...(error ? { error } : {}),
    };
    this.emit(this.status);
  }
  private observe(
    listener: (event: LiveEvent) => void,
    event: LiveEvent,
  ): void {
    try {
      listener(event);
    } catch {
      /* Independent observer. */
    }
  }
  private emit(event: LiveEvent): void {
    for (const listener of this.listeners) this.observe(listener, event);
  }
}

export function createLocalHubSession(url: string): LiveSession {
  return new LocalHubSession(url);
}
