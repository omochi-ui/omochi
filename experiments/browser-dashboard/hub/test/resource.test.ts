import assert from 'node:assert/strict';
import { jest } from 'bun:test';
import { test, waitFor, type TestContext } from '@omochi-ui/core/testing';
import { SessionFault } from '@omochi-ui/core/browser';
import type { Connection, OpenHeader, Stream } from '@omochi-ui/browser-transport';
import { PngResources, type ResourceScope } from '../src/resource.js';

const resourceId = 'a'.repeat(32), uri = `omochi://resources/${resourceId}`;
const png = Uint8Array.from(Buffer.from('89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c63f8cfc0f01f00050001ff89993d1d0000000049454e44ae426082', 'hex'));
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
function abortable<T>(promise: Promise<T>, signal: AbortSignal): Promise<T> {
  return new Promise((resolve, reject) => {
    const abort = () => reject(new SessionFault('Cancelled'));
    if (signal.aborted) { abort(); return; }
    signal.addEventListener('abort', abort, { once: true });
    void promise.then(value => { signal.removeEventListener('abort', abort); resolve(value); }, error => { signal.removeEventListener('abort', abort); reject(error); });
  });
}
function fixture(t: TestContext) {
  const resources = new PngResources(), leaseController = new AbortController(), calls: Record<string, unknown>[] = [];
  const streams: { header: OpenHeader; cancelled: boolean }[] = [];
  const gates = { stat: null as ReturnType<typeof deferred<void>> | null, read: null as ReturnType<typeof deferred<void>> | null, open: null as ReturnType<typeof deferred<void>> | null };
  let content = png;
  let returnedSize: number | null = null;
  let metadata: Record<string, unknown> = {}, chunk: Record<string, unknown> = {};
  let corrupt = false;
  const connection: Connection = {
    remoteId: 'b'.repeat(64), closed: () => new Promise(() => {}), async close() {},
    async open(header, signal) {
      if (gates.open) await abortable(gates.open.promise, signal);
      const opened = { header, cancelled: false }; streams.push(opened);
      let request: Record<string, unknown> = {};
      const stream: Stream = {
        async send(json, signal) { if (signal.aborted) throw new SessionFault('Cancelled'); request = JSON.parse(json); calls.push(request); },
        async finish() {}, async cancel() { opened.cancelled = true; },
        async receive(signal) {
          const op = request.op as 'stat' | 'read'; if (gates[op]) await abortable(gates[op]!.promise, signal);
          const base = { omochi: 'resources/1', kind: 'response', id: 'gateway_replaced_id-123', status: 200 };
          let data = content.slice(Number(request.offset), Number(request.offset) + Number(request.length));
          if (corrupt) { data = data.slice(); data[0] = 0; }
          const value = op === 'stat' ? { ...base, size: returnedSize ?? content.length, mime: 'image/png', etag: `"${resourceId}"`, ...metadata } :
            { ...base, data: Buffer.from(data).toString('base64'), ...chunk };
          const json = JSON.stringify(value); return { json, wireBytes: json.length };
        },
      }; return stream;
    },
  };
  const scope: ResourceScope = {
    task: Object.freeze({ gatewayId: connection.remoteId, gatewayEpoch: 1, serviceId: 's'.repeat(64), generation: '18446744073709551615', serviceInstance: 2 }),
    lease: Object.freeze({ id: crypto.randomUUID(), surfaceId: `${connection.remoteId}~${'s'.repeat(64)}/main/nested`, signal: leaseController.signal }),
    connection,
    current() { if (leaseController.signal.aborted) throw new SessionFault('ScopeExpired'); },
  };
  t.after(() => resources.close());
  return { resources, scope, leaseController, calls, streams, gates,
    load: (value = uri, signal = new AbortController().signal) => resources.load(scope, value, signal),
    set metadata(value: Record<string, unknown>) { metadata = value; }, set chunk(value: Record<string, unknown>) { chunk = value; },
    set size(value: number) { returnedSize = value; }, set content(value: Uint8Array<ArrayBuffer>) { content = value; },
    set corrupt(value: boolean) { corrupt = value; } };
}

