import assert from 'node:assert/strict';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { WebSocket } from 'ws';
import { test } from '@omochi-ui/core/testing';
import { App, schema, type JsonObject } from '../src/index.js';
import { CallbackTransport } from '../src/transport.js';
import { UnixSocketServerTransport, WebSocketServerTransport } from '../src/node/index.js';
import { MAX_PENDING_BYTES, MAX_PENDING_MESSAGES, ReceiveQueue, ReceiveQueueOverflowError } from '../src/receive-queue.js';

function barrier() {
  let resolve!: () => void;
  const promise = new Promise<void>(done => { resolve = done; });
  return { promise, resolve };
}

function blockedApp() {
  const entered = barrier(), release = barrier();
  let calls = 0;
  const app = new App({});
  const action = app.action('block', schema.literal(null), async () => {
    calls++;
    entered.resolve();
    await release.promise;
  });
  app.surface('main', ui => ui.button('Block', action.call(null), { key: 'block' }));
  const uri = app.resources.bytes(Buffer.from('media'), 'video/mp4');
  const stat = { omochi: 'resources/1', kind: 'request', id: 'barrier', op: 'stat', resource: uri.split('/').at(-1)! };
  return { app, entered, release, stat, calls: () => calls };
}

function actionMessage(payload = ''): JsonObject {
  return {
    version: 'v0.9',
    action: { name: 'block', surfaceId: 'main', sourceComponentId: 'block', timestamp: '2026-08-12T00:00:00.000Z', context: {} },
    metadata: { a2uiClientDataModel: { version: 'v0.9', surfaces: { main: { inputs: { payload } } } } },
  };
}

for (const budget of ['count', 'bytes'] as const) {
  const message = actionMessage(budget === 'bytes' ? 'é'.repeat(512 * 1024) : '');
  const capacity = Math.min(MAX_PENDING_MESSAGES, Math.floor(MAX_PENDING_BYTES / Buffer.byteLength(JSON.stringify(message))));
  for (const kind of ['websocket', 'unix'] as const) {
    for (const ending of ['overflow', 'shutdown'] as const) {
      test(`${kind} handles ${ending} at pending action ${budget} capacity while serving resource RPC`, async t => {
        const root = await mkdtemp(path.join(tmpdir(), 'omochi-queue-'));
        t.after(() => rm(root, { recursive: true, force: true }));
        const transport = kind === 'websocket'
          ? new WebSocketServerTransport({ port: 0 })
          : new UnixSocketServerTransport({ serviceId: 'queue', runtimeDir: root });
        const blocked = blockedApp();
        const running = blocked.app.start(transport);
        const outcome = running.then(() => null, error => error);
        t.after(async () => { blocked.release.resolve(); await transport.close(); await outcome; });
        await transport.ready;
        const responded = barrier();
        const inspect = (text: string) => {
          const response = JSON.parse(text);
          if (response.id === 'barrier') {
            assert.equal(response.status, 200);
            assert.equal(response.size, 5);
            responded.resolve();
          }
        };
        let send: (value: unknown) => void;
        let closed: Promise<unknown>;
        if (transport instanceof WebSocketServerTransport) {
          const client = new WebSocket(transport.url!);
          t.after(() => client.terminate());
          client.on('message', data => inspect(data.toString()));
          closed = once(client, 'close');
          await once(client, 'open');
          send = value => client.send(JSON.stringify(value));
        } else {
          const client = createConnection(transport.socketPath!);
          t.after(() => client.destroy());
          createInterface({ input: client }).on('line', inspect);
          closed = once(client, 'close');
          await once(client, 'connect');
          send = value => { client.write(`${JSON.stringify(value)}\n`); };
        }
        send(actionMessage());
        await blocked.entered.promise;
        for (let i = 0; i < capacity; i++) send(message);
        send(blocked.stat);
        // A response on this connection proves every preceding action was read.
        await responded.promise;
        assert.equal(blocked.calls(), 1);
        if (ending === 'overflow') send(message);
        else await transport.close();
        await closed;
        blocked.release.resolve();
        if (ending === 'overflow') assert.ok(await outcome instanceof ReceiveQueueOverflowError);
        else assert.equal(await outcome, null);
        assert.equal(blocked.calls(), 1);
        await transport.close();
      });
    }
  }

  test(`resource pump bounds pending action ${budget} and closes its source on overflow`, async t => {
    const blocked = blockedApp(), responded = barrier(), overflow = barrier();
    const sourceClosing = barrier(), releaseCleanup = barrier(), sourceClosed = barrier();
    const transport = new CallbackTransport(async frame => {
      if (frame.id === 'barrier') {
        assert.equal(frame.status, 200);
        responded.resolve();
      }
    }, async function* () {
      try {
        yield actionMessage();
        await blocked.entered.promise;
        for (let i = 0; i < capacity; i++) yield message;
        yield blocked.stat;
        await overflow.promise;
        yield message;
        assert.fail('pump must stop consuming after overflow');
      } finally {
        sourceClosing.resolve();
        await releaseCleanup.promise;
        sourceClosed.resolve();
      }
    });
    const outcome = blocked.app.start(transport).then(() => null, error => error);
    t.after(async () => { overflow.resolve(); releaseCleanup.resolve(); blocked.release.resolve(); await outcome; });
    await responded.promise;
    assert.equal(blocked.calls(), 1);
    overflow.resolve();
    await sourceClosing.promise;
    blocked.release.resolve();
    assert.ok(await outcome instanceof ReceiveQueueOverflowError);
    assert.equal(blocked.calls(), 1);
    releaseCleanup.resolve();
    await sourceClosed.promise;
  });
}

test('dequeue and clear release pending byte and message capacity', () => {
  const queue = new ReceiveQueue<string>();
  const value = 'é'.repeat(MAX_PENDING_BYTES / 2 - 1); // JSON quotes fill the remaining two bytes.
  queue.push(value);
  assert.throws(() => queue.push(''), ReceiveQueueOverflowError);
  assert.equal(queue.shift(), value);
  queue.push(value);
  queue.clear();
  for (let i = 0; i < MAX_PENDING_MESSAGES; i++) queue.push('');
  assert.throws(() => queue.push(''), ReceiveQueueOverflowError);
  assert.equal(queue.shift(), '');
  queue.push('');
});
