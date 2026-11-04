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
    let chunk: Uint8Array | undefined;
    let ended = this.stop.signal.aborted;
    let failure: unknown;
    let wake: (() => void) | undefined;
    let line: string | undefined;
    const framer = new JsonlFramer(value => { line = value; return true; });
    const takeLine = (): string | undefined => { const value = line; line = undefined; return value; };
    const data = (value: Buffer | string) => {
      // Retain at most one input chunk. Parse and yield its lines individually so
      // resourceMessages remains responsible for bounded action admission.
      stdin.pause();
      chunk = typeof value === "string" ? Buffer.from(value) : value;
      wake?.();
    };
    const end = () => { ended = true; wake?.(); };
    const error = (value: Error) => { failure = value; end(); };
    const close = () => {
      stdin.off("data", data);
      stdin.off("end", end);
      stdin.off("error", error);
      stdin.pause();
      chunk = undefined;
      framer.close();
      end();
    };
    this.stop.signal.addEventListener("abort", close, { once: true });
    if (!ended) {
      stdin.on("end", end);
      stdin.on("error", error);
      stdin.on("data", data);
    }
    const input = (async function* () {
      try {
        while (!ended || chunk) {
          if (!chunk) {
            await new Promise<void>(resolve => { wake = resolve; });
            continue;
          }
          const current = chunk;
          chunk = undefined;
          for (let offset = 0; offset < current.length;) {
            const newline = current.indexOf(0x0a, offset);
            const next = newline === -1 ? current.length : newline + 1;
            if (!framer.write(current.subarray(offset, next))) return;
            const value = takeLine();
            if (value?.trim()) yield JSON.parse(value) as JsonObject;
            offset = next;
          }
          if (!ended) stdin.resume();
        }
        if (failure) throw failure;
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
