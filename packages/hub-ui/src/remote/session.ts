import { parseServices } from '../services.js';
import {
  SessionFault,
  applyToRenderer,
  type A2uiMessage,
  type ApplyResult,
  type ClientMessage,
  type LiveEvent,
  type LiveSession,
  type RenderEvent,
  type SurfaceLease,
} from '@omochi-ui/core/browser';
import type {
  BrowserTransport,
  Endpoint,
  Connection,
  Stream,
  DownstreamState,
} from '@omochi-ui/hub-transport';
import { boardFields, validateBoardInput } from '../board-codec.js';
import { BoardFault } from '../board-repository.js';
import type { Dashboard, DashboardInput } from '../contracts.js';
import { address, type SavedShare } from './link.js';

export interface SharedPermissions {
  interact: boolean;
  editLayout: boolean;
}
export interface SharedView {
  board: Dashboard;
  permissions: SharedPermissions;
}
export interface RemoteSessionOptions {
  source: Pick<SavedShare, 'hubId' | 'grantId' | 'relayUrls'>;
  secretHex: string;
  owner?: boolean;
  publicSecret?: string;
  /** Called only after the complete authoritative snapshot was applied. */
  onView(view: SharedView): void;
}
function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value))
    throw new SessionFault('ProtocolError');
  return value as Record<string, unknown>;
}
function dashboard(value: unknown): Dashboard {
  const row = boardFields(value, ['id', 'revision', 'name', 'panels']);
  if (
    typeof row.id !== 'string' ||
    !row.id.trim() ||
    row.id.length > 100 ||
    !Number.isSafeInteger(row.revision) ||
    Number(row.revision) < 1
  )
    throw new SessionFault('ProtocolError');
  return {
    id: row.id,
    revision: Number(row.revision),
    ...validateBoardInput({ name: row.name, panels: row.panels }),
  };
}
export function parseWelcome(value: Record<string, unknown>): SharedView {
  if (
    value.kind !== 'welcome' ||
    Object.keys(value).sort().join(',') !== 'board,kind,permissions'
  )
    throw new SessionFault('ProtocolError');
  const p = record(value.permissions);
  if (
    Object.keys(p).sort().join(',') !== 'editLayout,interact' ||
    typeof p.interact !== 'boolean' ||
    typeof p.editLayout !== 'boolean'
  )
    throw new SessionFault('ProtocolError');
  return {
    board: dashboard(value.board),
    permissions: { interact: p.interact, editLayout: p.editLayout },
  };
}
async function receive(
  stream: Stream,
  signal: AbortSignal,
): Promise<Record<string, unknown>> {
  const frame = await stream.receive(signal);
  if (!frame) throw new SessionFault('NetworkUnavailable');
  try {
    return record(JSON.parse(frame.json));
  } catch {
    throw new SessionFault('ProtocolError');
  }
}
function ownerServices(value: unknown) {
  const services = parseServices(value);
  if (!services) throw new SessionFault('ProtocolError');
  return services.map((service) => {
    if (!service.node) throw new SessionFault('ProtocolError');
    return { ...service, node: service.node };
  });
}
const operationSignal = (signal: AbortSignal) =>
  AbortSignal.any([signal, AbortSignal.timeout(10000)]);
