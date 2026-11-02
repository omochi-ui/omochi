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
  constructor(private readonly session: BrowserImages, readonly token: string) {}
  revoke(): void { this.lifetime.abort(); }
  async load(uri: string, cancel: AbortSignal): Promise<BrowserImage> {
    if (loading >= 2) throw new Error('Image reader busy');
    const lifetime = AbortSignal.any([cancel, this.lifetime.signal, AbortSignal.timeout(25000)]);
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
      try {
        while (true) {
          const next = await reader.read();
          if (next.done) break;
          lifetime.throwIfAborted();
          bytes += next.value.byteLength;
          if (bytes > MAX_BYTES) throw new Error('Image size limit reached');
          parts.push(new Uint8Array(next.value));
        }
      } finally { await reader.cancel().catch(() => {}); reader.releaseLock(); }
      const mime = response.headers.get('content-type') ?? '';
      if (!['image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/bmp'].includes(mime)) throw new Error('Unsupported image');
      bitmap = await createImageBitmap(new Blob(parts, { type: mime }));
      lifetime.throwIfAborted();
      if (bitmap.width !== width || bitmap.height !== height) throw new Error('Invalid image dimensions');
      let retained = true;
      const result: BrowserImage = { bitmap, dispose: () => {
        if (!retained) return;
        retained = false;
        result.bitmap.close();
        decodedBytes -= charge;
      } };
      bitmap = undefined;
      return result;
    } catch (error) {
      bitmap?.close();
      decodedBytes -= charge;
      throw error;
    } finally { await response?.body?.cancel().catch(() => {}); loading--; }
  }
}
interface Pending {
  resolve(reference: string): void;
  reject(error: Error): void;
}
/** Created only after an images/1 hello. An unsupported old server keeps its legacy catalog. */
export class BrowserImages {
  private next = 0;
  private stopped = false;
  private readonly pending = new Map<number, Pending>();
  private readonly scopes = new Map<string, BrowserImageScope>();
  constructor(private readonly endpoint: string, private readonly send: (value: unknown) => void) {}
  scope(surface: string): BrowserImageScope | undefined { return this.scopes.get(surface); }
  update(surface: string, token: string | null): void {
    const previous = this.scopes.get(surface);
    if (previous?.token === token) return;
    previous?.revoke();
    this.scopes.delete(surface);
    if (token) this.scopes.set(surface, new BrowserImageScope(this, token));
  }
  response(value: Record<string, unknown>): void {
    if (!Number.isSafeInteger(value.id)) throw new Error('Invalid image response');
    const pending = this.pending.get(value.id as number);
    if (!pending) return; // The component may have cancelled before this response arrived.
    if (typeof value.reference === 'string' && /^[a-f0-9]{32}$/.test(value.reference)) pending.resolve(value.reference);
    else pending.reject(new Error('Image request failed'));
  }
  close(): void {
    this.stopped = true;
    for (const scope of this.scopes.values()) scope.revoke();
    this.scopes.clear();
    for (const pending of this.pending.values()) pending.reject(new Error('Image session closed'));
    this.pending.clear();
  }
  async fetch(handle: string, uri: string, signal: AbortSignal): Promise<Response> {
    if (this.stopped || uri.length > 16 * 1024) throw new Error('Image source unavailable');
    signal.throwIfAborted();
    const id = ++this.next;
    const release = () => { if (!this.stopped) this.send({ omochi: 'images/1', kind: 'release', id }); };
    const abort = () => { this.pending.get(id)?.reject(new Error('Image cancelled')); release(); };
    signal.addEventListener('abort', abort, { once: true });
    try {
      const reference = await new Promise<string>((resolve, reject) => {
        this.pending.set(id, { resolve, reject });
        this.send({ omochi: 'images/1', kind: 'image', id, handle, uri });
      });
      signal.throwIfAborted();
      const url = new URL(this.endpoint, globalThis.location?.href ?? 'http://localhost');
      url.protocol = url.protocol === 'wss:' ? 'https:' : 'http:';
      url.pathname = `/images/${reference}`;
      url.search = ''; url.hash = ''; url.username = ''; url.password = '';
      const response = await fetch(url, { signal, credentials: 'omit', cache: 'no-store', redirect: 'error', referrerPolicy: 'no-referrer' });
      if (!response.ok) { await response.body?.cancel(); throw new Error('Image transfer failed'); }
      // The reference remains owned until the body completes or is cancelled.
      const reader = response.body?.getReader();
      if (!reader) throw new Error('Empty image response');
      const finish = () => { signal.removeEventListener('abort', abort); this.pending.delete(id); release(); };
      const body = new ReadableStream<Uint8Array>({
        async pull(controller) {
          try {
            const next = await reader.read();
            if (next.done) { finish(); controller.close(); }
            else controller.enqueue(next.value);
          } catch (error) { finish(); controller.error(error); }
        },
        async cancel() { try { await reader.cancel(); } finally { finish(); } },
      });
      return new Response(body, { headers: response.headers });
    } catch (error) {
      signal.removeEventListener('abort', abort);
      this.pending.delete(id);
      release();
      throw error;
    }
  }
}
