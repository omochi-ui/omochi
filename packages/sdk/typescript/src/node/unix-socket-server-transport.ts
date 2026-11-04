import { isResourceFrame, resourceHello, serveResources, type ResourceFrame } from "@omochi-ui/core";
import { ResourceRegistry } from "../resources.js";
import { chmod, rmdir } from "node:fs/promises";
import { createServer, type Server, type Socket } from "node:net";
import path from "node:path";
import process from "node:process";

import { A2uiClientMessageSchema } from "@a2ui/web_core/v0_9";

import { ReceiveQueue, ReceiveQueueOverflowError } from "../receive-queue.js";
import type { Transport } from "../transport.js";
import type { A2UIMessage, JsonObject } from "../types.js";
import {
  assertSocketPathFits,
  claimSocketPath,
  ensureServiceDirectory,
  lockService,
  omochiRuntimeRoot,
  removeServiceDirectory,
  removeServiceDirectorySync,
  validateServiceId,
  writeServiceManifest,
} from "./service-directory.js";
import { A2uiSnapshot } from "./snapshot.js";
import { encodeReplay, OutboundWriter } from "./outbound-writer.js";
import { JsonlFramer } from "./jsonl-framer.js";

export interface UnixSocketServerTransportOptions {
  /** Discovery id and directory name; must match `[A-Za-z0-9._-]{1,64}`. */
  serviceId: string;
  /** Human-readable label shown by a hub. Defaults to `serviceId`. */
  name?: string;
  /** An emoji, or a `data:image/...;base64,` URL a browser can render directly. */
  icon?: string;
  /** Defaults to `$XDG_RUNTIME_DIR/omochi`. */
  runtimeDir?: string;
  capabilities?: Iterable<string>;
}

interface PendingReceive {
  resolve(result: IteratorResult<JsonObject>): void;
  reject(error: unknown): void;
}

const SOCKET_FILE_NAME = "a2ui.sock";

/**
 * Publishes one application on a unix domain socket and advertises it for discovery.
 *
 * The transport writes `<runtimeDir>/<serviceId>/manifest.json` next to the socket so a
 * hub can find it without configuration. Reaching the socket is the authentication: the
 * runtime directory is mode 0700, so only the owning user can connect.
 *
 * Like `WebSocketServerTransport`, it fans one application out to every connected
 * renderer and merges their inbound messages into a single stream for the Runtime.
 */
export class UnixSocketServerTransport implements Transport {
  resources?: ResourceRegistry;
  readonly capabilities: ReadonlySet<string>;
  readonly ready: Promise<void>;

  private readonly serviceId: string;
  private readonly manifestName: string;
  private readonly icon: string | undefined;
  private readonly root: string;
  private readonly server: Server = createServer();
  private readonly clients = new Set<Socket>();
  private readonly outbound = new WeakMap<Socket, OutboundWriter>();
  private readonly snapshot = new A2uiSnapshot();
  private readonly received = new ReceiveQueue<JsonObject>();
  private readonly waiting: PendingReceive[] = [];
  private readonly exitCleanup: () => void;
  private releaseService: (() => void) | null = null;
  private resolvedDirectory: string | null = null;
  private closed = false;
  private receiveError: unknown = null;
  private closePromise: Promise<void> | null = null;

  constructor(options: UnixSocketServerTransportOptions) {
    this.serviceId = validateServiceId(options.serviceId);
    this.manifestName = options.name ?? this.serviceId;
    this.icon = options.icon;
    this.root = options.runtimeDir ?? omochiRuntimeRoot();
    this.capabilities = new Set(options.capabilities ?? []);
    this.server.on("connection", (socket) => this.accept(socket));
    this.exitCleanup = () => {
      if (this.resolvedDirectory) removeServiceDirectorySync(this.resolvedDirectory);
      this.resolvedDirectory = null;
      this.releaseService?.();
      this.releaseService = null;
    };
    process.once("exit", this.exitCleanup);
    this.ready = this.publish().catch((error: unknown) => {
      this.receiveError = error;
      this.flushReceivers();
      throw error;
    });
  }