test('PNG resources use captured service/max-u64 generation, independent one-request streams and gateway-replaced response IDs', async t => {
  const f = fixture(t), response = await f.load();
  assert.equal(response.headers.get('content-type'), 'image/png'); assert.equal(response.headers.get('x-omochi-image-width'), '1');
  assert.equal(response.headers.get('x-omochi-image-height'), '1');
  assert.deepEqual(f.calls.map(({ id: _id, ...request }) => request), [
    { omochi: 'resources/1', kind: 'request', op: 'stat', resource: resourceId },
    { omochi: 'resources/1', kind: 'request', op: 'read', resource: resourceId, offset: 0, length: png.length },
  ]);
  assert.equal(new Set(f.calls.map(request => request.id)).size, 2);
  assert(f.streams.every(stream => stream.cancelled && JSON.stringify(stream.header) === JSON.stringify({ kind: 'resource', service: 's'.repeat(64), generation: '18446744073709551615' })));
  assert.deepEqual(f.resources.diagnostics, { activeReads: 1, encodedBytes: png.length }, 'a returned unread body retains its encoded reservation');
  assert.deepEqual(new Uint8Array(await response.arrayBuffer()), png);
  assert.deepEqual(f.resources.diagnostics, { activeReads: 0, encodedBytes: 0 });
});

test('successful multi-chunk PNG transfer uses exact sequential offsets and disposes each captured stream before the next opens', async t => {
  const f = fixture(t), payload = new TextEncoder().encode('Description\0' + 'A'.repeat(30000));
  const text = new Uint8Array(payload.length + 12), view = new DataView(text.buffer);
  view.setUint32(0, payload.length); text.set(new TextEncoder().encode('tEXt'), 4); text.set(payload, 8);
  let crc = 0xffffffff;
  for (const byte of text.subarray(4, -4)) { crc ^= byte; for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0); }
  view.setUint32(text.length - 4, (crc ^ 0xffffffff) >>> 0);
  const content = new Uint8Array(png.length + text.length); content.set(png.subarray(0, 33)); content.set(text, 33); content.set(png.subarray(33), 33 + text.length);
  f.content = content;
  const response = await f.load();
  assert.deepEqual(f.calls.filter(request => request.op === 'read').map(({ offset, length }) => ({ offset, length })), [
    { offset: 0, length: 24576 }, { offset: 24576, length: content.length - 24576 },
  ]);
  assert.equal(f.streams.length, 3); assert(f.streams.every(stream => stream.cancelled));
  assert.deepEqual(new Uint8Array(await response.arrayBuffer()), content);
});

test('resource URI, metadata and exact canonical chunk lengths are validated before image decoding or further reads', async t => {
  const f = fixture(t);
  for (const unsupported of ['https://example.test/image.png', uri + '?x', uri + '#x', 'omochi://resources/' + 'A'.repeat(32), '/images/abc', 'data:image/png;base64,AA==']) await assert.rejects(f.load(unsupported), { code: 'UnsupportedMedia' });
  assert.equal(f.calls.length, 0);
  for (const [metadata, code] of [
    [{ size: 32 }, 'InvalidImage'], [{ size: 1.5 }, 'InvalidImage'], [{ etag: resourceId }, 'InvalidImage'],
    [{ size: 8 * 1024 * 1024 + 1 }, 'CapacityExceeded'], [{ mime: 'image/jpeg' }, 'UnsupportedMedia'],
    [{ size: undefined }, 'ProtocolError'], [{ extra: true }, 'ProtocolError'], [{ id: 'not-an-ASCII-id\n' }, 'ProtocolError'],
    [{ status: 404, size: undefined, mime: undefined, etag: undefined }, 'SourceUnavailable'],
  ] as const) {
    f.metadata = metadata; const count: number = f.calls.length;
    await assert.rejects(f.load(), { code }); assert.equal(f.calls.length, count + 1);
    assert.deepEqual(f.resources.diagnostics, { activeReads: 0, encodedBytes: 0 });
  }
  f.metadata = {};
  for (const encoded of [Buffer.from(png).toString('base64').replace(/=+$/, ''), Buffer.from(png).toString('base64') + '\n', 'A'.repeat(4),
    // The PNG length is 70 (one data byte in the final group); these nonzero pad bits decode to the same final byte.
    Buffer.from(png).toString('base64').slice(0, -3) + 'h==',
  ]) { f.chunk = { data: encoded }; await assert.rejects(f.load(), { code: 'ProtocolError' }); }
  f.chunk = {}; f.corrupt = true; await assert.rejects(f.load(), { code: 'InvalidImage' });
  assert.deepEqual(f.resources.diagnostics, { activeReads: 0, encodedBytes: 0 });
});

