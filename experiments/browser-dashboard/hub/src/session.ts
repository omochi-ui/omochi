import {
  SessionFault,
  applyToRenderer,
  isValidNodeId,
  jsonBytes,
  jsonStringBytes,
  materializeJson,
  parseControlFrame,
  parseServiceFrame,
  prefixServerMessage,
  routeClientMessage,
  splitRouteId,
  validateClientMessage,
  type A2uiMessage,
  type ApplyResult,
  type ClientMessage,
  type GatewayService,
  type LiveEvent,
  type LiveSession,
  type RenderEvent,
  type ServiceView,
  type SurfaceLease,
  type Unsubscribe,
} from '@omochi-ui/core/browser';
import type {
  BrowserTransport,
  Connection,
  Endpoint,
  ReceivedFrame,
  Stream,
} from '@omochi-ui/browser-transport';
import { endpointFrameBytes } from '@omochi-ui/browser-transport';
import {
  BytePool,
  LIMITS,
  RenderDeliveryPool,
  type Reservation,
} from './budget.js';
import { BoundedSnapshot } from './snapshot.js';
import {
  SurfaceScopes,
  sameServiceScope,
  type ServiceScope,
} from './scopes.js';
import { PngResources } from './resource.js';

export interface GatewayRecord {
  nodeId: string;
  name: string;
  relayUrls: readonly string[];
  enrollment: 'pending' | 'accepted';
  createdAt: number;
}
export type AcceptedGateway = GatewayRecord & { enrollment: 'accepted' };
export interface ProfileSnapshot {
  identity: { secretHex: string; endpointId: string };
  gateways: readonly AcceptedGateway[];
}
type SourceState =
  | 'idle'
  | 'connecting'
  | 'control'
  | 'online'
  | 'backoff'
  | 'terminal'
  | 'stopping'
  | 'stopped';
type ServiceState =
  'absent' | 'awaitingSnapshot' | 'ready' | 'backoff' | 'terminal';
interface GatewayActor {
  record: Readonly<AcceptedGateway>;
  name: string;
  controller: AbortController;
  state: SourceState;
  error?: SessionFault;
  epoch: number;
  job: Promise<void>;
  attempt: GatewayAttempt | null;
  roster: Map<string, GatewayService>;
  services: Map<string, ServiceActor>;
  dirty: boolean;
  reconciling: Promise<void> | null;
}
interface GatewayAttempt {
  controller: AbortController;
  signal: AbortSignal;
  connection: Connection | null;
  control: Stream | null;
  closeCode: string | null;
  failure?: SessionFault;
  ping: { id: string; expires: number; resolve(): void } | null;
  wake: (() => void) | null;
  nextPing: number;
}
interface ServiceActor {
  key: string;
  gateway: GatewayActor;
  generation: string;
  controller: AbortController;
  state: ServiceState;
  error?: SessionFault;
  job: Promise<void>;
  run: ServiceRun | null;
  overflows: number;
}
interface ServiceRun {
  actor: ServiceActor;
  scope: Readonly<ServiceScope>;
  controller: AbortController;
  signal: AbortSignal;
  connection: Connection;
  stream: Stream | null;
  snapshot: BoundedSnapshot | null;
  ready: boolean;
  rendered: boolean;
  failure?: SessionFault;
  retryOverflow: boolean;
  leases: Map<string, SurfaceLease>;
  actions: ActionItem[];
  actionJob: Promise<void> | null;
}
interface ActionItem {
  lease: SurfaceLease;
  json: string;
  reservation: Reservation;
  settled: boolean;
  resolve(): void;
  reject(error: SessionFault): void;
}
type RenderItem =
  | {
      kind: 'commit';
      run: ServiceRun;
      resolve(): void;
      reject(error: SessionFault): void;
    }
  | {
      kind: 'update';
      run: ServiceRun;
      message: A2uiMessage;
      lease?: SurfaceLease;
      reservation: Reservation;
    };

