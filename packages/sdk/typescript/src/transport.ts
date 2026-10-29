import { type ResourceFrame } from "@omochi-ui/core";
import { ResourceRegistry, resourceMessages } from "./resources.js";
import { createInterface } from "node:readline";
import type { A2UIMessage, JsonObject } from "./types.js";

export interface Transport extends AsyncIterable<JsonObject> {
  resources?: ResourceRegistry;
  readonly capabilities: ReadonlySet<string>;
  send(message: A2UIMessage | ResourceFrame): Promise<void>;
}

export class StdioTransport implements Transport {
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
    const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
    const input = (async function* () { for await (const line of lines) if (line.trim()) yield JSON.parse(line) as JsonObject; })();
    try { yield* resourceMessages(input, frame => this.send(frame), this.resources); } finally { lines.close(); }
  }
}

export interface WebSocketLike extends AsyncIterable<string | Uint8Array> {
  send(data: string): unknown | Promise<unknown>;
}

export class WebSocketTransport implements Transport {
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
    yield* resourceMessages(input, frame => this.send(frame), this.resources);
  }
}

export class CallbackTransport implements Transport {
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
    return resourceMessages(this.receiver(), frame => this.send(frame), this.resources)[Symbol.asyncIterator]();
  }
}
