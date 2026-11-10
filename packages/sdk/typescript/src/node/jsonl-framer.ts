// Match the Python and Rust SDKs: LF is excluded; a preceding CR counts.
export const MAX_FRAME_BYTES = 4 * 1024 * 1024;

export class JsonlFrameTooLargeError extends Error {
  constructor() {
    super('JSONL frame exceeded 4 MiB');
    this.name = 'JsonlFrameTooLargeError';
  }
}

/** Bound partial lines before decoding UTF-8, trimming whitespace, or parsing JSON. */
export class JsonlFramer {
  private buffer = Buffer.alloc(0);
  private length = 0;
  private closed = false;

  constructor(private readonly onLine: (line: string) => boolean) {}

  /** A false callback stops delivery, including subsequent lines in this chunk. */
  write(chunk: Uint8Array): boolean {
    if (this.closed) return false;
    let offset = 0;
    while (offset < chunk.length) {
      const newline = chunk.indexOf(0x0a, offset);
      const end = newline === -1 ? chunk.length : newline;
      const size = end - offset;
      if (this.length + size > MAX_FRAME_BYTES) {
        this.close();
        throw new JsonlFrameTooLargeError();
      }
      if (newline !== -1 && this.length === 0) {
        // Complete frames can be decoded directly without retaining the input chunk.
        const line = Buffer.from(
          chunk.buffer,
          chunk.byteOffset + offset,
          size,
        ).toString('utf8');
        if (!this.deliver(line)) return false;
      } else {
        this.append(chunk.subarray(offset, end));
        if (newline !== -1) {
          const line = this.buffer.toString('utf8', 0, this.length);
          this.length = 0;
          if (!this.deliver(line)) return false;
        }
      }
      offset = end + 1;
    }
    return true;
  }

  /** EOF terminates a final line without requiring an LF. */
  end(): boolean {
    if (this.closed) return false;
    const accepted =
      this.length === 0 ||
      this.deliver(this.buffer.toString('utf8', 0, this.length));
    this.close();
    return accepted;
  }

  close(): void {
    this.closed = true;
    this.buffer = Buffer.alloc(0);
    this.length = 0;
  }

  private deliver(line: string): boolean {
    if (!this.onLine(line)) this.close();
    return !this.closed;
  }

  private append(bytes: Uint8Array): void {
    const needed = this.length + bytes.length;
    if (needed > this.buffer.length) {
      // Geometric growth keeps one-byte writes linear, with at most 4 MiB retained.
      const buffer = Buffer.allocUnsafe(
        Math.min(
          MAX_FRAME_BYTES,
          Math.max(needed, this.buffer.length * 2, 1024),
        ),
      );
      this.buffer.copy(buffer, 0, 0, this.length);
      this.buffer = buffer;
    }
    this.buffer.set(bytes, this.length);
    this.length = needed;
  }
}