const cancelled = () => new SessionFault('Cancelled');
function asFault(error: unknown): SessionFault {
  return error instanceof SessionFault
    ? error
    : new SessionFault('NetworkUnavailable');
}
function check(signal: AbortSignal): void {
  if (signal.aborted) throw cancelled();
}
function decode(frame: ReceivedFrame | null): unknown {
  if (frame === null) throw new SessionFault('NetworkUnavailable');
  if (
    !Number.isSafeInteger(frame.wireBytes) ||
    frame.wireBytes < 1 ||
    frame.wireBytes > LIMITS.frame
  )
    throw new SessionFault('ProtocolError');
  try {
    return JSON.parse(frame.json);
  } catch {
    throw new SessionFault('ProtocolError');
  }
}
async function deadline<T>(
  ms: number,
  parent: AbortSignal,
  operation: (signal: AbortSignal) => Promise<T>,
): Promise<T> {
  check(parent);
  const controller = new AbortController();
  const expires = Date.now() + ms;
  const timer = setTimeout(() => controller.abort(), ms);
  const signal = AbortSignal.any([parent, controller.signal]);
  try {
    const value = await operation(signal);
    check(parent);
    if (controller.signal.aborted || Date.now() >= expires)
      throw new SessionFault('Timeout');
    return value;
  } catch (error) {
    if (parent.aborted) throw cancelled();
    if (controller.signal.aborted || Date.now() >= expires)
      throw new SessionFault('Timeout');
    throw asFault(error);
  } finally {
    clearTimeout(timer);
  }
}
function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      reject(cancelled());
      return;
    }
    const finish = () => {
      signal.removeEventListener('abort', abort);
      resolve();
    };
    const timer = setTimeout(finish, ms);
    const abort = () => {
      clearTimeout(timer);
      signal.removeEventListener('abort', abort);
      reject(cancelled());
    };
    signal.addEventListener('abort', abort, { once: true });
  });
}
function backoff(failures: number): number {
  return (
    Math.min(30000, 500 * 2 ** Math.min(failures, 6)) *
    (0.8 + Math.random() * 0.4)
  );
}
function surfaceId(message: A2uiMessage): string {
  if ('createSurface' in message) return message.createSurface.surfaceId;
  if ('updateComponents' in message) return message.updateComponents.surfaceId;
  if ('updateDataModel' in message) return message.updateDataModel.surfaceId;
  return message.deleteSurface.surfaceId;
}
function supported(): boolean {
  const agent = globalThis.navigator?.userAgent ?? '';
  const version = /(?:Chrome|Chromium)\/(\d+)/.exec(agent)?.[1];
  return (
    globalThis.isSecureContext === true &&
    /Linux/.test(agent) &&
    !/Android/.test(agent) &&
    Number(version) >= 156 &&
    typeof WebAssembly === 'object' &&
    typeof indexedDB === 'object' &&
    typeof navigator.locks?.request === 'function' &&
    typeof AbortController === 'function' &&
    typeof DecompressionStream === 'function' &&
    typeof createImageBitmap === 'function'
  );
}
function captureProfile(value: ProfileSnapshot): ProfileSnapshot {
  const fail = () => {
    throw new SessionFault('StorageCorrupt');
  };
  if (
    !value ||
    Object.keys(value).sort().join(',') !== 'gateways,identity' ||
    !value.identity ||
    Object.keys(value.identity).sort().join(',') !== 'endpointId,secretHex' ||
    typeof value.identity.secretHex !== 'string' ||
    !isValidNodeId(value.identity.secretHex) ||
    typeof value.identity.endpointId !== 'string' ||
    !isValidNodeId(value.identity.endpointId) ||
    !Array.isArray(value.gateways)
  )
    return fail();
  if (value.gateways.length > LIMITS.gateways)
    throw new SessionFault('CapacityExceeded');
  const ids = new Set<string>();
  const gateways = value.gateways.map((record) => {
    if (
      !record ||
      Object.keys(record).sort().join(',') !==
        'createdAt,enrollment,name,nodeId,relayUrls' ||
      !isValidNodeId(record.nodeId) ||
      ids.has(record.nodeId) ||
      record.enrollment !== 'accepted' ||
      typeof record.name !== 'string' ||
      !record.name.trim() ||
      record.name !== record.name.trim() ||
      record.name.length > 256 ||
      !Number.isSafeInteger(record.createdAt) ||
      record.createdAt < 0 ||
      !Array.isArray(record.relayUrls) ||
      record.relayUrls.length < 1 ||
      record.relayUrls.length > 4
    )
      return fail();
    for (const relay of record.relayUrls) {
      try {
        const url = new URL(relay);
        if (
          typeof relay !== 'string' ||
          relay.length > 2048 ||
          url.protocol !== 'https:' ||
          url.username ||
          url.password ||
          url.search ||
          url.hash
        )
          return fail();
      } catch {
        return fail();
      }
    }
    ids.add(record.nodeId);
    return Object.freeze({
      ...record,
      relayUrls: Object.freeze([...record.relayUrls]),
    });
  });
  return Object.freeze({
    identity: Object.freeze({ ...value.identity }),
    gateways: Object.freeze(gateways),
  });
}

