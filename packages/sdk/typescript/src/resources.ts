import { randomBytes } from 'node:crypto';
import { mkdtempSync, openSync, readSync, writeSync, closeSync, unlinkSync, rmdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { isResourceFrame, validResourceRequest, RESOURCE_PROTOCOL, type ResourceFrame } from '@omochi-ui/core';

export const RESOURCE_MIME: Record<string, string> = { '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif', '.webp': 'image/webp', '.svg': 'image/svg+xml', '.avif': 'image/avif', '.mp3': 'audio/mpeg', '.wav': 'audio/wav', '.ogg': 'audio/ogg', '.m4a': 'audio/mp4', '.mp4': 'video/mp4', '.webm': 'video/webm', '.mov': 'video/quicktime' };
export class ResourceRegistry {
  private directory: string | null = null;
  private entries = new Map<string, { filename: string; size: number; mime: string }>();
  private total = 0;
  private closed = false;
  constructor(readonly maxFileBytes = 1024 ** 3, readonly maxTotalBytes = 4 * 1024 ** 3) {}
  file(filename: string, mimeType = RESOURCE_MIME[path.extname(filename).toLowerCase()]): string {
    const fd = openSync(filename, 'r');
    try { return this.register(mimeType, (buffer) => readSync(fd, buffer)); } finally { closeSync(fd); }
  }
  bytes(data: Uint8Array, mimeType: string): string {
    let offset = 0;
    return this.register(mimeType, buffer => { const length = Math.min(buffer.length, data.length - offset); buffer.set(data.subarray(offset, offset + length)); offset += length; return length; });
  }
  private register(mime: string | undefined, read: (buffer: Buffer) => number): string {
    if (this.closed) throw new Error('resource registry closed');
    if (!mime || !/^(image|audio|video)\/[a-zA-Z0-9.+-]+$/.test(mime)) throw new Error('explicit media MIME type required');
    this.directory ??= mkdtempSync(path.join(tmpdir(), 'omochi-resources-'));
    const id = randomBytes(16).toString('hex'), filename = path.join(this.directory, id);
    const fd = openSync(filename, 'wx', 0o600);
    let size = 0;
    try {
      const buffer = Buffer.alloc(64 * 1024);
      for (;;) {
        const count = read(buffer); if (!count) break;
        size += count;
        if (size > this.maxFileBytes || this.total + size > this.maxTotalBytes) throw new Error('resource storage limit exceeded');
        let offset = 0; while (offset < count) offset += writeSync(fd, buffer, offset, count - offset);
      }
    } catch (error) { closeSync(fd); unlinkSync(filename); throw error; }
    closeSync(fd); this.total += size; this.entries.set(id, { filename, size, mime });
    return `omochi://resources/${id}`;
  }
  release(uri: string): void {
    const id = /^omochi:\/\/resources\/([a-f0-9]{32})$/.exec(uri)?.[1];
    const entry = id ? this.entries.get(id) : undefined;
    if (entry && id) { this.entries.delete(id); this.total -= entry.size; unlinkSync(entry.filename); }
  }
  close(): void {
    for (const id of this.entries.keys()) this.release(`omochi://resources/${id}`);
    if (this.directory) { rmdirSync(this.directory); this.directory = null; }
    this.closed = true;
  }
  respond(frame: ResourceFrame): ResourceFrame {
    const response: ResourceFrame = { omochi: RESOURCE_PROTOCOL, kind: 'response', id: frame.id, status: 200 };
    if (!validResourceRequest(frame)) return { ...response, status: 400 };
    const entry = this.entries.get(String(frame.resource));
    if (!entry) return { ...response, status: 404 };
    if (frame.op === 'stat') return { ...response, size: entry.size, mime: entry.mime, etag: `"${frame.resource}"` };
    const offset = Number(frame.offset), length = Number(frame.length);
    if (offset + length > entry.size) return { ...response, status: 416 };
    try {
      const fd = openSync(entry.filename, 'r'), buffer = Buffer.alloc(length);
      try { const count = readSync(fd, buffer, 0, length, offset); return { ...response, data: buffer.subarray(0, count).toString('base64') }; }
      finally { closeSync(fd); }
    } catch { return { ...response, status: 500 }; }
  }
}
/** Keep reading while an action executes, serving resource RPC independently. */
export async function* resourceMessages<T>(input: AsyncIterable<T>, send: (frame: ResourceFrame) => Promise<void>, registry?: ResourceRegistry): AsyncIterable<T> {
  const queue: T[] = []; let done = false; let stopped = false; let failure: unknown; let wake: (() => void) | undefined;
  const pump = (async () => {
    try { for await (const value of input) {
      if (stopped) break;
      if (registry && isResourceFrame(value)) { if (value.kind === 'request') await send(registry.respond(value)); }
      else { queue.push(value); wake?.(); }
    } } catch (error) { failure = error; } finally { done = true; wake?.(); }
  })();
  try { while (!done || queue.length) { if (queue.length) yield queue.shift()!; else await new Promise<void>(resolve => { wake = resolve; }); } if (failure) throw failure; }
  finally { stopped = true; void pump; }
}
