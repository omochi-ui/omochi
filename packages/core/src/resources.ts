import { randomUUID } from 'node:crypto'
import type { IncomingMessage, ServerResponse } from 'node:http'

export const RESOURCE_PROTOCOL = 'resources/1'
export const RESOURCE_CHUNK = 24 * 1024
export type ResourceFrame = { omochi: typeof RESOURCE_PROTOCOL; kind: string; id?: string; op?: string; resource?: string; offset?: number; length?: number; status?: number; size?: number; mime?: string; etag?: string; data?: string; service?: string }
export function isResourceFrame(value: unknown): value is ResourceFrame {
  return !!value && typeof value === 'object' && (value as ResourceFrame).omochi === RESOURCE_PROTOCOL
}
export const resourceHello = (): ResourceFrame => ({ omochi: RESOURCE_PROTOCOL, kind: 'hello' })
export class ResourceError extends Error {
  constructor(readonly status: number, message: string) { super(message) }
}
export type ResourceRequest = { op: 'stat' | 'read'; resource: string; offset?: number; length?: number; service?: string }
export function validResourceRequest(frame: ResourceFrame): boolean {
  return frame.kind === 'request' && typeof frame.id === 'string' && /^[\w-]{1,80}$/.test(frame.id)
    && typeof frame.resource === 'string' && /^[a-f0-9]{32}$/.test(frame.resource)
    && (frame.op === 'stat' || (frame.op === 'read' && Number.isSafeInteger(frame.offset) && Number(frame.offset) >= 0
      && Number.isSafeInteger(frame.length) && Number(frame.length) > 0 && Number(frame.length) <= RESOURCE_CHUNK))
}
/** One broker per upstream connection: responses can never cross service boundaries. */
export class ResourceClient {
  supported = false
  private closed = false
  private pending = new Map<string, { resolve(value: ResourceFrame): void; reject(error: Error): void; timer: ReturnType<typeof setTimeout> }>()
  constructor(private send: (frame: ResourceFrame) => void) {}
  receive(value: unknown): boolean {
    if (!isResourceFrame(value)) return false
    if (this.closed) return true
    if (value.kind === 'hello') this.supported = true
    if (value.kind === 'response' && typeof value.id === 'string') {
      const pending = this.pending.get(value.id)
      if (pending) {
        this.pending.delete(value.id); clearTimeout(pending.timer)
        if (value.status !== 200) pending.reject(new ResourceError([400, 404, 416, 500, 501, 503, 504].includes(Number(value.status)) ? Number(value.status) : 502, 'resource request failed'))
        else pending.resolve(value)
      }
    }
    return true
  }
  request(request: ResourceRequest, signal?: AbortSignal): Promise<ResourceFrame> {
    if (this.closed) return Promise.reject(new ResourceError(503, 'resource connection closed'))
    if (!this.supported) return Promise.reject(new ResourceError(501, 'resources unsupported'))
    if (signal?.aborted) return Promise.reject(new ResourceError(499, 'cancelled'))
    if (this.pending.size >= 8) return Promise.reject(new ResourceError(503, 'too many resource requests'))
    const id = randomUUID()
    return new Promise<ResourceFrame>((resolve, reject) => {
      const cancel = () => { const p = this.pending.get(id); if (p) { this.pending.delete(id); clearTimeout(p.timer); p.reject(new ResourceError(499, 'cancelled')) } }
      const cleanup = () => signal?.removeEventListener('abort', cancel)
      const timer = setTimeout(() => { this.pending.delete(id); cleanup(); reject(new ResourceError(504, 'resource timeout')) }, 15000)
      this.pending.set(id, { timer, resolve: value => { cleanup(); resolve(value) }, reject: error => { cleanup(); reject(error) } })
      signal?.addEventListener('abort', cancel, { once: true })
      try { this.send({ omochi: RESOURCE_PROTOCOL, kind: 'request', id, ...request }) } catch { this.close() }
    })
  }
  close(): void {
    this.closed = true
    this.supported = false
    for (const p of this.pending.values()) { clearTimeout(p.timer); p.reject(new ResourceError(503, 'resource connection closed')) }
    this.pending.clear()
  }
}
export type ResourceFetch = (route: string, request: ResourceRequest, signal: AbortSignal) => Promise<ResourceFrame>