class BrowserSession implements LiveSession {
  readonly imagePolicy = 'sdk-png' as const;
  private readonly profile: ProfileSnapshot;
  private readonly root = new AbortController();
  private readonly gateways = new Map<string, GatewayActor>();
  private readonly listeners = new Set<(event: LiveEvent) => void>();
  private readonly scopes = new SurfaceScopes();
  private readonly resources = new PngResources();
  private readonly snapshots = new BytePool(LIMITS.snapshots);
  private readonly delivery = new RenderDeliveryPool();
  private readonly renderBytes = new BytePool(LIMITS.renderQueueBytes);
  private readonly actionBytes = new BytePool(LIMITS.actionBytes);
  private readonly retrying = new Map<string, Promise<void>>();
  private desired: readonly string[] = [];
  private cachedServices: readonly ServiceView[] = [];
  private phase: 'idle' | 'started' | 'stopping' | 'stopped' = 'idle';
  private apply: ((event: RenderEvent) => ApplyResult) | null = null;
  private applying = false;
  private readonly removals = new Set<ServiceRun>();
  private readonly rendering: RenderItem[] = [];
  private renderCount = 0;
  private renderTimer: ReturnType<typeof setTimeout> | null = null;
  private actionCount = 0;
  private epoch = 0;
  private instance = 0;
  private endpoint: Endpoint | null = null;
  private binding: Promise<Endpoint> | null = null;
  private starting: Promise<void> | null = null;
  private stopping: Promise<void> | null = null;
  constructor(
    profile: ProfileSnapshot,
    private readonly transport: BrowserTransport,
  ) {
    this.profile = captureProfile(profile);
    for (const record of this.profile.gateways)
      this.gateways.set(record.nodeId, this.gateway(record));
  }
  diagnostics() {
    const runs = [...this.gateways.values()].flatMap((gateway) =>
      [...gateway.services.values()]
        .map((actor) => actor.run)
        .filter((run) => run !== null),
    );
    return {
      frameBytes: this.endpoint ? endpointFrameBytes(this.endpoint) : 0,
      snapshotBytes: this.snapshots.usedBytes,
      peakSnapshotBytes: this.snapshots.peakBytes,
      largestServiceSnapshot: Math.max(
        0,
        ...runs.map((run) => run.snapshot?.byteSize ?? 0),
      ),
      deliveryBytes: this.delivery.usedBytes,
      peakDeliveryBytes: this.delivery.peakBytes,
      renderBytes: this.renderBytes.usedBytes,
      peakRenderBytes: this.renderBytes.peakBytes,
      renderCount: this.renderCount,
      actionBytes: this.actionBytes.usedBytes,
      peakActionBytes: this.actionBytes.peakBytes,
      actionCount: this.actionCount,
      leases: this.scopes.size,
      leaseBytes: this.scopes.retainedBytes,
      ...this.resources.budgetDiagnostics,
    };
  }
  subscribe(listener: (event: LiveEvent) => void): Unsubscribe {
    this.listeners.add(listener);
    this.observe(listener, { kind: 'images', enabled: true });
    for (const gateway of this.gateways.values())
      this.observe(listener, this.status(gateway));
    this.observe(listener, { kind: 'services', services: this.cachedServices });
    return () => this.listeners.delete(listener);
  }
  attachRenderer(apply: (event: RenderEvent) => ApplyResult): void {
    if (this.apply || this.phase !== 'idle')
      throw new SessionFault('InvalidState');
    this.apply = apply;
  }
  start(): Promise<void> {
    if (this.phase === 'stopping' || this.phase === 'stopped' || !this.apply)
      return Promise.reject(new SessionFault('InvalidState'));
    if (this.starting) return this.starting;
    this.phase = 'started';
    this.starting = (async () => {
      if (!supported()) throw new SessionFault('UnsupportedBrowser');
      const identity = await this.transport.identityFromSecret(
        this.profile.identity.secretHex,
      );
      if (identity.endpointId !== this.profile.identity.endpointId)
        throw new SessionFault('StorageCorrupt');
      check(this.root.signal);
      for (const gateway of this.gateways.values())
        gateway.job = this.gatewayLoop(gateway);
      globalThis.addEventListener?.('online', this.resume);
      globalThis.addEventListener?.('pagehide', this.pagehide);
      globalThis.document?.addEventListener('visibilitychange', this.resume);
    })();
    return this.starting;
  }
  setServices(ordered: readonly string[]): void {
    if (this.phase === 'stopping' || this.phase === 'stopped')
      throw new SessionFault('InvalidState');
    if (
      !Array.isArray(ordered) ||
      ordered.some((key) => typeof key !== 'string' || !splitRouteId(key))
    )
      throw new SessionFault('ProtocolError');
    const desired = [...new Set(ordered)];
    if (
      desired.length === this.desired.length &&
      desired.every((key, i) => key === this.desired[i])
    )
      return;
    this.desired = Object.freeze(desired);
    for (const gateway of this.gateways.values()) {
      this.reconcile(gateway, true);
    }
    this.publishServices();
  }
  retry(target: { gatewayId: string } | { service: string }): Promise<void> {
    if (this.phase !== 'started')
      return Promise.reject(new SessionFault('InvalidState'));
    if (
      !target ||
      typeof target !== 'object' ||
      Object.keys(target).length !== 1
    )
      return Promise.reject(new SessionFault('ProtocolError'));
    const gatewayTarget = 'gatewayId' in target;
    const key = gatewayTarget
      ? target.gatewayId
      : 'service' in target
        ? target.service
        : null;
    if (
      typeof key !== 'string' ||
      (gatewayTarget ? !isValidNodeId(key) : !splitRouteId(key))
    )
      return Promise.reject(new SessionFault('ProtocolError'));
    const id = `${gatewayTarget ? 'gateway' : 'service'}:${key}`;
    const previous = this.retrying.get(id);
    if (previous) return previous;
    const retry = (async () => {
      await this.starting;
      if (this.phase !== 'started') throw new SessionFault('InvalidState');
      if (gatewayTarget) {
        const old = this.gateways.get(key);
        if (!old) throw new SessionFault('SourceUnavailable');
        old.controller.abort();
        this.retireGateway(old);
        await old.job;
        await old.reconciling;
        if (this.phase !== 'started') throw new SessionFault('InvalidState');
        const next = this.gateway(old.record);
        this.gateways.set(key, next);
        next.job = this.gatewayLoop(next);
      } else {
        const route = splitRouteId(key)!;
        const gateway = this.gateways.get(route.nodeId);
        if (
          !this.desired.slice(0, LIMITS.services).includes(key) ||
          !gateway ||
          gateway.state !== 'online' ||
          !gateway.roster.has(route.serviceId)
        )
          throw new SessionFault('SourceUnavailable');
        const old = gateway.services.get(key);
        if (old) {
          this.stopService(old);
          await old.job;
          if (gateway.services.get(key) === old) gateway.services.delete(key);
        }
        if (this.phase !== 'started') throw new SessionFault('InvalidState');
        this.reconcile(gateway);
      }
      this.publishServices();
    })().finally(() => {
      this.retrying.delete(id);
    });
    this.retrying.set(id, retry);
    return retry;
  }
  action(id: string, message: ClientMessage): Promise<void> {
    try {
      if (this.phase !== 'started' || this.applying)
        throw new SessionFault('ScopeExpired');
      const scoped = this.scopes.require(id, (task) => this.current(task));
      const run = this.runFor(scoped.task)!;
      const validated = validateClientMessage(message);
      const routed = routeClientMessage(validated);
      if (
        !routed ||
        routed.channelId !== run.actor.key ||
        surfaceIdForClient(validated) !== scoped.lease.surfaceId
      )
        throw new SessionFault('ScopeExpired');
      const bytes = jsonBytes(routed.message);
      if (bytes > LIMITS.frame || this.actionCount >= LIMITS.actionCount)
        throw new SessionFault('CapacityExceeded');
      const reservation = this.actionBytes.reserve(bytes);
      let json: string;
      try {
        json = JSON.stringify(routed.message);
      } catch {
        reservation.release();
        throw new SessionFault('ProtocolError');
      }
      this.actionCount++;
      const result = new Promise<void>((resolve, reject) => {
        run.actions.push({
          lease: scoped.lease,
          json,
          reservation,
          settled: false,
          resolve,
          reject,
        });
      });
      this.sendActions(run);
      return result;
    } catch (error) {
      return Promise.reject(asFault(error));
    }
  }
  async image(id: string, uri: string, signal: AbortSignal): Promise<Response> {
    check(signal);
    const scoped = this.scopes.require(id, (task) => this.current(task));
    const connection = this.runFor(scoped.task)!.connection;
    return this.resources.load(
      {
        ...scoped,
        connection,
        current: () => {
          this.scopes.require(id, (task) => this.current(task));
        },
      },
      uri,
      signal,
    );
  }
  stop(): Promise<void> {
    if (this.stopping) return this.stopping;
    this.phase = 'stopping';
    this.root.abort();
    this.scopes.close();
    globalThis.removeEventListener?.('online', this.resume);
    globalThis.removeEventListener?.('pagehide', this.pagehide);
    globalThis.document?.removeEventListener('visibilitychange', this.resume);
    for (const gateway of this.gateways.values()) {
      gateway.controller.abort();
      this.retireGateway(gateway);
    }
    if (this.renderTimer) clearTimeout(this.renderTimer);
    this.renderTimer = null;
    this.stopping = (async () => {
      await this.starting?.catch(() => {});
      await this.resources.close();
      await Promise.all(
        [...this.gateways.values()].map(async (gateway) => {
          await gateway.job;
          await gateway.reconciling;
        }),
      );
      await Promise.allSettled(this.retrying.values());
      await this.binding?.catch(() => {});
      await this.endpoint?.close();
      this.endpoint = null;
      this.phase = 'stopped';
      for (const gateway of this.gateways.values()) {
        gateway.state = 'stopped';
        gateway.error = undefined;
        this.emit(this.status(gateway));
      }
      this.publishServices();
    })();
    return this.stopping;
  }

