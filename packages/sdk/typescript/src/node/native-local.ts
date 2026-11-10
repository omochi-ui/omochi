import { createRequire } from "node:module";
import { Duplex } from "node:stream";

export type LocalEndpoint = { kind: "unix"; path: string } | { kind: "namedPipe"; name: string };
interface NativeContext { request(command: string, data: Buffer): Promise<Buffer>; }
interface Binding { createPrivateDirectory(path: string): void; runtimeRoot(): string; abiVersion(): number; version(): string; LocalContext: new () => NativeContext; }
let context: NativeContext | undefined;
let binding: Binding | undefined;
function load(): Binding {
  if (binding) return binding;
  const require = createRequire(import.meta.url);
  const libc = process.platform === "linux" ? (process.report?.getReport() as { header?: { glibcVersionRuntime?: string } }).header?.glibcVersionRuntime ? "-gnu" : "-musl" : "";
  const name = `@omochi-ui/local-${process.platform}-${process.arch}${libc}`;
  try {
    if (process.env.OMOCHI_LOCAL_NODE) binding = require(process.env.OMOCHI_LOCAL_NODE) as Binding;
    else {
      try { binding = require(name) as Binding; }
      catch { binding = require("../../native/omochi_local.node") as Binding; }
    }
  }
  catch (cause) { throw new Error(`Missing Omochi native adapter ${name}; install with optional dependencies enabled or build SDK adapters`, { cause }); }
  if (binding.version() !== require("../../package.json").version || binding.abiVersion() !== 1) { binding = undefined; throw new Error("Incompatible Omochi native ABI"); }
  context = new binding.LocalContext();
  return binding;
}
export function nativeRequest(op: string, id = 0, data: Buffer = Buffer.alloc(0), options: Record<string, unknown> = {}): Promise<Buffer> {
  load();
  return context!.request(JSON.stringify({ op, id, ...options }), data);
}
export class NativeDuplex extends Duplex {
  private reading = false;
  constructor(private readonly handle: number) { super({ allowHalfOpen: false }); }
  override _read(): void {
    if (this.reading || this.destroyed) return;
    this.reading = true;
    nativeRequest("read", this.handle).then((bytes) => {
      this.reading = false;
      if (this.destroyed) return;
      if (!bytes.length) this.push(null);
      else if (this.push(bytes)) this._read();
    }, (error: Error) => { this.reading = false; this.destroy(error); });
  }
  override _write(chunk: Buffer, _encoding: BufferEncoding, complete: (error?: Error | null) => void): void {
    void (async () => {
      for (let offset = 0; offset < chunk.length; offset += 65536) {
        await nativeRequest("write", this.handle, chunk.subarray(offset, offset + 65536));
      }
    })().then(() => complete(), complete);
  }
  override _destroy(error: Error | null, complete: (error?: Error | null) => void): void {
    nativeRequest("close", this.handle).then(() => complete(error), (failure: Error) => complete(error ?? failure));
  }
}

export function omochiRuntimeRoot(): string { return load().runtimeRoot(); }

export function createPrivateDirectory(path: string): void { load().createPrivateDirectory(path); }
