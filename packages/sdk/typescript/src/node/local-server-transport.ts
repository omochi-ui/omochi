import {
  isResourceFrame,
  resourceHello,
  type ResourceFrame,
} from '@omochi-ui/core';
import { ResourceRegistry } from '../resources.js';

import { A2uiClientMessageSchema } from '@a2ui/web_core/v0_9';

import { ReceiveQueue, ReceiveQueueOverflowError } from '../receive-queue.js';
import type { Transport } from '../transport.js';
import type { A2UIMessage, JsonObject } from '../types.js';
import {
  nativeRequest,
  NativeDuplex,
  type LocalEndpoint,
} from './native-local.js';
export type { LocalEndpoint } from './native-local.js';
import { A2uiSnapshot } from './snapshot.js';
import { encodeReplay, OutboundWriter } from './outbound-writer.js';
import { JsonlFramer } from './jsonl-framer.js';

export interface LocalServerTransportOptions {
  /** Discovery id and directory name; must match `[a-z0-9._-]{1,64}`. */
  serviceId: string;
  /** Human-readable label shown by a hub. Defaults to `serviceId`. */
  name?: string;
  /** An emoji, or a `data:image/...;base64,` URL a browser can render directly. */
  icon?: string;
  /** Complete runtime root; defaults to OMOCHI_RUNTIME_DIR or the platform v2 directory. */
  runtimeDir?: string;
  capabilities?: Iterable<string>;
}

interface PendingReceive {
  resolve(result: IteratorResult<JsonObject>): void;
  reject(error: unknown): void;
}

/**
 * Publishes one application through user-private local IPC for discovery.
 *
 * The shared native adapter writes `<runtimeDir>/svc-<serviceId>~/manifest.json`
 * and serves a Unix domain socket or Windows named pipe. Access is restricted
 * to the current OS user through directory permissions or Windows ACLs.
 *
 * Like `WebSocketServerTransport`, it fans one application out to every connected
 * renderer and merges their inbound messages into a single stream for the Runtime.
 */
export class LocalServerTransport implements Transport {
  resources?: ResourceRegistry;
  readonly capabilities: ReadonlySet<string>;
  readonly ready: Promise<void>;

  private readonly serviceId: string;
  private readonly manifestName: string;
  private readonly icon: string | undefined;
  private readonly root: string | undefined;
  private nativeId: number | null = null;
  private resolvedEndpoint: LocalEndpoint | null = null;
  private accepting: Promise<void> | null = null;
  private readonly clients = new Set<NativeDuplex>();
  private readonly outbound = new WeakMap<NativeDuplex, OutboundWriter>();
  private readonly snapshot = new A2uiSnapshot();
  private readonly received = new ReceiveQueue<JsonObject>();
  private readonly waiting: PendingReceive[] = [];
  private resolvedDirectory: string | null = null;
  private closed = false;
  private receiveError: unknown = null;
  private closePromise: Promise<void> | null = null;

  constructor(options: LocalServerTransportOptions) {
    if (
      !/^[a-z0-9._-]{1,64}$/.test(options.serviceId) ||
      ['.', '..'].includes(options.serviceId)
    )
      throw new TypeError(
        'serviceId must match [a-z0-9._-]{1,64} and cannot be . or ..',
      );
    this.serviceId = options.serviceId;
    this.manifestName = options.name ?? this.serviceId;
    this.icon = options.icon;
    this.root = options.runtimeDir;
    this.capabilities = new Set(options.capabilities ?? []);
    this.ready = this.publish().catch((error: unknown) => {
      this.receiveError = error;
      this.flushReceivers();
      throw error;
    });
  }

  get endpoint(): LocalEndpoint | null {
    return this.resolvedEndpoint;
  }

  get directory(): string | null {
    return this.resolvedDirectory;
  }

  async send(message: A2UIMessage | ResourceFrame): Promise<void> {
    if (!isResourceFrame(message)) this.snapshot.apply(message);
    const encoded = `${JSON.stringify(message)}\n`;
    for (const client of this.clients) {
      this.outbound.get(client)?.send(encoded);
    }
  }

  async *[Symbol.asyncIterator](): AsyncIterator<JsonObject> {
    await this.ready;
    while (true) {
      const result = await this.nextMessage();
      if (result.done) return;
      yield result.value;
    }
  }

  stopReceiving(): Promise<void> {
    return this.close();
  }

  close(): Promise<void> {
    if (this.closePromise) return this.closePromise;
    this.closed = true;
    this.received.clear();
    this.flushReceivers();
    this.closePromise = (async () => {
      // Startup may still be binding or writing its manifest when close is called.
      await this.ready.catch(() => {});
      for (const client of this.clients) {
        this.outbound.get(client)?.close();
        client.destroy();
      }
      this.clients.clear();
      if (this.nativeId !== null) await nativeRequest('close', this.nativeId);
      if (this.accepting) await this.accepting;
      this.nativeId = null;
      this.resolvedDirectory = null;
      this.resolvedEndpoint = null;
    })();
    return this.closePromise;
  }