  private gateway(record: Readonly<AcceptedGateway>): GatewayActor {
    return {
      record,
      name: record.name,
      controller: new AbortController(),
      state: 'idle',
      epoch: 0,
      job: Promise.resolve(),
      attempt: null,
      roster: new Map(),
      services: new Map(),
      dirty: false,
      reconciling: null,
    };
  }
  private async bind(): Promise<Endpoint> {
    if (this.endpoint) return this.endpoint;
    if (!this.binding) {
      const relays = [
        ...new Set(
          this.profile.gateways.flatMap((gateway) => [...gateway.relayUrls]),
        ),
      ];
      this.binding = this.transport
        .openEndpoint(this.profile.identity.secretHex, relays)
        .then(async (endpoint) => {
          if (this.root.signal.aborted) {
            await endpoint.close();
            throw cancelled();
          }
          this.endpoint = endpoint;
          return endpoint;
        })
        .finally(() => {
          this.binding = null;
        });
    }
    return this.binding;
  }
  private async gatewayLoop(actor: GatewayActor): Promise<void> {
    const signal = AbortSignal.any([this.root.signal, actor.controller.signal]);
    let failures = 0;
    while (!signal.aborted) {
      const controller = new AbortController();
      const attempt: GatewayAttempt = {
        controller,
        signal: AbortSignal.any([signal, controller.signal]),
        connection: null,
        control: null,
        closeCode: null,
        ping: null,
        wake: null,
        nextPing: 0,
      };
      actor.attempt = attempt;
      actor.epoch = ++this.epoch;
      let onlineAt = 0;
      let failure = new SessionFault('NetworkUnavailable');
      actor.state = 'connecting';
      actor.error = undefined;
      this.emit(this.status(actor));
      try {
        const endpoint = await this.bind();
        check(attempt.signal);
        // One address per attempt preserves saved relay priority, without truncating the endpoint's union.
        attempt.connection = await deadline(
          10000,
          attempt.signal,
          async (dialSignal) => {
            let last: SessionFault = new SessionFault('NetworkUnavailable');
            for (const relay of actor.record.relayUrls) {
              check(dialSignal);
              try {
                return await endpoint.connect(
                  JSON.stringify({
                    id: actor.record.nodeId,
                    addrs: [{ Relay: relay }],
                  }),
                  'omochi/gateway/4',
                  dialSignal,
                );
              } catch (error) {
                last = asFault(error);
                check(dialSignal);
              }
            }
            throw last;
          },
        );
        const connection = attempt.connection;
        if (connection.remoteId !== actor.record.nodeId)
          throw new SessionFault('ProtocolError');
        const ended = connection.closed().then(
          (close) => {
            attempt.closeCode = close.code;
            if (close.code === '1')
              attempt.failure = new SessionFault('NotAuthorized');
            attempt.controller.abort();
          },
          () => {
            attempt.controller.abort();
          },
        );
        void ended.catch(() => {});
        actor.state = 'control';
        this.emit(this.status(actor));
        const roster = await deadline(
          10000,
          attempt.signal,
          async (helloSignal) => {
            const control = await connection.open(
              { kind: 'control', protocol: 4 },
              helloSignal,
            );
            attempt.control = control;
            const hello = parseControlFrame(
              decode(await control.receive(helloSignal)),
            );
            if (hello.kind !== 'hello' || hello.nodeId !== actor.record.nodeId)
              throw new SessionFault('ProtocolError');
            const roster = parseControlFrame(
              decode(await control.receive(helloSignal)),
            );
            if (roster.kind !== 'services')
              throw new SessionFault('ProtocolError');
            return { name: hello.name, services: roster.services };
          },
        );
        check(attempt.signal);
        actor.name = roster.name;
        actor.roster = new Map(
          roster.services.map((service) => [service.id, service]),
        );
        actor.state = 'online';
        onlineAt = Date.now();
        this.reconcile(actor);
        this.emit(this.status(actor));
        this.publishServices();
        const reading = this.control(actor, attempt).catch((error) => {
          attempt.failure ??= asFault(error);
          controller.abort();
        });
        const heartbeat = this.heartbeat(attempt).catch((error) => {
          attempt.failure ??= asFault(error);
          controller.abort();
        });
        await Promise.all([reading, heartbeat]);
        failure = attempt.failure ?? failure;
      } catch (error) {
        failure = attempt.failure ?? asFault(error);
      } finally {
        controller.abort();
        this.retireGateway(actor);
        await attempt.control?.cancel();
        await attempt.connection?.close();
        await Promise.all(
          [...actor.services.values()].map((service) => service.job),
        );
        await actor.reconciling;
        actor.services.clear();
        if (actor.attempt === attempt) actor.attempt = null;
      }
      if (signal.aborted) break;
      if (attempt.closeCode === '1')
        failure = new SessionFault('NotAuthorized');
      if (failure.code === 'Cancelled')
        failure = new SessionFault('NetworkUnavailable');
      actor.error = failure;
      const terminal = [
        'ProtocolError',
        'NotAuthorized',
        'CapacityExceeded',
        'RenderError',
      ].includes(failure.code);
      actor.state = terminal ? 'terminal' : 'backoff';
      this.emit(this.status(actor));
      this.publishServices();
      if (terminal) return;
      if (onlineAt && Date.now() - onlineAt >= 30000) failures = 0;
      try {
        await sleep(backoff(failures++), signal);
      } catch {
        break;
      }
    }
  }
  private async control(
    actor: GatewayActor,
    attempt: GatewayAttempt,
  ): Promise<void> {
    while (!attempt.signal.aborted) {
      const frame = parseControlFrame(
        decode(await attempt.control!.receive(attempt.signal)),
      );
      check(attempt.signal);
      if (frame.kind === 'services') {
        actor.roster = new Map(
          frame.services.map((service) => [service.id, service]),
        );
        this.reconcile(actor);
        this.publishServices();
      } else if (frame.kind === 'pong') {
        if (!attempt.ping || attempt.ping.id !== frame.id)
          throw new SessionFault('ProtocolError');
        if (Date.now() >= attempt.ping.expires)
          throw new SessionFault('Timeout');
        const ping = attempt.ping;
        attempt.ping = null;
        ping.resolve();
      } else
        throw new SessionFault(
          frame.kind === 'bye' ? 'NetworkUnavailable' : 'ProtocolError',
        );
    }
  }
  private async heartbeat(attempt: GatewayAttempt): Promise<void> {
    while (!attempt.signal.aborted) {
      await new Promise<void>((resolve, reject) => {
        const finish = () => {
          clearTimeout(timer);
          attempt.signal.removeEventListener('abort', abort);
          attempt.wake = null;
          resolve();
        };
        const abort = () => {
          clearTimeout(timer);
          attempt.signal.removeEventListener('abort', abort);
          attempt.wake = null;
          reject(cancelled());
        };
        const timer = setTimeout(finish, 10000);
        attempt.wake = finish;
        attempt.signal.addEventListener('abort', abort, { once: true });
        if (attempt.signal.aborted) abort();
      });
      check(attempt.signal);
      const id = String(
        (attempt.nextPing =
          attempt.nextPing === 0xffffffff ? 1 : attempt.nextPing + 1),
      );
      let resolve!: () => void;
      const pong = new Promise<void>((yes) => {
        resolve = yes;
      });
      attempt.ping = { id, expires: Date.now() + 10000, resolve };
      await deadline(10000, attempt.signal, async (pongSignal) => {
        await deadline(5000, pongSignal, (writeSignal) =>
          attempt.control!.send(
            JSON.stringify({ kind: 'ping', id: Number(id) }),
            writeSignal,
          ),
        );
        await new Promise<void>((yes, no) => {
          const abort = () => {
            pongSignal.removeEventListener('abort', abort);
            no(cancelled());
          };
          pongSignal.addEventListener('abort', abort, { once: true });
          void pong.then(() => {
            pongSignal.removeEventListener('abort', abort);
            yes();
          });
          if (pongSignal.aborted) abort();
        });
      });
    }
  }
  private readonly resume = () => {
    if (globalThis.document?.visibilityState === 'hidden') return;
    for (const actor of this.gateways.values()) {
      const attempt = actor.attempt;
      if (!attempt || actor.state !== 'online') continue;
      if (attempt.ping && Date.now() >= attempt.ping.expires) {
        attempt.failure = new SessionFault('Timeout');
        attempt.controller.abort();
        this.retireGateway(actor);
      } else if (!attempt.ping) attempt.wake?.();
    }
  };
  private readonly pagehide = () => {
    void this.stop().catch(() => {});
  };