const maxPendingOperations = 6;
export class RemoteHubSession implements LiveSession {
  readonly imagePolicy = 'hub' as const;
  private readonly options: RemoteSessionOptions;
  private readonly transport: BrowserTransport;
  private readonly listeners = new Set<(event: LiveEvent) => void>();
  private apply: ((event: RenderEvent) => ApplyResult) | null = null;
  private running: Promise<void> | null = null;
  private endpoint: Endpoint | null = null;
  private connection: Connection | null = null;
  private readonly lifetime = new AbortController();
  private scope = new AbortController();
  private leases = new Map<string, SurfaceLease>();
  private leaseStops = new Map<string, AbortController>();
  private view: SharedView | null = null;
  private wireState: DownstreamState | null = null;
  private epoch = 0;
  private pending = 0;
  private retryNow: (() => void) | null = null;
  constructor(options: RemoteSessionOptions, transport: BrowserTransport) {
    this.options = { ...options, source: structuredClone(options.source) };
    this.transport = transport;
  }
  subscribe(listener: (event: LiveEvent) => void) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  private emit(event: LiveEvent) {
    for (const listener of this.listeners) {
      try {
        listener(event);
      } catch {
        /* Independent observer. */
      }
    }
  }
  private status(state: string, error?: SessionFault) {
    this.emit({
      kind: 'status',
      sourceId: this.options.source.hubId,
      state,
      error,
    });
  }
  attachRenderer(apply: (event: RenderEvent) => ApplyResult): void {
    if (this.apply || this.running) throw new SessionFault('InvalidState');
    this.apply = apply;
  }
  start(): Promise<void> {
    if (!this.apply || this.running || this.lifetime.signal.aborted)
      return Promise.reject(new SessionFault('InvalidState'));
    this.emit({ kind: 'images', enabled: true });
    this.running = this.loop();
    return Promise.resolve();
  }
  setServices(_ordered: readonly string[]): void {
    /* Saved shared board controls membership on the hub. */
  }
  async retry(): Promise<void> {
    this.retryNow?.();
  }
  private retract() {
    this.wireState?.close();
    this.wireState = null;
    this.scope.abort();
    this.scope = new AbortController();
    this.leases.clear();
    this.leaseStops.clear();
    if (this.apply)
      applyToRenderer(this.apply, {
        kind: 'remove',
        service: 'shared',
        epoch: this.epoch,
        reason: 'Connection ended',
      });
    this.emit({ kind: 'services', services: [] });
    this.view = null;
  }
  private publishServices(view: SharedView) {
    if (this.options.owner) return;
    const present = new Set(
      [...this.leases.values()].map((l) => l.surfaceId.split('/')[0]!),
    );
    this.emit({
      kind: 'services',
      services: [
        ...new Set(
          view.board.panels.map(
            (p) => `${p.reference.nodeId}~${p.reference.serviceId}`,
          ),
        ),
      ].map((id) => ({
        id,
        name: id.split('~')[1]!,
        node: { id: id.split('~')[0]!, name: id.split('~')[0]!, local: false },
        status: present.has(id) ? 'connected' : 'disconnected',
      })),
    });
  }
  private async pause(milliseconds: number) {
    if (this.lifetime.signal.aborted) return;
    await new Promise<void>((resolve) => {
      let timer: ReturnType<typeof setTimeout>;
      const finish = () => {
        clearTimeout(timer);
        this.lifetime.signal.removeEventListener('abort', finish);
        this.retryNow = null;
        resolve();
      };
      timer = setTimeout(finish, milliseconds);
      this.retryNow = finish;
      this.lifetime.signal.addEventListener('abort', finish, { once: true });
    });
  }
  private async loop(): Promise<void> {
    let failures = 0;
    while (!this.lifetime.signal.aborted) {
      let heartbeat: ReturnType<typeof setInterval> | undefined;
      try {
        this.status('Connecting…');
        this.endpoint = await this.transport.openEndpoint(
          this.options.secretHex,
          this.options.source.relayUrls,
        );
        const connection = await this.endpoint.connect(
          address(this.options.source),
          this.options.owner ? 'omochi/owner/1' : 'omochi/dashboard/2',
          operationSignal(this.lifetime.signal),
        );
        this.connection = connection;
        if (connection.remoteId !== this.options.source.hubId)
          throw new SessionFault('NotAuthorized');
        let stream: Stream;
        let first: Record<string, unknown>;
        try {
          stream = await connection.open(operationSignal(this.lifetime.signal));
          const hello = {
            version: this.options.owner ? 1 : 2,
            grantId: this.options.source.grantId,
            ...(this.options.publicSecret
              ? { secret: this.options.publicSecret }
              : {}),
          };
          await stream.send(
            JSON.stringify(hello),
            operationSignal(this.lifetime.signal),
          );
          first = await receive(stream, operationSignal(this.lifetime.signal));
        } catch (error) {
          if (
            (
              await Promise.race([
                connection.closed(),
                new Promise<{ code: string }>((resolve) =>
                  setTimeout(() => resolve({ code: '0' }), 50),
                ),
              ])
            ).code === '1'
          )
            throw new SessionFault('NotAuthorized');
          throw error;
        }
        if (this.options.owner && first.kind !== 'ownerWelcome')
          throw new SessionFault('ProtocolError');
        const view: SharedView = this.options.owner
          ? {
              board: { id: '', revision: 1, name: '', panels: [] },
              permissions: { interact: true, editLayout: true },
            }
          : parseWelcome(first);
        if (this.options.owner) {
          const services = ownerServices(first.services);
          if (Object.keys(first).sort().join(',') !== 'kind,services')
            throw new SessionFault('ProtocolError');
          this.emit({ kind: 'services', services });
        }
        const admitted = new Set(
          view.board.panels.map(
            (panel) =>
              `${panel.reference.nodeId}~${panel.reference.serviceId}/${panel.reference.surfaceId}`,
          ),
        );
        const attempt = ++this.epoch;
        const scope = this.scope.signal;
        const isCurrent = () =>
          this.connection === connection &&
          !scope.aborted &&
          !this.lifetime.signal.aborted;
        let heartbeatPending = false;
        const wireState = this.transport.createDownstreamState(
          this.options.owner ? 'owner' : 'share',
          view.permissions,
        );
        this.wireState = wireState;
        let ready = false,
          bytes = 0;
        let messages: A2uiMessage[] = [];
        heartbeat = setInterval(() => {
          // Local saturation is not a failed heartbeat. Retry on the next tick.
          if (
            !isCurrent() ||
            heartbeatPending ||
            this.pending >= maxPendingOperations
          )
            return;
          heartbeatPending = true;
          void this.rpc({ op: 'ping' }, scope, false)
            .then((reply) => {
              if (isCurrent() && this.options.owner) {
                const services = ownerServices(reply.services);
                this.emit({ kind: 'services', services });
              }
            })
            .catch(() => {
              if (isCurrent()) void connection.close();
            })
            .finally(() => {
              heartbeatPending = false;
            });
        }, 10000);
        while (!this.lifetime.signal.aborted) {
          const frame = await receive(
            stream,
            ready
              ? this.lifetime.signal
              : operationSignal(this.lifetime.signal),
          );
          const accepted = wireState.accept(frame);
          if (frame.kind === 'message') {
            const message = frame.message as A2uiMessage;
            const surface = accepted.surface!;
            if (!this.options.owner && !admitted.has(surface))
              throw new SessionFault('ProtocolError');
            for (const id of accepted.invalidatedLeases) {
              this.leaseStops.get(id)?.abort();
              this.leaseStops.delete(id);
              this.leases.delete(id);
            }
            let lease: SurfaceLease | undefined;
            if (accepted.lease) {
              let cancel = this.leaseStops.get(accepted.lease);
              if (!cancel) {
                cancel = new AbortController();
                this.leaseStops.set(accepted.lease, cancel);
              }
              lease = {
                id: accepted.lease,
                surfaceId: surface,
                signal: AbortSignal.any([this.scope.signal, cancel.signal]),
              };
              this.leases.set(lease.id, lease);
            }
            if (ready) {
              const result = applyToRenderer(this.apply!, {
                kind: 'update',
                service: 'shared',
                epoch: attempt,
                message,
                lease,
              });
              if (result.ok === false) throw new SessionFault(result.code);
              this.publishServices(view);
            } else {
              bytes += new TextEncoder().encode(JSON.stringify(message)).length;
              if (bytes > 16 * 1024 * 1024)
                throw new SessionFault('CapacityExceeded');
              messages.push(message);
            }
          } else if (frame.kind === 'snapshotEnd' && !ready) {
            const result = applyToRenderer(this.apply!, {
              kind: 'commit',
              service: 'shared',
              epoch: attempt,
              messages,
              leases: [...this.leases.values()],
            });
            if (result.ok === false) throw new SessionFault(result.code);
            messages = [];
            this.view = view;
            ready = true;
            failures = 0;
            this.publishServices(view);
            this.options.onView(structuredClone(view));
            this.status('Connected');
          } else throw new SessionFault('ProtocolError');
          await stream.send(
            JSON.stringify({ ack: accepted.sequence }),
            operationSignal(this.lifetime.signal),
          );
          wireState.acknowledge(accepted.sequence);
        }
      } catch (error) {
        clearInterval(heartbeat);
        if (!this.lifetime.signal.aborted)
          this.status(
            'Disconnected',
            error instanceof SessionFault
              ? error
              : new SessionFault('NetworkUnavailable'),
          );
        const terminal =
          error instanceof SessionFault &&
          !this.transport.isRetryable(error.code);
        if (terminal) {
          this.retract();
          await this.connection?.close();
          await this.endpoint?.close();
          this.connection = null;
          this.endpoint = null;
          await this.pause(2147483647);
          failures = 0;
        }
      } finally {
        clearInterval(heartbeat);
        this.retract();
        await this.connection?.close();
        await this.endpoint?.close();
        this.connection = null;
        this.endpoint = null;
      }
      if (!this.lifetime.signal.aborted)
        await this.pause(
          Math.min(15000, 250 * 2 ** Math.min(failures++, 6)) +
            Math.floor(Math.random() * 250),
        );
    }
  }
  private async rpc(
    input: unknown,
    signal: AbortSignal,
    requireReady = true,
  ): Promise<Record<string, unknown>> {
    const connection = this.connection;
    if (
      !connection ||
      (requireReady && !this.view) ||
      this.lifetime.signal.aborted
    )
      throw new SessionFault('SourceUnavailable');
    if (this.pending >= maxPendingOperations)
      throw new SessionFault('CapacityExceeded');
    this.pending++;
    const combined = operationSignal(
      AbortSignal.any([signal, this.lifetime.signal, this.scope.signal]),
    );
    let stream: Stream | undefined;
    try {
      stream = await connection.open(combined);
      await stream.send(JSON.stringify(input), combined);
      const value = await receive(stream, combined);
      if (value.kind === 'error') {
        if (
          [
            'Conflict',
            'NotFound',
            'InvalidBoard',
            'StorageUnavailable',
          ].includes(String(value.code))
        )
          throw new BoardFault(
            value.code as
              'Conflict' | 'NotFound' | 'InvalidBoard' | 'StorageUnavailable',
          );
        throw new SessionFault('NotAuthorized');
      }
      return value;
    } finally {
      this.pending--;
      await stream?.cancel();
    }
  }
  async control(command: unknown): Promise<unknown> {
    if (!this.options.owner) throw new SessionFault('NotAuthorized');
    const reply = await this.rpc(
      { op: 'control', command },
      this.lifetime.signal,
    );
    if (reply.kind !== 'result') throw new SessionFault('ProtocolError');
    return reply.value;
  }
  async action(lease: string, message: ClientMessage): Promise<void> {
    if (!this.view?.permissions.interact || !this.leases.has(lease))
      throw new SessionFault('NotAuthorized');
    await this.rpc({ op: 'action', lease, message }, this.scope.signal);
  }
  async updateBoard(
    revision: number,
    input: DashboardInput,
  ): Promise<Dashboard> {
    if (!this.view?.permissions.editLayout)
      throw new SessionFault('NotAuthorized');
    const reply = await this.rpc(
      { op: 'layout', value: { ...input, revision } },
      this.scope.signal,
    );
    if (reply.kind !== 'board') throw new SessionFault('ProtocolError');
    return dashboard(reply.board);
  }
  async image(
    lease: string,
    uri: string,
    signal: AbortSignal,
  ): Promise<Response> {
    if (
      !this.view ||
      !this.leases.has(lease) ||
      this.pending >= maxPendingOperations
    )
      throw new SessionFault('ScopeExpired');
    const connection = this.connection;
    if (!connection) throw new SessionFault('SourceUnavailable');
    const combined = AbortSignal.any([
      signal,
      this.lifetime.signal,
      this.scope.signal,
      this.leases.get(lease)!.signal,
      AbortSignal.timeout(25000),
    ]);
    this.pending++;
    let stream: Stream | undefined;
    try {
      stream = await connection.open(operationSignal(combined));
      await stream.send(
        JSON.stringify({ op: 'image', lease, uri }),
        operationSignal(combined),
      );
      const parts: Uint8Array<ArrayBuffer>[] = [];
      let size = 0,
        mime = '',
        width = 0,
        height = 0;
      for (;;) {
        const value = await receive(stream, operationSignal(combined));
        if (value.kind === 'imageEnd') break;
        if (
          value.kind !== 'imageChunk' ||
          typeof value.data !== 'string' ||
          value.data.length > 32768 ||
          !/^[A-Za-z0-9+/]*={0,2}$/.test(value.data)
        )
          throw new SessionFault('InvalidImage');
        const chunk = Uint8Array.from(atob(value.data), (c) => c.charCodeAt(0));
        size += chunk.length;
        if (
          size > 8 * 1024 * 1024 ||
          !Number.isSafeInteger(value.width) ||
          !Number.isSafeInteger(value.height)
        )
          throw new SessionFault('InvalidImage');
        if (
          mime &&
          (mime !== value.mime ||
            width !== value.width ||
            height !== value.height)
        )
          throw new SessionFault('InvalidImage');
        mime = String(value.mime);
        width = Number(value.width);
        height = Number(value.height);
        parts.push(chunk);
      }
      combined.throwIfAborted();
      return new Response(new Blob(parts, { type: mime }), {
        headers: {
          'Content-Type': mime,
          'x-omochi-image-width': String(width),
          'x-omochi-image-height': String(height),
        },
      });
    } finally {
      this.pending--;
      await stream?.cancel();
    }
  }
  async stop(): Promise<void> {
    this.lifetime.abort();
    this.retryNow?.();
    this.retract();
    await this.connection?.close();
    await this.endpoint?.close();
    await this.running;
    this.listeners.clear();
  }
}
