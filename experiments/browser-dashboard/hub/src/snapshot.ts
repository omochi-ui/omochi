import {
  SessionFault,
  MAX_JSON_DEPTH,
  jsonBytes,
  jsonStringBytes,
  measureJson,
  materializeJson,
  planDataPatch,
  splitRouteId,
  type A2uiMessage,
  type CreateSurfaceMessage,
} from '@omochi-ui/core/browser';
import {
  LIMITS,
  type DeliveryPool,
  type Reservation,
  type SnapshotPool,
} from './budget.js';

export type { DeliveryPool, Reservation, SnapshotPool } from './budget.js';

interface Surface {
  readonly create: CreateSurfaceMessage;
  readonly components: ReadonlyMap<string, Record<string, unknown>>;
  readonly data: unknown;
  readonly hasData: boolean;
  readonly byteSize: number;
}

function freezeTree<T>(value: T): T {
  if (value !== null && typeof value === 'object' && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) freezeTree(child);
    Object.freeze(value);
  }
  return value;
}
function owned<T>(value: T): T {
  return freezeTree(materializeJson(value) as T);
}
const rootMessage = (surfaceId: string, data?: unknown): A2uiMessage => ({
  version: 'v0.9',
  updateDataModel: {
    surfaceId,
    path: '/',
    ...(data === undefined ? {} : { value: data }),
  },
});
function componentBytes(
  id: string,
  values: Iterable<Record<string, unknown>>,
): number {
  let count = 0;
  let bytes = 0;
  for (const component of values) {
    const measured = measureJson(component);
    if (measured.maxDepth + 3 > MAX_JSON_DEPTH)
      throw new SessionFault('ProtocolError');
    bytes += measured.byteSize + (count++ ? 1 : 0);
  }
  return count
    ? jsonBytes({
        version: 'v0.9',
        updateComponents: { surfaceId: id, components: [] },
      }) + bytes
    : 0;
}
function dataBytes(
  id: string,
  present: boolean,
  bytes: number,
  depth: number,
): number {
  if (depth + 2 > MAX_JSON_DEPTH) throw new SessionFault('ProtocolError');
  return jsonBytes(rootMessage(id)) + (present ? 9 + bytes : 0);
}
function surfaceBytes(id: string, surface: Omit<Surface, 'byteSize'>): number {
  const data = measureJson(surface.data);
  return (
    jsonBytes(surface.create) +
    componentBytes(id, surface.components.values()) +
    (surface.hasData
      ? dataBytes(id, surface.data !== undefined, data.byteSize, data.maxDepth)
      : 0)
  );
}

/** Browser-only retained-state admission. All candidate copies follow reservation. */
export class BoundedSnapshot {
  private surfaces = new Map<string, Surface>();
  private reservation: Reservation | null = null;
  private bytes = 0;
  private disposed = false;
  constructor(
    private readonly pools: { snapshots: SnapshotPool; delivery: DeliveryPool },
  ) {}
  get byteSize(): number {
    return this.bytes;
  }