  /** Coalesce roster/selection changes; at most one reconciliation awaits disposal. */
  private reconcile(gateway: GatewayActor, retryCapacity = false): void {
    const wanted = new Set(this.desired.slice(0, LIMITS.services));
    for (const service of gateway.services.values()) {
      const id = splitRouteId(service.key)!.serviceId;
      if (
        gateway.state !== 'online' ||
        !wanted.has(service.key) ||
        gateway.roster.get(id)?.generation !== service.generation ||
        (retryCapacity &&
          service.state === 'terminal' &&
          service.error?.code === 'CapacityExceeded')
      )
        this.stopService(service);
    }
    gateway.dirty = true;
    if (gateway.reconciling) return;
    gateway.reconciling = (async () => {
      await Promise.resolve();
      while (gateway.dirty) {
        gateway.dirty = false;
        for (const [key, service] of gateway.services)
          if (service.controller.signal.aborted) {
            await service.job;
            if (gateway.services.get(key) === service)
              gateway.services.delete(key);
          }
        const attempt = gateway.attempt;
        if (
          this.phase !== 'started' ||
          gateway.controller.signal.aborted ||
          gateway.state !== 'online' ||
          !attempt ||
          attempt.signal.aborted
        )
          continue;
        for (const key of this.desired.slice(0, LIMITS.services)) {
          const route = splitRouteId(key)!;
          if (
            route.nodeId !== gateway.record.nodeId ||
            gateway.services.has(key)
          )
            continue;
          const record = gateway.roster.get(route.serviceId);
          if (!record) continue;
          const actor: ServiceActor = {
            key,
            gateway,
            generation: record.generation,
            controller: new AbortController(),
            state: 'absent',
            job: Promise.resolve(),
            run: null,
            overflows: 0,
          };
          gateway.services.set(key, actor);
          actor.job = this.serviceLoop(actor, attempt);
        }
      }
    })().finally(() => {
      gateway.reconciling = null;
      if (gateway.dirty) this.reconcile(gateway);
    });
  }
  private async serviceLoop(
    actor: ServiceActor,
    attempt: GatewayAttempt,
  ): Promise<void> {
    const lifetime = AbortSignal.any([actor.controller.signal, attempt.signal]);
    let failures = 0;
    while (!lifetime.aborted) {
      const controller = new AbortController();
      const scope = Object.freeze({
        gatewayId: actor.gateway.record.nodeId,
        gatewayEpoch: actor.gateway.epoch,
        serviceId: splitRouteId(actor.key)!.serviceId,
        generation: actor.generation,
        serviceInstance: ++this.instance,
      });
      const run: ServiceRun = {
        actor,
        scope,
        controller,
        signal: AbortSignal.any([lifetime, controller.signal]),
        connection: attempt.connection!,
        stream: null,
        snapshot: null,
        ready: false,
        rendered: false,
        retryOverflow: false,
        leases: new Map(),
        actions: [],
        actionJob: null,
      };
      actor.run = run;
      actor.state = 'awaitingSnapshot';
      actor.error = undefined;
      this.publishServices();
      let readyAt = 0;
      let failure = new SessionFault('NetworkUnavailable');
      try {
        await deadline(10000, run.signal, async (firstSignal) => {
          run.stream = await run.connection.open(
            {
              kind: 'service',
              service: scope.serviceId,
              generation: scope.generation,
            },
            firstSignal,
          );
          const first = parseServiceFrame(
            decode(await run.stream.receive(firstSignal)),
          );
          if (first.kind !== 'snapshotStart')
            throw new SessionFault('ProtocolError');
        });
        run.snapshot = new BoundedSnapshot({
          snapshots: this.snapshots,
          delivery: this.delivery,
        });
        await deadline(30000, run.signal, async (snapshotSignal) => {
          while (true) {
            const frame = parseServiceFrame(
              decode(await run.stream!.receive(snapshotSignal)),
            );
            check(snapshotSignal);
            if (frame.kind === 'snapshotEnd') return;
            if (frame.kind !== 'message')
              throw new SessionFault('ProtocolError');
            run.snapshot!.apply(frame.message);
          }
        });
        await this.commit(run);
        check(run.signal);
        readyAt = Date.now();
        while (!run.signal.aborted) {
          const received = await run.stream!.receive(run.signal);
          const frame = parseServiceFrame(decode(received));
          check(run.signal);
          if (frame.kind !== 'message') throw new SessionFault('ProtocolError');
          this.enqueue(run, frame.message, received!.wireBytes);
        }
      } catch (error) {
        failure = run.failure ?? asFault(error);
      } finally {
        this.retireRun(run);
        await run.stream?.cancel();
        await run.actionJob;
        if (actor.run === run) actor.run = null;
      }
      failure = run.failure ?? failure;
      if (lifetime.aborted) break;
      if (failure.code === 'Cancelled')
        failure = new SessionFault('NetworkUnavailable');
      actor.error = failure;
      const terminal =
        ['ProtocolError', 'RenderError', 'CapacityExceeded'].includes(
          failure.code,
        ) && !run.retryOverflow;
      actor.state = terminal ? 'terminal' : 'backoff';
      this.publishServices();
      if (terminal) return;
      if (readyAt && Date.now() - readyAt >= 30000) failures = 0;
      try {
        await sleep(backoff(failures++), lifetime);
      } catch {
        break;
      }
    }
  }
  private commit(run: ServiceRun): Promise<void> {
    return new Promise((resolve, reject) => {
      this.rendering.push({ kind: 'commit', run, resolve, reject });
      this.scheduleRender();
    });
  }
  private enqueue(run: ServiceRun, message: A2uiMessage, bytes: number): void {
    if (!this.active(run)) throw cancelled();
    if (
      this.renderCount >= LIMITS.renderQueueCount ||
      bytes > this.renderBytes.availableBytes
    ) {
      run.actor.overflows++;
      run.retryOverflow = run.actor.overflows === 1;
      this.failRun(run, new SessionFault('CapacityExceeded'));
      return;
    }
    const reservation = this.renderBytes.reserve(bytes);
    this.renderCount++;
    try {
      run.snapshot!.apply(message);
      const id = surfaceId(message);
      let lease: SurfaceLease | undefined;
      if ('deleteSurface' in message) {
        const old = run.leases.get(id);
        if (old) this.scopes.remove(old.id);
        run.leases.delete(id);
      } else if ('createSurface' in message) {
        lease = this.scopes.create(run.scope, `${run.actor.key}/${id}`).lease;
        run.leases.set(id, lease);
      }
      this.rendering.push({ kind: 'update', run, message, lease, reservation });
      this.scheduleRender();
    } catch (error) {
      reservation.release();
      this.renderCount--;
      this.failRun(run, asFault(error));
    }
  }
  private scheduleRender(): void {
    if (this.renderTimer !== null) return;
    this.renderTimer = setTimeout(() => {
      this.renderTimer = null;
      this.drainRender();
    }, 0);
  }
  private drainRender(): void {
    while (this.rendering.length) {
      const item = this.rendering.shift()!;
      const run = item.run;
      try {
        if (!this.active(run)) throw cancelled();
        if (item.kind === 'commit') {
          const payload = run.snapshot!.renderPayload(`${run.actor.key}/`);
          try {
            const leases: SurfaceLease[] = [];
            for (const message of payload.messages)
              if ('createSurface' in message) {
                const lease = this.scopes.create(
                  run.scope,
                  message.createSurface.surfaceId,
                ).lease;
                leases.push(lease);
                run.leases.set(
                  message.createSurface.surfaceId.slice(
                    run.actor.key.length + 1,
                  ),
                  lease,
                );
              }
            run.rendered = true;
            const result = this.deliver({
              kind: 'commit',
              service: run.actor.key,
              epoch: run.scope.serviceInstance,
              messages: payload.messages,
              leases,
            });
            if (!result.ok) throw new SessionFault(result.code);
            check(run.signal);
            run.ready = true;
            run.actor.state = 'ready';
            run.actor.error = undefined;
            item.resolve();
            this.publishServices();
          } finally {
            payload.release();
          }
        } else {
          const id = surfaceId(item.message);
          const bytes =
            jsonBytes(item.message) +
            jsonStringBytes(`${run.actor.key}/${id}`) -
            jsonStringBytes(id);
          const reservation = this.delivery.acquire(bytes);
          try {
            const message = prefixServerMessage(
              run.actor.key,
              materializeJson(item.message),
            ) as A2uiMessage;
            run.ready = false;
            const result = this.deliver({
              kind: 'update',
              service: run.actor.key,
              epoch: run.scope.serviceInstance,
              message,
              ...(item.lease ? { lease: item.lease } : {}),
              budget: this.delivery,
            });
            if (!result.ok) throw new SessionFault(result.code);
            check(run.signal);
            run.ready = true;
          } finally {
            reservation.release();
          }
        }
      } catch (error) {
        const failure = asFault(error);
        this.failRun(run, failure);
        if (item.kind === 'commit') item.reject(failure);
      } finally {
        if (item.kind === 'update') {
          item.reservation.release();
          this.renderCount--;
        }
      }
    }
  }
  private deliver(event: RenderEvent): ApplyResult {
    if (this.applying || !this.apply) return { ok: false, code: 'RenderError' };
    this.applying = true;
    let result: ApplyResult;
    try {
      result = applyToRenderer(this.apply, event);
    } finally {
      this.applying = false;
    }
    while (this.removals.size) {
      const run = this.removals.values().next().value!;
      this.removals.delete(run);
      this.removeRendered(run);
    }
    return result;
  }
  private failRun(run: ServiceRun, failure: SessionFault): void {
    run.failure ??= failure;
    this.retireRun(run);
  }
  private retireRun(run: ServiceRun): void {
    run.ready = false;
    run.controller.abort();
    this.scopes.removeTask(run.scope);
    run.leases.clear();
    run.snapshot?.dispose();
    run.snapshot = null;
    for (let i = this.rendering.length - 1; i >= 0; i--)
      if (this.rendering[i]!.run === run) {
        const [item] = this.rendering.splice(i, 1);
        if (item!.kind === 'commit') item!.reject(run.failure ?? cancelled());
        else {
          item!.reservation.release();
          this.renderCount--;
        }
      }
    for (const item of run.actions.splice(0))
      this.settleAction(item, run.failure ?? new SessionFault('ScopeExpired'));
    if (this.applying) this.removals.add(run);
    else this.removeRendered(run);
  }
  private removeRendered(run: ServiceRun): void {
    if (!run.rendered) return;
    run.rendered = false;
    const result = this.deliver({
      kind: 'remove',
      service: run.actor.key,
      epoch: run.scope.serviceInstance,
      reason: 'Source unavailable',
    });
    if (!result.ok) run.failure = new SessionFault('RenderError');
  }
  private stopService(actor: ServiceActor): void {
    actor.controller.abort();
    if (actor.run) this.retireRun(actor.run);
  }
  private retireGateway(actor: GatewayActor): void {
    actor.attempt?.controller.abort();
    actor.roster.clear();
    for (const service of actor.services.values()) this.stopService(service);
    if (actor.state === 'online') {
      actor.state =
        actor.controller.signal.aborted || this.phase !== 'started'
          ? 'stopping'
          : 'backoff';
      this.emit(this.status(actor));
    }
    this.publishServices();
  }
  private sendActions(run: ServiceRun): void {
    if (run.actionJob) return;
    run.actionJob = (async () => {
      await Promise.resolve();
      while (run.actions.length) {
        const item = run.actions.shift()!;
        try {
          this.scopes.require(item.lease.id, (scope) => this.current(scope));
          if (!this.active(run) || !run.ready)
            throw new SessionFault('ScopeExpired');
          await deadline(
            5000,
            AbortSignal.any([run.signal, item.lease.signal]),
            (signal) => run.stream!.send(item.json, signal),
          );
          this.settleAction(item);
        } catch (error) {
          const failure = asFault(error);
          this.settleAction(item, failure);
          if (failure.code !== 'ScopeExpired') this.failRun(run, failure);
        }
      }
    })().finally(() => {
      run.actionJob = null;
      if (run.actions.length && this.active(run)) this.sendActions(run);
    });
  }
  private settleAction(item: ActionItem, failure?: SessionFault): void {
    if (item.settled) return;
    item.settled = true;
    item.reservation.release();
    this.actionCount--;
    if (failure) item.reject(failure);
    else item.resolve();
  }
  private active(run: ServiceRun): boolean {
    return (
      this.phase === 'started' &&
      !run.signal.aborted &&
      run.actor.run === run &&
      run.actor.gateway.attempt?.connection === run.connection &&
      run.actor.gateway.epoch === run.scope.gatewayEpoch
    );
  }
  private runFor(scope: ServiceScope): ServiceRun | null {
    const run = this.gateways
      .get(scope.gatewayId)
      ?.services.get(`${scope.gatewayId}~${scope.serviceId}`)?.run;
    return run && sameServiceScope(run.scope, scope) ? run : null;
  }
  private current(scope: ServiceScope): boolean {
    const run = this.runFor(scope);
    const attempt = run?.actor.gateway.attempt;
    if (attempt?.ping && Date.now() >= attempt.ping.expires) {
      attempt.failure = new SessionFault('Timeout');
      attempt.controller.abort();
      if (run) this.retireGateway(run.actor.gateway);
      return false;
    }
    return !!run && this.active(run) && run.ready && !this.applying;
  }
  private status(
    gateway: GatewayActor,
  ): Extract<LiveEvent, { kind: 'status' }> {
    return {
      kind: 'status',
      sourceId: gateway.record.nodeId,
      state: gateway.state,
      ...(gateway.error ? { error: gateway.error } : {}),
    };
  }
  private publishServices(): void {
    const services = new Map<string, ServiceView>();
    for (const gateway of this.gateways.values())
      for (const service of gateway.roster.values()) {
        const key = `${gateway.record.nodeId}~${service.id}`;
        const actor = gateway.services.get(key);
        services.set(key, {
          id: key,
          name: service.name,
          node: { id: gateway.record.nodeId, name: gateway.name, local: false },
          status:
            actor?.state === 'terminal'
              ? 'error'
              : actor
                ? actor.state === 'ready'
                  ? 'connected'
                  : 'disconnected'
                : gateway.state === 'online'
                  ? 'connected'
                  : 'disconnected',
          ...(actor?.error ? { detail: actor.error.message } : {}),
        });
      }
    this.desired.forEach((key, index) => {
      const route = splitRouteId(key)!;
      const gateway = this.gateways.get(route.nodeId);
      const view = services.get(key) ?? {
        id: key,
        name: route.serviceId,
        node: {
          id: route.nodeId,
          name: gateway?.name ?? route.nodeId,
          local: false,
        },
        status: 'disconnected' as const,
        detail:
          gateway?.error?.message ??
          new SessionFault('SourceUnavailable').message,
      };
      services.set(
        key,
        index >= LIMITS.services
          ? { ...view, status: 'error', detail: 'Service limit' }
          : view,
      );
    });
    this.cachedServices = Object.freeze(
      [...services.values()].map((view) =>
        Object.freeze({ ...view, node: Object.freeze({ ...view.node }) }),
      ),
    );
    this.emit({ kind: 'services', services: this.cachedServices });
  }
  private observe(
    listener: (event: LiveEvent) => void,
    event: LiveEvent,
  ): void {
    try {
      const result: unknown = listener(event);
      if (result instanceof Promise) void result.catch(() => {});
    } catch {
      /* Independent observer. */
    }
  }
  private emit(event: LiveEvent): void {
    for (const listener of this.listeners) this.observe(listener, event);
  }
}
function surfaceIdForClient(message: ClientMessage): string {
  return 'action' in message
    ? message.action.surfaceId
    : message.error.surfaceId;
}

export function createBrowserSession(
  profile: ProfileSnapshot,
  transport: BrowserTransport,
): LiveSession {
  return new BrowserSession(profile, transport);
}

/** Detached numeric counters for capacity diagnostics; contains no frames, keys or resource IDs. */
export function browserSessionDiagnostics(source: LiveSession) {
  if (!(source instanceof BrowserSession))
    throw new SessionFault('InvalidState');
  return source.diagnostics();
}
