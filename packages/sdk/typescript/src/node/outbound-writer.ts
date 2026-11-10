/** Matches the native fanout wire-data budget; includes the active write. */
export const MAX_OUTBOUND_BYTES = 16 * 1024 * 1024;

/** Keep captured replay state in the iterator, not a connection handler's closure. */
export function* encodeReplay(
  messages: Iterable<unknown>,
  hello: unknown,
  delimiter: string,
): Generator<string> {
  if (hello !== undefined) yield JSON.stringify(hello) + delimiter;
  for (const message of messages) yield JSON.stringify(message) + delimiter;
}

/** One connection's writer. Publication never waits for a slow connection. */
export class OutboundWriter {
  private readonly pending: string[] = [];
  private bytes = 0;
  private active = false;
  private stopped = false;
  private replay: Iterator<string> | undefined;

  constructor(
    private readonly write: (
      text: string,
      complete: (error?: Error | null) => void,
    ) => void,
    private readonly disconnect: () => void,
    private readonly limit = MAX_OUTBOUND_BYTES,
  ) {}

  start(replay: Iterable<string>): void {
    this.replay = replay[Symbol.iterator]();
    this.pump();
  }

  send(text: string): void {
    if (this.stopped || !this.reserve(text)) return;
    this.pending.push(text);
    this.pump();
  }

  close(): void {
    this.stopped = true;
    this.pending.length = 0;
    this.bytes = 0;
    this.replay?.return?.();
    this.replay = undefined;
  }

  private reserve(text: string): boolean {
    const bytes = Buffer.byteLength(text);
    if (bytes > this.limit - this.bytes) {
      this.close();
      this.disconnect();
      return false;
    }
    this.bytes += bytes;
    return true;
  }

  private pump(): void {
    if (this.stopped || this.active) return;
    let text: string | undefined;
    if (this.replay) {
      const next = this.replay.next();
      if (next.done) this.replay = undefined;
      else {
        text = next.value;
        // Stream aggregate snapshots larger than the budget, while counting each
        // active replay frame alongside live messages waiting behind the replay.
        // If these cannot fit together, disconnect rather than reorder the stream.
        if (!this.reserve(text)) return;
      }
    }
    text ??= this.pending.shift();
    if (text === undefined) return;
    this.active = true;
    const bytes = Buffer.byteLength(text);
    const complete = (error?: Error | null) => {
      if (this.stopped) return;
      if (error) {
        this.close();
        this.disconnect();
        return;
      }
      this.bytes -= bytes;
      this.active = false;
      // Also support writers whose completion callback runs synchronously.
      queueMicrotask(() => this.pump());
    };
    try {
      this.write(text, complete);
    } catch (error) {
      complete(error instanceof Error ? error : new Error(String(error)));
    }
  }
}
