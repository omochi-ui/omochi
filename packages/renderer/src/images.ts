import { NativeImages } from './native-images.js';

/** Browser-side lifetime and memory limits for the Hub's scoped image protocol. */
const MAX_BYTES = 8 * 1024 * 1024;
const MAX_PIXELS = 4 * 1024 * 1024;
const MAX_DECODED_BYTES = 32 * 1024 * 1024;
let loading = 0;
let decodedBytes = 0;

export interface BrowserImage {
  bitmap: ImageBitmap;
  dispose(): void;
}
export class BrowserImageScope {
  private readonly lifetime = new AbortController();
  readonly signal = this.lifetime.signal;
  private readonly retained = new Set<BrowserImage>();
  constructor(private readonly session: BrowserImages, readonly token: string) {}
  revoke(): void {
    this.lifetime.abort();
    for (const image of this.retained) image.dispose();
  }
  async load(uri: string, cancel: AbortSignal): Promise<BrowserImage> {
    if (loading >= 2) throw new Error('Image reader busy');
    const retainedLifetime = AbortSignal.any([cancel, this.lifetime.signal]);
    const lifetime = AbortSignal.any([retainedLifetime, AbortSignal.timeout(25000)]);
    lifetime.throwIfAborted();
    loading++;
    let charge = 0;
    let bitmap: ImageBitmap | undefined;
    let response: Response | undefined;
    try {
      response = await this.session.fetch(this.token, uri, lifetime);
      const width = Number(response.headers.get('x-omochi-image-width'));
      const height = Number(response.headers.get('x-omochi-image-height'));
      if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width < 1 || height < 1 ||
          width > 8192 || height > 8192 || width * height > MAX_PIXELS) throw new Error('Invalid image dimensions');
      // Account for both the temporary bitmap and the canvas backing store.
      charge = width * height * 8;
      if (decodedBytes + charge > MAX_DECODED_BYTES) { charge = 0; throw new Error('Image memory limit reached'); }
      decodedBytes += charge;
      const reader = response.body?.getReader();
      if (!reader) throw new Error('Empty image response');
      const parts: Uint8Array<ArrayBuffer>[] = [];
      let bytes = 0;
      const abortRead = () => { void reader.cancel().catch(() => {}); };
      lifetime.addEventListener('abort', abortRead, { once: true });
      try {
        while (true) {
          const next = await reader.read();
          lifetime.throwIfAborted();
          if (next.done) break;
          bytes += next.value.byteLength;
          if (bytes > MAX_BYTES) throw new Error('Image size limit reached');
          parts.push(new Uint8Array(next.value));
        }
      } finally {
        lifetime.removeEventListener('abort', abortRead);
        await reader.cancel().catch(() => {}); reader.releaseLock();
      }
      const mime = response.headers.get('content-type') ?? '';
      if (!['image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/bmp'].includes(mime)) throw new Error('Unsupported image');
      bitmap = await createImageBitmap(new Blob(parts, { type: mime }));
      lifetime.throwIfAborted();
      if (bitmap.width !== width || bitmap.height !== height) throw new Error('Invalid image dimensions');
      let retained = true;
      const result: BrowserImage = { bitmap, dispose: () => {
        if (!retained) return;
        retained = false;
        retainedLifetime.removeEventListener('abort', dispose);
        this.retained.delete(result);
        result.bitmap.close();
        decodedBytes -= charge;
      } };
      const dispose = () => result.dispose();
      this.retained.add(result);
      retainedLifetime.addEventListener('abort', dispose, { once: true });
      bitmap = undefined;
      return result;
    } catch (error) {
      bitmap?.close();
      decodedBytes -= charge;
      throw error;
    } finally { await response?.body?.cancel().catch(() => {}); loading--; }
  }
}
/** Scoped decoding; protocol I/O is supplied by the selected live-session adapter. */
export class BrowserImages {
  private stopped = false;
  private readonly scopes = new Map<string, BrowserImageScope>();
  private readonly native: NativeImages | null;
  private readonly loader: (lease: string, uri: string, signal: AbortSignal) => Promise<Response>;
  constructor(loader: (lease: string, uri: string, signal: AbortSignal) => Promise<Response>);
  constructor(endpoint: string, send: (value: unknown) => void);
  constructor(loader: string | ((lease: string, uri: string, signal: AbortSignal) => Promise<Response>), send?: (value: unknown) => void) {
    this.native = typeof loader === 'string' ? new NativeImages(loader, send!) : null;
    this.loader = typeof loader === 'string' ? (lease, uri, signal) => this.native!.fetch(lease, uri, signal) : loader;
  }
  scope(surface: string): BrowserImageScope | undefined { return this.scopes.get(surface); }
  diagnostics() { return { scopes: this.scopes.size, loading, decodedBytes }; }
  update(surface: string, token: string | null): void {
    const previous = this.scopes.get(surface);
    if (previous?.token === token) return;
    previous?.revoke();
    this.scopes.delete(surface);
    if (token && !this.stopped) this.scopes.set(surface, new BrowserImageScope(this, token));
  }
  response(value: Record<string, unknown>): void { this.native?.response(value); }
  close(): void {
    this.stopped = true;
    for (const scope of this.scopes.values()) scope.revoke();
    this.scopes.clear();
    this.native?.close();
  }
  async fetch(handle: string, uri: string, signal: AbortSignal): Promise<Response> {
    if (this.stopped || uri.length > 16 * 1024) throw new Error('Image source unavailable');
    signal.throwIfAborted();
    return this.loader(handle, uri, signal);
  }
}