  apply(message: A2uiMessage): void {
    this.assertLive();
    // Inputs were schema-validated at the stream boundary. Inspect without copying.
    measureJson(message);
    let id: string;
    let candidateBytes: number;
    let build: () => Surface | null;
    if ('createSurface' in message) {
      id = message.createSurface.surfaceId;
      if (this.surfaces.has(id)) throw new SessionFault('ProtocolError');
      candidateBytes = jsonBytes(message);
      build = () => ({
        create: owned(message),
        components: new Map(),
        data: undefined,
        hasData: false,
        byteSize: candidateBytes,
      });
    } else if ('deleteSurface' in message) {
      id = message.deleteSurface.surfaceId;
      if (!this.surfaces.has(id)) return;
      candidateBytes = 0;
      build = () => null;
    } else {
      id =
        'updateComponents' in message
          ? message.updateComponents.surfaceId
          : message.updateDataModel.surfaceId;
      const old = this.surfaces.get(id);
      if (!old) throw new SessionFault('ProtocolError');
      if ('updateComponents' in message) {
        // Only this bounded input overlay is allocated before admission, not the merged map/list.
        const overlay = new Map<string, Record<string, unknown>>();
        for (const component of message.updateComponents.components)
          overlay.set(component.id, component);
        function* components() {
          for (const [key, value] of old!.components)
            yield overlay.get(key) ?? value;
          for (const [key, value] of overlay)
            if (!old!.components.has(key)) yield value;
        }
        candidateBytes =
          old.byteSize -
          componentBytes(id, old.components.values()) +
          componentBytes(id, components());
        build = () => {
          const merged = new Map(old.components);
          for (const [key, value] of overlay) merged.set(key, owned(value));
          return { ...old, components: merged, byteSize: candidateBytes };
        };
      } else {
        const update = message.updateDataModel;
        const plan = planDataPatch(
          old.data,
          update.path ?? '/',
          Object.hasOwn(update, 'value'),
          update.value,
        );
        const previous = measureJson(old.data);
        const oldBytes = old.hasData
          ? dataBytes(
              id,
              old.data !== undefined,
              previous.byteSize,
              previous.maxDepth,
            )
          : 0;
        // Undefined root is the only zero-sized JSON value.
        candidateBytes =
          old.byteSize -
          oldBytes +
          dataBytes(id, plan.byteSize !== 0, plan.byteSize, plan.maxDepth);
        build = () => ({
          ...old,
          data: freezeTree(plan.materialize()),
          hasData: true,
          byteSize: candidateBytes,
        });
      }
    }
    const total =
      this.bytes - (this.surfaces.get(id)?.byteSize ?? 0) + candidateBytes;
    if (total > LIMITS.snapshot || total > this.pools.snapshots.availableBytes)
      throw new SessionFault('CapacityExceeded');
    const reservation = this.pools.snapshots.reserve(total);
    try {
      const candidate = build();
      if (candidate && surfaceBytes(id, candidate) !== candidateBytes)
        throw new SessionFault('ProtocolError');
      const surfaces = new Map(this.surfaces);
      if (candidate) surfaces.set(id, candidate);
      else surfaces.delete(id);
      const previous = this.reservation;
      this.surfaces = surfaces;
      this.bytes = total;
      this.reservation = reservation;
      previous?.release();
    } catch (error) {
      reservation.release();
      throw error;
    }
  }

  renderPayload(routePrefix: string): {
    messages: A2uiMessage[];
    release(): void;
  } {
    this.assertLive();
    if (!routePrefix.endsWith('/') || !splitRouteId(routePrefix.slice(0, -1)))
      throw new SessionFault('ProtocolError');
    let bytes = this.bytes;
    for (const [id, surface] of this.surfaces) {
      const routed = routePrefix + id;
      const difference = jsonStringBytes(routed) - jsonStringBytes(id);
      bytes +=
        difference *
        (1 + Number(surface.components.size > 0) + Number(surface.hasData));
      bytes += jsonBytes(rootMessage(routed));
      if (bytes > LIMITS.delivery) throw new SessionFault('CapacityExceeded');
    }
    const reservation = this.pools.delivery.acquire(bytes);
    const messages: A2uiMessage[] = [];
    try {
      for (const [id, surface] of this.surfaces) {
        const surfaceId = routePrefix + id;
        const create = materializeJson(surface.create) as CreateSurfaceMessage;
        create.createSurface.surfaceId = surfaceId;
        messages.push(create, rootMessage(surfaceId));
        if (surface.components.size)
          messages.push({
            version: 'v0.9',
            updateComponents: {
              surfaceId,
              components: [...surface.components.values()].map(
                (value) =>
                  materializeJson(value) as Record<string, unknown> & {
                    id: string;
                  },
              ),
            },
          });
        if (surface.hasData)
          messages.push(rootMessage(surfaceId, materializeJson(surface.data)));
      }
      if (
        messages.reduce((sum, message) => sum + jsonBytes(message), 0) !== bytes
      )
        throw new SessionFault('ProtocolError');
      return {
        messages,
        release: () => {
          messages.length = 0;
          reservation.release();
        },
      };
    } catch (error) {
      reservation.release();
      throw error;
    }
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.surfaces.clear();
    this.reservation?.release();
    this.reservation = null;
    this.bytes = 0;
  }
  private assertLive(): void {
    if (this.disposed) throw new SessionFault('InvalidState');
  }
}
