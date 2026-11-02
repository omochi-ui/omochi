import assert from 'node:assert/strict';
import { test } from '@omochi-ui/core/testing';
import { BrowserImages } from '../src/images.js';
import { A2uiWebSocketClient, type Processor } from '../src/client.js';

const headers = { 'content-type': 'image/png', 'x-omochi-image-width': '2', 'x-omochi-image-height': '3' };

test('scoped images fetch only opaque Hub references without credentials and release every lease', async t => {
  const savedFetch = globalThis.fetch, savedDecode = globalThis.createImageBitmap;
  t.after(() => { globalThis.fetch = savedFetch; globalThis.createImageBitmap = savedDecode; });
  const requests: { url: string; options?: RequestInit }[] = [];
  let closed = 0;
  globalThis.fetch = (async (url: string | URL | Request, options?: RequestInit) => {
    requests.push({ url: String(url), options });
    return new Response(new Uint8Array([1, 2, 3]), { headers });
  }) as unknown as typeof fetch;
  globalThis.createImageBitmap = async () => ({ width: 2, height: 3, close: () => { closed++; } }) as ImageBitmap;
  const sent: Record<string, unknown>[] = [];
  let images: BrowserImages;
  images = new BrowserImages('ws://localhost:5555/ws?private=not-forwarded', value => {
    const message = value as Record<string, unknown>;
    sent.push(message);
    if (message.kind === 'image') queueMicrotask(() => images.response({ id: message.id, reference: 'a'.repeat(32) }));
  });
  images.update('one', 'opaque-handle');
  const cancel = new AbortController();
  const image = await images.scope('one')!.load('https://public.example/a.png?token=secret', cancel.signal);
  assert.deepEqual(requests.map(request => request.url), [`http://localhost:5555/images/${'a'.repeat(32)}`]);
  assert.equal(requests[0]!.options!.credentials, 'omit');
  assert.equal(requests[0]!.options!.redirect, 'error');
  assert.equal(requests[0]!.options!.cache, 'no-store');
  assert.equal(sent[0]!.handle, 'opaque-handle');
  assert.equal(sent.filter(message => message.kind === 'release').length, 1);
  image.dispose(); image.dispose();
  assert.equal(closed, 1);
  const old = images.scope('one')!;
  images.update('one', 'new-handle');
  await assert.rejects(old.load('https://public.example/new.png', cancel.signal));
  assert.equal(requests.length, 1);
  images.close();
});

test('revoked scopes cancel pending references and late decode results; encoded and decoded memory stay bounded', async t => {
  const savedFetch = globalThis.fetch, savedDecode = globalThis.createImageBitmap;
  t.after(() => { globalThis.fetch = savedFetch; globalThis.createImageBitmap = savedDecode; });
  const sent: Record<string, unknown>[] = [];
  let autoReply = false;
  let images: BrowserImages;
  images = new BrowserImages('wss://hub.example/ws', value => {
    const message = value as Record<string, unknown>; sent.push(message);
    if (autoReply && message.kind === 'image') queueMicrotask(() => images.response({ id: message.id, reference: 'b'.repeat(32) }));
  });
  const cancel = new AbortController();
  images.update('one', 'first');
  const pending = images.scope('one')!.load('https://example.com/one', cancel.signal);
  images.update('one', 'second');
  await assert.rejects(pending);
  assert(sent.some(message => message.kind === 'release'));
  autoReply = true;
  globalThis.fetch = (async () => new Response(new Uint8Array([1]), { headers })) as unknown as typeof fetch;
  let decode: ((bitmap: ImageBitmap) => void) | undefined;
  globalThis.createImageBitmap = () => new Promise<ImageBitmap>(resolve => { decode = resolve; });
  const late = images.scope('one')!.load('https://example.com/two', cancel.signal);
  for (let tick = 0; !decode && tick < 100; tick++) await Bun.sleep(1);
  assert(decode);
  images.update('one', 'third');
  let closed = 0;
  decode({ width: 2, height: 3, close: () => { closed++; } } as ImageBitmap);
  await assert.rejects(late);
  assert.equal(closed, 1);
  globalThis.fetch = (async () => new Response(new Uint8Array(8 * 1024 * 1024 + 1), { headers })) as unknown as typeof fetch;
  await assert.rejects(images.scope('one')!.load('https://example.com/big', cancel.signal), /size limit/);
  globalThis.fetch = (async () => new Response(new Uint8Array([1]), { headers: { ...headers, 'x-omochi-image-width': '2048', 'x-omochi-image-height': '2048' } })) as unknown as typeof fetch;
  globalThis.createImageBitmap = async () => ({ width: 2048, height: 2048, close: () => {} }) as ImageBitmap;
  const maximum = await images.scope('one')!.load('https://example.com/max', cancel.signal);
  await assert.rejects(images.scope('one')!.load('https://example.com/extra', cancel.signal), /memory limit/);
  maximum.dispose();
  (await images.scope('one')!.load('https://example.com/after-dispose', cancel.signal)).dispose();
  images.close();
});

test('image transport wraps untrusted service fields, acknowledges application and stops repeated resyncs', t => {
  const original = globalThis.WebSocket;
  const sockets: FakeSocket[] = [];
  class FakeSocket {
    static OPEN = 1; readyState = 1;
    onopen?: () => void; onmessage?: (event: { data: string }) => void;
    onerror?: () => void; onclose?: (event: { reason: string; code: number }) => void;
    messages: Record<string, unknown>[] = [];
    constructor(readonly url: string) { sockets.push(this); }
    close() {}
    send(value: string) { this.messages.push(JSON.parse(value)); }
    push(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) }); }
  }
  globalThis.WebSocket = FakeSocket as unknown as typeof WebSocket;
  t.after(() => { globalThis.WebSocket = original; });
  let processor: Processor | null = null, images: BrowserImages | null = null;
  const errors: string[] = [];
  const client = new A2uiWebSocketClient('ws://localhost/ws?custom=1', {
    onProcessor: value => { processor = value; }, onImages: value => { images = value; },
    onError: value => { if (value) errors.push(value); }, onStatus: () => {},
  });
  t.after(() => client.close());
  client.connect();
  const socket = sockets[0]!;
  assert.equal(new URL(socket.url).searchParams.get('images'), '1');
  assert.equal(new URL(socket.url).searchParams.get('custom'), '1');
  socket.onopen!();
  socket.push({ omochi: 'images/1', kind: 'hello' });
  const create = { version: 'v0.9', createSurface: { surfaceId: 'main', catalogId: 'https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json' }, omochi: 'images/1', kind: 'image', reference: 'attacker' };
  socket.push({ omochi: 'images/1', kind: 'update', sequence: 1, handle: 'trusted', message: create });
  assert((processor as Processor | null)?.model.surfacesMap.has('main'));
  assert.equal((images as BrowserImages | null)?.scope('main')?.token, 'trusted');
  assert.deepEqual(socket.messages, [{ omochi: 'images/1', kind: 'ack', sequence: 1 }]);
  socket.push({ omochi: 'images/1', kind: 'update', sequence: 2, handle: null, message: { version: 'v0.9', deleteSurface: { surfaceId: 'main' } } });
  assert.equal((images as BrowserImages | null)?.scope('main'), undefined);
  socket.onclose!({ reason: 'session ended', code: 1009 });
  assert.equal(processor, null);
  assert.equal(images, null);
  assert.deepEqual(errors, ['Rendering capacity exceeded. Reload to retry.']);
});