  private async publish(): Promise<void> {
    const result = JSON.parse(
      (
        await nativeRequest('publish', 0, Buffer.alloc(0), {
          serviceId: this.serviceId,
          name: this.manifestName,
          icon: this.icon,
          runtimeDir: this.root,
        })
      ).toString(),
    ) as { id: number; endpoint: LocalEndpoint; directory: string };
    this.nativeId = result.id;
    this.resolvedDirectory = result.directory;
    this.resolvedEndpoint = result.endpoint;
    this.accepting = this.acceptNative();
  }

  private async acceptNative(): Promise<void> {
    try {
      while (!this.closed && this.nativeId !== null) {
        const handle = JSON.parse(
          (await nativeRequest('accept', this.nativeId)).toString(),
        ) as number;
        this.accept(new NativeDuplex(handle));
      }
    } catch (error) {
      if (!this.closed) {
        this.receiveError = error;
        this.flushReceivers();
        void this.close();
      }
    }
  }

  private accept(socket: NativeDuplex): void {
    if (this.closed) {
      socket.destroy();
      return;
    }
    this.clients.add(socket);
    const lines = new JsonlFramer((line) => receiveLine(line));
    const disconnect = () => {
      this.clients.delete(socket);
      this.outbound.get(socket)?.close();
      lines.close();
      socket.off('data', receiveChunk);
      socket.off('end', endInput);
      socket.destroy();
    };
    socket.on('error', disconnect);
    socket.on('close', disconnect);
    const outbound = new OutboundWriter((text, complete) => {
      socket.write(text, complete);
    }, disconnect);
    this.outbound.set(socket, outbound);
    const receiveLine = (line: string): boolean => {
      if (this.closed || socket.destroyed) return false;
      if (!line.trim()) return true;
      try {
        const value: unknown = JSON.parse(line);
        if (this.resources && isResourceFrame(value)) {
          if (value.kind === 'request')
            outbound.send(`${JSON.stringify(this.resources.respond(value))}\n`);
          return true;
        }
        const parsed = A2uiClientMessageSchema.safeParse(value);
        if (!parsed.success || parsed.data.version !== 'v0.9') {
          throw new Error(
            parsed.success
              ? `unsupported version: ${parsed.data.version}`
              : 'expected an A2UI v0.9 client message',
          );
        }
        const message = value as JsonObject;
        if ('error' in message) {
          // The schema can omit a missing generic code or strip an extra action.
          if (
            'action' in message ||
            !('error' in parsed.data) ||
            !('code' in parsed.data.error)
          ) {
            throw new Error('invalid client error report');
          }
          // Valid client error reports carry no action; keep the connection open.
          return true;
        }
        // Preserve metadata/extensions, but do not dispatch legacy Runtime commands
        // from an otherwise valid A2UI envelope.
        delete message.type;
        this.enqueue(message);
        return true;
      } catch (error) {
        if (error instanceof ReceiveQueueOverflowError) this.failReceive(error);
        disconnect();
        return false;
      }
    };
    const receiveChunk = (chunk: Buffer) => {
      try {
        lines.write(chunk);
      } catch {
        disconnect();
      }
    };
    const endInput = () => {
      try {
        lines.end();
      } catch {
        disconnect();
      }
    };
    socket.on('data', receiveChunk);
    socket.on('end', endInput);
    outbound.start(
      encodeReplay(
        this.snapshot.messages(),
        this.resources ? resourceHello() : undefined,
        '\n',
      ),
    );
  }

  private enqueue(message: JsonObject): void {
    const waiter = this.waiting.shift();
    if (waiter) waiter.resolve({ value: message, done: false });
    else this.received.push(message);
  }

  private failReceive(error: ReceiveQueueOverflowError): void {
    this.receiveError = error;
    // Abort immediately: queued actions must not run after an action was lost.
    for (const client of this.clients) client.destroy();
    void this.close().catch(() => {});
  }

  private nextMessage(): Promise<IteratorResult<JsonObject>> {
    const message = this.received.shift();
    if (message) return Promise.resolve({ value: message, done: false });
    if (this.receiveError) return Promise.reject(this.receiveError);
    if (this.closed) return Promise.resolve({ value: undefined, done: true });
    return new Promise((resolve, reject) =>
      this.waiting.push({ resolve, reject }),
    );
  }

  private flushReceivers(): void {
    for (const waiter of this.waiting.splice(0)) {
      if (this.receiveError) waiter.reject(this.receiveError);
      else waiter.resolve({ value: undefined, done: true });
    }
  }
}