/** Deliberately rejects cross-site media requests without an explicitly allowed Origin. */
export function resourceOriginAllowed(request: IncomingMessage, allowed?: (origin: string | undefined) => boolean): boolean {
  const origin = request.headers.origin
  if (origin && !(allowed?.(origin) ?? /^https?:\/\/(localhost|127\.0\.0\.1|\[::1\])(:\d+)?$/.test(origin))) return false
  return request.headers['sec-fetch-site'] !== 'cross-site' || !!origin && !!allowed?.(origin)
}
export function serveResources(request: IncomingMessage, response: ServerResponse, fetch: ResourceFetch,
  allowed?: (origin: string | undefined) => boolean): boolean {
  const path = new URL(request.url ?? '/', 'http://localhost').pathname
  if (!path.startsWith('/resources/')) return false
  if (!resourceOriginAllowed(request, allowed)) { response.writeHead(403).end(); return true }
  if (request.method !== 'GET' && request.method !== 'HEAD') { response.writeHead(405, { Allow: 'GET, HEAD' }).end(); return true }
  const match = /^\/resources\/([^/]+)\/([a-f0-9]{32})$/.exec(path)
  if (!match) { response.writeHead(404).end(); return true }
  const controller = new AbortController()
  response.once('close', () => controller.abort())
  void (async () => {
    const route = decodeURIComponent(match[1]!)
    const resource = match[2]!
    const metadata = await fetch(route, { op: 'stat', resource }, controller.signal)
    const size = Number(metadata.size), mime = metadata.mime, etag = metadata.etag
    if (!Number.isSafeInteger(size) || size < 0 || typeof mime !== 'string' || !/^(image|audio|video)\/[a-zA-Z0-9.+-]+$/.test(mime)
      || typeof etag !== 'string' || !/^"[a-f0-9]{32}"$/.test(etag)) throw new ResourceError(502, 'invalid resource metadata')
    let start = 0, end = size - 1, partial = false
    const range = request.headers.range
    if (request.method === 'GET' && range && !range.includes(',') && (!request.headers['if-range'] || request.headers['if-range'] === etag)) {
      const parts = /^bytes=(\d*)-(\d*)$/.exec(range)
      if (parts && (parts[1] || parts[2])) {
        start = parts[1] ? Number(parts[1]) : Math.max(0, size - Number(parts[2]))
        end = parts[1] && parts[2] ? Math.min(size - 1, Number(parts[2])) : size - 1
        if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start > end || start >= size) {
          response.writeHead(416, { 'Content-Range': `bytes */${size}`, 'Cache-Control': 'no-store' }).end(); return
        }
        partial = true
      }
    }
    const headers: Record<string, string | number> = { 'Content-Type': mime, 'Content-Length': Math.max(0, end - start + 1),
      'Accept-Ranges': 'bytes', ETag: etag, 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff',
      'Content-Security-Policy': "sandbox; default-src 'none'", Vary: 'Origin, Sec-Fetch-Site' }
    if (request.headers.origin) headers['Access-Control-Allow-Origin'] = request.headers.origin
    if (partial) headers['Content-Range'] = `bytes ${start}-${end}/${size}`
    response.writeHead(partial ? 206 : 200, headers)
    if (request.method === 'HEAD') { response.end(); return }
    while (start <= end && !controller.signal.aborted) {
      const length = Math.min(RESOURCE_CHUNK, end - start + 1)
      const chunk = await fetch(route, { op: 'read', resource, offset: start, length }, controller.signal)
      if (typeof chunk.data !== 'string' || chunk.data.length > 32768 || !/^[A-Za-z0-9+/]*={0,2}$/.test(chunk.data)) throw new ResourceError(502, 'invalid resource chunk')
      const data = Buffer.from(chunk.data, 'base64')
      if (data.length !== length || data.toString('base64') !== chunk.data) throw new ResourceError(502, 'short resource read')
      if (!response.write(data)) await new Promise<void>(resolve => {
        const done = () => { response.off('drain', done); response.off('close', done); resolve() }
        response.once('drain', done); response.once('close', done)
      })
      start += length
    }
    response.end()
  })().catch(error => {
    if (response.headersSent) response.destroy()
    else response.writeHead(error instanceof ResourceError ? error.status : 500).end()
  })
  return true
}