test('resource admission has two active slots and reserves encoded bytes before read allocation without an implicit queue', async t => {
  const f = fixture(t); f.size = 8 * 1024 * 1024; f.gates.read = deferred<void>();
  const first = f.load().catch(error => error.code), second = f.load().catch(error => error.code);
  await waitFor(() => f.calls.filter(call => call.op === 'read').length === 2);
  assert.deepEqual(f.resources.diagnostics, { activeReads: 2, encodedBytes: 16 * 1024 * 1024 });
  await assert.rejects(f.load(), { code: 'CapacityExceeded' }); assert.equal(f.calls.length, 4);
  f.leaseController.abort(); assert.deepEqual(await Promise.all([first, second]), ['ScopeExpired', 'ScopeExpired']);
  assert(f.streams.every(stream => stream.cancelled)); assert.deepEqual(f.resources.diagnostics, { activeReads: 0, encodedBytes: 0 });
});

test('stat/open/read cancellation and same-ID replacement prevent all subsequent reads and release stream/encoded charges', async t => {
  for (const stage of ['open', 'stat', 'read'] as const) {
    const f = fixture(t); f.gates[stage] = deferred<void>();
    const pending = f.load().catch(error => error.code);
    if (stage !== 'open') await waitFor(() => f.calls.some(call => call.op === stage));
    f.leaseController.abort();
    assert.equal(await pending, 'ScopeExpired');
    const count = f.calls.length; f.gates[stage]!.resolve(); await Promise.resolve();
    assert.equal(f.calls.length, count); assert(f.streams.every(stream => stream.cancelled));
    await assert.rejects(f.load(), { code: 'ScopeExpired' }); assert.deepEqual(f.resources.diagnostics, { activeReads: 0, encodedBytes: 0 });
  }
  const f = fixture(t), cancel = new AbortController(); f.gates.read = deferred<void>();
  const pending = f.load(uri, cancel.signal).catch(error => error.code); await waitFor(() => f.calls.some(call => call.op === 'read'));
  cancel.abort(); assert.equal(await pending, 'Cancelled');
});

test('unread responses are revoked, explicitly cancelled, expired or closed without retaining profile capacity', async t => {
  const f = fixture(t); let response = await f.load();
  await response.body!.cancel(); assert.deepEqual(f.resources.diagnostics, { activeReads: 0, encodedBytes: 0 });
  response = await f.load(); f.leaseController.abort();
  await assert.rejects(response.arrayBuffer(), { code: 'ScopeExpired' });
  assert.deepEqual(f.resources.diagnostics, { activeReads: 0, encodedBytes: 0 });
  const next = fixture(t), received = await next.load();
  await next.resources.close(); await assert.rejects(received.arrayBuffer(), { code: 'Cancelled' });
  assert.deepEqual(next.resources.diagnostics, { activeReads: 0, encodedBytes: 0 });
});

test('resource exchange and whole PNG deadlines do not extend when a new chunk starts', async t => {
  jest.useFakeTimers(); t.after(() => jest.useRealTimers());
  const flush = async () => { for (let index = 0; index < 80; index++) await Promise.resolve(); };
  const f = fixture(t); f.gates.stat = deferred<void>();
  const first = f.load().catch(error => error.code); await flush();
  jest.advanceTimersByTime(14999); await flush(); assert.equal(f.resources.diagnostics.activeReads, 1);
  jest.advanceTimersByTime(1); await flush(); assert.equal(await first, 'Timeout');
  const next = fixture(t); next.gates.stat = deferred<void>(); next.gates.read = deferred<void>();
  const second = next.load().catch(error => error.code); await flush();
  jest.advanceTimersByTime(14000); next.gates.stat.resolve(); await flush();
  assert.equal(next.calls.at(-1)?.op, 'read'); jest.advanceTimersByTime(5999); await flush();
  assert.equal(next.resources.diagnostics.activeReads, 1);
  jest.advanceTimersByTime(1); await flush(); assert.equal(await second, 'Timeout');
  assert.deepEqual(next.resources.diagnostics, { activeReads: 0, encodedBytes: 0 });
});
