import { SessionFault } from '@omochi-ui/core/browser';

export const MiB = 1024 * 1024;
export const LIMITS = Object.freeze({
  gateways: 2, services: 8, frame: 4 * MiB, frameBodies: 8 * MiB,
  snapshot: 16 * MiB, snapshots: 32 * MiB, delivery: 16 * MiB,
  renderQueueCount: 32, renderQueueBytes: 16 * MiB,
  actionCount: 8, actionBytes: 8 * MiB, leases: 4096, leaseText: 8 * MiB,
  resources: 2, png: 8 * MiB, pngBytes: 16 * MiB,
  imageSide: 8192, imagePixels: 4 * 1024 * 1024, decoded: 32 * MiB,
});

export interface Reservation { release(): void }
export interface SnapshotPool {
  readonly availableBytes: number;
  reserve(bytes: number): Reservation;
}
export interface DeliveryPool { acquire(bytes: number): Reservation }

/** Synchronous admission, including old+candidate transient charges; never queues. */
export class BytePool implements SnapshotPool {
  private used = 0;
  private peak = 0;
  constructor(readonly capacity: number) {
    if (!Number.isSafeInteger(capacity) || capacity < 0) throw new SessionFault('InvalidState');
  }
  get availableBytes(): number { return this.capacity - this.used; }
  get usedBytes(): number { return this.used; }
  get peakBytes(): number { return this.peak; }
  reserve(bytes: number): Reservation {
    if (!Number.isSafeInteger(bytes) || bytes < 0) throw new SessionFault('ProtocolError');
    if (bytes > this.availableBytes) throw new SessionFault('CapacityExceeded');
    this.used += bytes;
    this.peak = Math.max(this.peak, this.used);
    let live = true;
    return { release: () => { if (live) { live = false; this.used -= bytes; } } };
  }
}

/** One synchronous render payload, with additional scratch in that same slot. */
export class RenderDeliveryPool implements DeliveryPool {
  private active = false;
  private readonly bytes: BytePool;
  constructor(capacity = LIMITS.delivery) { this.bytes = new BytePool(capacity); }
  get usedBytes(): number { return this.bytes.usedBytes; }
  get peakBytes(): number { return this.bytes.peakBytes; }
  acquire(bytes: number): Reservation {
    if (this.active) throw new SessionFault('CapacityExceeded');
    const reservation = this.bytes.reserve(bytes);
    this.active = true;
    let live = true;
    return { release: () => {
      if (!live) return;
      live = false;
      reservation.release();
      this.active = false;
    } };
  }
  reserve(bytes: number): () => void {
    if (!this.active) throw new SessionFault('InvalidState');
    const reservation = this.bytes.reserve(bytes);
    return () => reservation.release();
  }
}
