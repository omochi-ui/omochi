import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from '@omochi-ui/core/testing';
import { ResourceRegistry } from '../src/resources.js';
import { App } from '../src/index.js';
import { WebSocketServerTransport } from '../src/node/index.js';
const fixture = JSON.parse(readFileSync(new URL('../../../../spec/fixtures/resources.json', import.meta.url), 'utf8'));

test('shared resource reads, immutable files, limits and release', () => {
  const root = mkdtempSync(path.join(tmpdir(), 'omochi-media-test-'));
  const registry = new ResourceRegistry(10, 20);
  try {
    const file = path.join(root, 'clip.mp4'); writeFileSync(file, fixture.content);
    const uri = registry.file(file), resource = uri.split('/').at(-1)!;
    writeFileSync(file, 'changed');
    for (const read of fixture.reads) {
      const response = registry.respond({ omochi: 'resources/1', kind: 'request', id: 'test', op: 'read', resource, offset: read.offset, length: read.length });
      assert.equal(response.status, read.status); assert.equal(response.data, read.data);
    }
    const stat = () => registry.respond({ omochi: 'resources/1', kind: 'request', id: 'test', op: 'stat', resource });
    assert.equal(stat().mime, fixture.mime); assert.equal(stat().size, 10);
    assert.throws(() => registry.bytes(Buffer.alloc(11), fixture.mime));
    registry.release(uri); assert.equal(stat().status, 404);
    assert.notEqual(registry.bytes(Buffer.from(fixture.content), fixture.mime), uri);
  } finally { registry.close(); rmSync(root, { recursive: true }); }
});

test('direct HTTP media serves bounded ranges and rejects foreign origins', async t => {
  const app = new App({});
  const uri = app.resources.bytes(Buffer.from(fixture.content), fixture.mime);
  app.surface('main', ui => ui.video(uri));
  const transport = new WebSocketServerTransport({ port: 0 });
  const running = app.start(transport);
  await transport.ready;
  t.after(async () => { await transport.close(); await running; });
  const url = transport.url!.replace('ws:', 'http:').replace('/ws', `/resources/_/${uri.split('/').at(-1)}`);
  for (const [range, expected, status] of [['bytes=2-4', '234', 206], ['bytes=-2', '89', 206], ['bytes=8-', '89', 206], ['bytes=20-', '', 416], ['bytes=0-1,4-5', fixture.content, 200]] as const) {
    const result = await fetch(url, { headers: { Range: range } });
    assert.equal(result.status, status); assert.equal(await result.text(), expected);
  }
  const head = await fetch(url, { method: 'HEAD' });
  assert.equal(head.headers.get('content-length'), '10'); assert.equal(await head.text(), '');
  const stale = await fetch(url, { headers: { Range: 'bytes=2-4', 'If-Range': '"stale"' } });
  assert.equal(stale.status, 200); assert.equal(await stale.text(), fixture.content);
  assert.equal((await fetch(url, { headers: { Origin: 'https://evil.example' } })).status, 403);
  assert.equal((await fetch(url, { headers: { 'Sec-Fetch-Site': 'cross-site' } })).status, 403);
  app.resources.release(uri); assert.equal((await fetch(url)).status, 404);
});

test('initial rendering failure closes registered resources', async () => {
  const app = new App({});
  const uri = app.resources.bytes(Buffer.from('media'), 'video/mp4');
  app.surface('main', () => { throw new Error('render failed'); });
  const transport = { capabilities: new Set<string>(), async send() {}, async *[Symbol.asyncIterator]() {} };
  await assert.rejects(app.start(transport), /render failed/);
  assert.equal(app.resources.respond({ omochi: 'resources/1', kind: 'request', id: 'test', op: 'stat', resource: uri.split('/').at(-1)! }).status, 404);
});
