import {
  SessionFault, isU64Decimal, isValidNodeId, isValidServiceId, isValidSurfaceId,
  type LeaseId, type SurfaceLease,
} from '@omochi-ui/core/browser';
import { BytePool, LIMITS } from './budget.js';

/** Captured service provenance; never consult a mutable current connection to route work. */
export interface ServiceScope {
  readonly gatewayId: string;
  readonly gatewayEpoch: number;
  readonly serviceId: string;
  readonly generation: string;
  readonly serviceInstance: number;
}
export interface ScopedSurface {
  readonly task: Readonly<ServiceScope>;
  readonly lease: SurfaceLease;
}
interface Entry extends ScopedSurface { controller: AbortController; release(): void }

export function sameServiceScope(a: ServiceScope, b: ServiceScope): boolean {
  return a.gatewayId === b.gatewayId && a.gatewayEpoch === b.gatewayEpoch && a.serviceId === b.serviceId &&
    a.generation === b.generation && a.serviceInstance === b.serviceInstance;
}

/** Local lifecycle references, separate from gateway-wide authorization. */
export class SurfaceScopes {
  private readonly entries = new Map<LeaseId, Entry>();
  private readonly text = new BytePool(LIMITS.leaseText);
  get size(): number { return this.entries.size; }
  get retainedBytes(): number { return this.text.usedBytes; }

  create(task: ServiceScope, surfaceId: string): ScopedSurface {
    if (!isValidNodeId(task.gatewayId) || !isValidServiceId(task.serviceId) || !isU64Decimal(task.generation) ||
        !Number.isSafeInteger(task.gatewayEpoch) || task.gatewayEpoch < 1 ||
        !Number.isSafeInteger(task.serviceInstance) || task.serviceInstance < 1) throw new SessionFault('ProtocolError');
    const prefix = `${task.gatewayId}~${task.serviceId}/`;
    if (!surfaceId.startsWith(prefix) || !isValidSurfaceId(surfaceId.slice(prefix.length))) throw new SessionFault('ProtocolError');
    if (this.entries.size >= LIMITS.leases) throw new SessionFault('CapacityExceeded');
    const reservation = this.text.reserve(new TextEncoder().encode(surfaceId).length);
    try {
      const controller = new AbortController();
      const lease = Object.freeze({ id: crypto.randomUUID(), surfaceId, signal: controller.signal });
      const entry = { task: Object.freeze({ ...task }), lease, controller, release: () => reservation.release() };
      this.entries.set(lease.id, entry);
      return Object.freeze({ task: entry.task, lease });
    } catch (error) { reservation.release(); throw error; }
  }
  require(id: LeaseId, isCurrent: (task: ServiceScope) => boolean): ScopedSurface {
    const entry = this.entries.get(id);
    if (!entry || entry.lease.signal.aborted || !isCurrent(entry.task)) throw new SessionFault('ScopeExpired');
    return Object.freeze({ task: entry.task, lease: entry.lease });
  }
  remove(id: LeaseId): void {
    const entry = this.entries.get(id);
    if (!entry) return;
    // Remove before abort handlers run so reentrant lookups already fail closed.
    this.entries.delete(id);
    entry.release();
    entry.controller.abort();
  }
  removeTask(task: ServiceScope): void {
    for (const [id, entry] of this.entries) if (sameServiceScope(entry.task, task)) this.remove(id);
  }
  close(): void { for (const id of this.entries.keys()) this.remove(id); }
}