  /** The socket path, available once `ready` resolves. */
  get socketPath(): string | null {
    return this.resolvedDirectory ? path.join(this.resolvedDirectory, SOCKET_FILE_NAME) : null;
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

  stopReceiving(): Promise<void> { return this.close(); }

  close(): Promise<void> {
    if (this.closePromise) return this.closePromise;
    this.closed = true;
    this.received.clear();
    this.flushReceivers();
    this.closePromise = (async () => {
      // Startup may still be binding or writing its manifest when close is called.
      await this.ready.catch(() => {});
      const directory = this.resolvedDirectory;
      try {
        // Retract the advertisement before tearing the socket down.
        if (directory) await removeServiceDirectory(directory);
      } finally {
        for (const client of this.clients) {
          this.outbound.get(client)?.close();
          client.destroy();
        }
        this.clients.clear();
        if (this.server.listening) {
          await new Promise<void>((resolve) => this.server.close(() => resolve()));
        }
      }
      this.resolvedDirectory = null;
      this.releaseService?.();
      this.releaseService = null;
      process.off("exit", this.exitCleanup);
    })();
    return this.closePromise;
  }

  private async publish(): Promise<void> {
    this.releaseService = await lockService(this.root, this.serviceId);
    const directory = path.join(this.root, this.serviceId);
    try {
      await ensureServiceDirectory(this.root, this.serviceId);
      await this.listen(directory);
    } catch (error) {
      // Leave no empty directory behind for a discovery pass to walk into. `rmdir`
      // refuses a non-empty directory, so a live service's files are never touched.
      await rmdir(directory).catch(() => undefined);
      if (!this.resolvedDirectory) {
        this.releaseService?.();
        this.releaseService = null;
        process.off("exit", this.exitCleanup);
      }
      throw error;
    }
  }

  private async listen(directory: string): Promise<void> {
    const socketPath = path.join(directory, SOCKET_FILE_NAME);
    assertSocketPathFits(socketPath);
    await claimSocketPath(socketPath);
    await new Promise<void>((resolve, reject) => {
      const onError = (error: Error) => reject(error);
      this.server.once("error", onError);
      this.server.listen(socketPath, () => {
        this.server.off("error", onError);
        this.resolvedDirectory = directory;
        resolve();
      });
    });
    // Linux enforces unix socket permissions on connect, and listen() applies the umask.
    await chmod(socketPath, 0o600);
    const manifest = { name: this.manifestName, socket: SOCKET_FILE_NAME } as {
      name: string;
      icon?: string;
      socket: string;
    };
    if (this.icon !== undefined) manifest.icon = this.icon;
    // Advertise only once the socket accepts connections, so a readable manifest is connectable.
    await writeServiceManifest(directory, manifest);
  }

  private accept(socket: Socket): void {
    if (this.closed) { socket.destroy(); return; }
    this.clients.add(socket);
    const lines = new JsonlFramer((line) => receiveLine(line));
    const disconnect = () => {
      this.clients.delete(socket);
      this.outbound.get(socket)?.close();
      lines.close();
      socket.off("data", receiveChunk);
      socket.off("end", endInput);
      socket.destroy();
    };
    socket.on("error", disconnect);
    socket.on("close", disconnect);
    socket.setNoDelay(true);
    const outbound = new OutboundWriter((text, complete) => { socket.write(text, complete); }, disconnect);
    this.outbound.set(socket, outbound);
    const receiveLine = (line: string): boolean => {
      if (this.closed || socket.destroyed) return false;
      if (!line.trim()) return true;
      try {
        const value: unknown = JSON.parse(line);
        if (this.resources && isResourceFrame(value)) {
          if (value.kind === "request") outbound.send(`${JSON.stringify(this.resources.respond(value))}\n`);
          return true;
        }
        const parsed = A2uiClientMessageSchema.safeParse(value);
        if (!parsed.success || parsed.data.version !== "v0.9") {
          throw new Error(parsed.success ? `unsupported version: ${parsed.data.version}` : "expected an A2UI v0.9 client message");
        }
        const message = value as JsonObject;
        if ("error" in message) {
          // The schema can omit a missing generic code or strip an extra action.
          if ("action" in message || !("error" in parsed.data) || !("code" in parsed.data.error)) {
            throw new Error("invalid client error report");
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
      try { lines.write(chunk); }
      catch { disconnect(); }
    };
    const endInput = () => {
      try { lines.end(); }
      catch { disconnect(); }
    };
    socket.on("data", receiveChunk);
    socket.on("end", endInput);
    outbound.start(encodeReplay(this.snapshot.messages(), this.resources ? resourceHello() : undefined, "\n"));
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
    return new Promise((resolve, reject) => this.waiting.push({ resolve, reject }));
  }

  private flushReceivers(): void {
    for (const waiter of this.waiting.splice(0)) {
      if (this.receiveError) waiter.reject(this.receiveError);
      else waiter.resolve({ value: undefined, done: true });
    }
  }
}
