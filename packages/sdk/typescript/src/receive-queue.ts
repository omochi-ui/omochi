// Pending actions exclude the action currently executing and resource RPC frames.
export const MAX_PENDING_MESSAGES = 256;
export const MAX_PENDING_BYTES = 4 * 1024 * 1024;

export class ReceiveQueueOverflowError extends Error {
  constructor() {
    super('pending action queue exceeded 256 messages or 4 MiB');
    this.name = 'ReceiveQueueOverflowError';
  }
}

/** Bound both object count and compact UTF-8 JSON size, independent of the wire format. */
export class ReceiveQueue<T> {
  private readonly entries: Array<{ value: T; bytes: number }> = [];
  private bytes = 0;

  get length(): number { return this.entries.length; }

  push(value: T): void {
    const bytes = Buffer.byteLength(JSON.stringify(value));
    if (this.entries.length >= MAX_PENDING_MESSAGES || this.bytes + bytes > MAX_PENDING_BYTES) {
      throw new ReceiveQueueOverflowError();
    }
    this.entries.push({ value, bytes });
    this.bytes += bytes;
  }

  shift(): T | undefined {
    const entry = this.entries.shift();
    if (!entry) return undefined;
    this.bytes -= entry.bytes;
    return entry.value;
  }

  clear(): void {
    this.entries.length = 0;
    this.bytes = 0;
  }
}
