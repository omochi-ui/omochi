interface Pending { resolve(reference: string): void; reject(error: Error): void }

/** Native images/1 references and HTTP transfer; no bitmap/renderer ownership here. */
export class NativeImages {
  private next = 0;
  private stopped = false;
  private readonly pending = new Map<number, Pending>();
  constructor(private readonly endpoint: string, private readonly send: (value: unknown) => void) {}
  response(value: Record<string, unknown>): void {
    if (!Number.isSafeInteger(value.id)) throw new Error('Invalid image response');
    const pending = this.pending.get(value.id as number);
    if (!pending) return;
    if (typeof value.reference === 'string' && /^[a-f0-9]{32}$/.test(value.reference)) pending.resolve(value.reference);
    else pending.reject(new Error('Image request failed'));
  }
  close(): void {
    this.stopped = true;
    for (const pending of this.pending.values()) pending.reject(new Error('Image session closed'));
    this.pending.clear();
  }
  async fetch(handle: string, uri: string, signal: AbortSignal): Promise<Response> {
    if (this.stopped || uri.length > 16 * 1024) throw new Error('Image source unavailable');
    signal.throwIfAborted();
    const id = ++this.next;
    let released = false;
    const release = () => {
      if (released) return;
      released = true;
      if (!this.stopped) this.send({ omochi: 'images/1', kind: 'release', id });
    };
    const abort = () => { this.pending.get(id)?.reject(new Error('Image cancelled')); release(); };
    signal.addEventListener('abort', abort, { once: true });
    const finish = () => { signal.removeEventListener('abort', abort); this.pending.delete(id); release(); };
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
      const reader = response.body?.getReader();
      if (!reader) throw new Error('Empty image response');
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
    } catch (error) { finish(); throw error; }
  }
}
