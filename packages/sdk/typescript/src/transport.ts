import { type ResourceFrame } from "@omochi-ui/core";
import { ResourceRegistry, resourceMessages } from "./resources.js";
import { JsonlFramer } from "./node/jsonl-framer.js";
import type { A2UIMessage, JsonObject } from "./types.js";

export interface Transport extends AsyncIterable<JsonObject> {
  resources?: ResourceRegistry;
  readonly capabilities: ReadonlySet<string>;
  send(message: A2UIMessage | ResourceFrame): Promise<void>;
  /** Optional runtime lifecycle hook. Stop reads and release owned listeners. */
  stopReceiving?(): void | Promise<void>;
}

export class StdioTransport implements Transport {
  private readonly stop = new AbortController();
  resources?: ResourceRegistry;
  readonly capabilities: ReadonlySet<string>;

  constructor(capabilities: Iterable<string> = []) {
    this.capabilities = new Set(capabilities);
  }

  async send(message: A2UIMessage | ResourceFrame): Promise<void> {
    await new Promise<void>((resolve, reject) => {
      process.stdout.write(`${JSON.stringify(message)}\n`, (error) => error ? reject(error) : resolve());
    });
  }

  async *[Symbol.asyncIterator](): AsyncIterator<JsonObject> {
    const stdin = process.stdin;
    let closed = this.stop.signal.aborted;
    let ended = stdin.readableEnded;
    let failure: unknown;
    let wake: (() => void) | undefined;
    let line: string | undefined;
    const framer = new JsonlFramer(value => { line = value; return true; });
    const takeLine = (): string | undefined => { const value = line; line = undefined; return value; };
    const readable = () => { wake?.(); };
    const end = () => { ended = true; wake?.(); };
    const error = (value: Error) => { failure = value; end(); };
    const close = () => {
      closed = true;
      stdin.off("readable", readable);
      stdin.off("end", end);
      stdin.off("error", error);
      stdin.pause();
      framer.close();
      end();
    };
    this.stop.signal.addEventListener("abort", close, { once: true });
    if (!closed) {
      stdin.on("end", end);
      stdin.on("error", error);
      stdin.on("readable", readable);
    }
    const input = (async function* () {
      try {
        // Pull from the stream's bounded buffer instead of toggling flowing mode:
        // Bun can lose the EOF notification when paused between complete lines.
        while (!closed) {
          const bytes = stdin.read() as Buffer | string | null;
          if (bytes === null) {
            if (ended) break;
            await new Promise<void>(resolve => { wake = resolve; });
            continue;
          }
          const current = typeof bytes === "string" ? Buffer.from(bytes) : bytes;
          for (let offset = 0; offset < current.length;) {
            const newline = current.indexOf(0x0a, offset);
            const next = newline === -1 ? current.length : newline + 1;
            if (!framer.write(current.subarray(offset, next))) return;
            const value = takeLine();
            if (value?.trim()) yield JSON.parse(value) as JsonObject;
            offset = next;
          }
        }
        if (failure) throw failure;
        if (closed) return;
        framer.end();
        const value = takeLine();
        if (value?.trim()) yield JSON.parse(value) as JsonObject;
      } finally { close(); }
    })();
    try { yield* resourceMessages(input, frame => this.send(frame), this.resources, this.stop.signal); }
    finally { this.stop.signal.removeEventListener("abort", close); close(); }
  }
  stopReceiving(): void { this.stop.abort(); }
}

export interface WebSocketLike extends AsyncIterable<string | Uint8Array> {
  send(data: string): unknown | Promise<unknown>;
}

export class WebSocketTransport implements Transport {
  private readonly stop = new AbortController();
  resources?: ResourceRegistry;
  readonly capabilities: ReadonlySet<string>;

  constructor(private readonly socket: WebSocketLike, capabilities: Iterable<string> = []) {
    this.capabilities = new Set(capabilities);
  }

  async send(message: A2UIMessage | ResourceFrame): Promise<void> {
    await this.socket.send(JSON.stringify(message));
  }

  async *[Symbol.asyncIterator](): AsyncIterator<JsonObject> {
    const socket = this.socket;
    const input = (async function* () { for await (const message of socket) {
      const text = typeof message === "string" ? message : new TextDecoder().decode(message);
      yield JSON.parse(text) as JsonObject;
    } })();
    yield* resourceMessages(input, frame => this.send(frame), this.resources, this.stop.signal);
  }
  stopReceiving(): void { this.stop.abort(); }
}

export class CallbackTransport implements Transport {
  private readonly stop = new AbortController();
  resources?: ResourceRegistry;
  readonly capabilities: ReadonlySet<string>;

  constructor(
    private readonly sender: (message: A2UIMessage | ResourceFrame) => Promise<void>,
    private readonly receiver: () => AsyncIterable<JsonObject>,
    capabilities: Iterable<string> = [],
  ) {
    this.capabilities = new Set(capabilities);
  }

  send(message: A2UIMessage | ResourceFrame): Promise<void> {
    return this.sender(message);
  }

  [Symbol.asyncIterator](): AsyncIterator<JsonObject> {
    return resourceMessages(this.receiver(), frame => this.send(frame), this.resources, this.stop.signal)[Symbol.asyncIterator]();
  }
  stopReceiving(): void { this.stop.abort(); }
}
