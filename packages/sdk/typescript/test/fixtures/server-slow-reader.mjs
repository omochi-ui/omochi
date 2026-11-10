import assert from 'node:assert/strict';
import { access, mkdtemp, rm } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { WebSocket } from 'ws';
import {
  LocalServerTransport,
  WebSocketServerTransport,
} from '@omochi-ui/sdk/node';
import { ResourceRegistry } from '@omochi-ui/sdk';

const kind = process.argv[2];
const root = await mkdtemp(path.join(tmpdir(), 'omochi-slow-'));
const makeTransport = () =>
  kind === 'unix'
    ? new LocalServerTransport({ serviceId: 'demo', runtimeDir: root })
    : new WebSocketServerTransport({ port: 0 });
const transport = makeTransport();
const peers = [];
const resources = new ResourceRegistry();
transport.resources = resources;
const resource = resources
  .bytes(Buffer.from('resource data'), 'image/png')
  .split('/')
  .at(-1);
const create = (surfaceId) => ({
  version: 'v0.9',
  createSurface: {
    surfaceId,
    catalogId:
      'https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json',
  },
});
const update = (surfaceId, value) => ({
  version: 'v0.9',
  updateDataModel: { surfaceId, path: '/', value },
});
async function connect(paused = false) {
  const pending = [],
    waiters = [];
  const receive = (text) => {
    const value = JSON.parse(String(text));
    const waiter = waiters.shift();
    if (waiter) waiter(value);
    else pending.push(value);
  };
  const address = kind === 'unix' ? undefined : new URL(transport.url);
  const socket =
    kind === 'unix'
      ? createConnection(transport.endpoint.path)
      : paused
        ? createConnection({
            host: address.hostname,
            port: Number(address.port),
          })
        : new WebSocket(transport.url);
  peers.push(socket);
  socket.on('error', () => {});
  const ready = new Promise((resolve) => {
    if (kind !== 'unix' && paused) {
      socket.once('connect', () =>
        socket.write(
          `GET ${address.pathname} HTTP/1.1\r\nHost: ${address.host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n`,
        ),
      );
      socket.once('data', (data) => {
        assert.match(data.toString(), /^HTTP\/1.1 101/);
        socket.pause();
        resolve();
      });
    } else socket.once(kind === 'unix' ? 'connect' : 'open', resolve);
  });
  if (kind === 'unix') {
    if (paused) socket.pause();
    else
      createInterface({ input: socket, crlfDelay: Infinity }).on(
        'line',
        receive,
      );
  } else {
    socket.on('message', receive);
  }
  await ready;
  return {
    socket,
    next: () =>
      pending.length
        ? Promise.resolve(pending.shift())
        : new Promise((resolve) => waiters.push(resolve)),
    send: (value) =>
      kind === 'unix'
        ? socket.write(JSON.stringify(value) + '\n')
        : socket.send(JSON.stringify(value)),
  };
}
const destroy = (socket) =>
  typeof socket.destroy === 'function' ? socket.destroy() : socket.terminate();
try {
  await transport.ready;
  await transport.send(create('main'));
  const slow = await connect(true);
  const healthy = await connect();
  assert.equal((await healthy.next()).kind, 'hello');
  assert.deepEqual(await healthy.next(), create('main'));
  assert.equal(transport.clients.size, 2);
  const slowServer = [...transport.clients][0];
  const closed = new Promise((resolve) => slowServer.once('close', resolve));
  const value = '🌸'.repeat(16384); // 64KiB UTF-8, avoiding code-unit accounting.
  for (let index = 0; index < 640; index++) {
    const message = update('main', { index, text: value });
    await transport.send(message);
    assert.deepEqual(await healthy.next(), message);
    const writer = transport.outbound.get(slowServer);
    assert.ok(writer.bytes <= 16 * 1024 * 1024);
  }
  await closed;
  assert.equal(transport.clients.size, 1);
  assert.equal(transport.outbound.get(slowServer).bytes, 0);
  destroy(slow.socket);

  healthy.send({
    omochi: 'resources/1',
    kind: 'request',
    id: 'healthy-resource',
    op: 'stat',
    resource,
  });
  assert.equal((await healthy.next()).id, 'healthy-resource');
  destroy(healthy.socket);

  // A healthy reconnect streams a total snapshot larger than the byte budget.
  // Updates and resource responses accepted during replay follow its final frame.
  for (let index = 0; index < 18; index++) {
    await transport.send(create(`replay-${index}`));
    await transport.send(update(`replay-${index}`, 'x'.repeat(1024 * 1024)));
  }
  const late = await connect();
  assert.equal((await late.next()).kind, 'hello');
  const live = update('main', { afterReplay: true });
  await transport.send(live);
  late.send({
    omochi: 'resources/1',
    kind: 'request',
    id: 'replay-resource',
    op: 'stat',
    resource,
  });
  assert.deepEqual(await late.next(), create('main'));
  assert.equal((await late.next()).updateDataModel.value.index, 639);
  for (let index = 0; index < 18; index++) {
    assert.deepEqual(await late.next(), create(`replay-${index}`));
    const message = await late.next();
    assert.equal(message.updateDataModel.surfaceId, `replay-${index}`);
    assert.equal(message.updateDataModel.value.length, 1024 * 1024);
  }
  assert.deepEqual(await late.next(), live);
  assert.equal((await late.next()).id, 'replay-resource');
  destroy(late.socket);
  await transport.close();
  if (kind === 'unix') await assert.rejects(access(path.join(root, 'demo')));
  const successor = makeTransport();
  await successor.ready;
  await successor.close();
} finally {
  for (const socket of peers) destroy(socket);
  await transport.close();
  resources.close();
  await rm(root, { recursive: true, force: true });
}
console.log('slow reader isolated');
