import assert from 'node:assert/strict';
import { once } from 'node:events';
import { readFileSync } from 'node:fs';
import { mkdtemp, rm } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { ResourceRegistry } from '@omochi-ui/sdk';
import { LocalServerTransport } from '@omochi-ui/sdk/node';

const actions = JSON.parse(
  readFileSync(
    new URL(
      '../../../../../spec/fixtures/client-actions.json',
      import.meta.url,
    ),
    'utf8',
  ),
);
const errors = JSON.parse(
  readFileSync(
    new URL('../../../../../spec/fixtures/client-errors.json', import.meta.url),
    'utf8',
  ),
);
const root = await mkdtemp(path.join(tmpdir(), 'omochi-frames-'));
const transport = new LocalServerTransport({
  serviceId: 'demo',
  runtimeDir: root,
});
const resources = new ResourceRegistry();
transport.resources = resources;
const uri = resources.bytes(new Uint8Array([1, 2, 3]), 'image/png');
const clients = [];
async function connect() {
  const socket = createConnection(transport.endpoint.path);
  clients.push(socket);
  const lines = createInterface({ input: socket, crlfDelay: Infinity });
  const messages = lines[Symbol.asyncIterator]();
  assert.equal(JSON.parse((await messages.next()).value).kind, 'hello');
  return { socket, messages };
}
const send = (socket, message) => socket.write(`${JSON.stringify(message)}\n`);
try {
  await transport.ready;
  const good = await connect();
  const received = transport[Symbol.asyncIterator]();
  const verify = async (action) => {
    send(good.socket, action);
    const expected = { ...action };
    delete expected.type; // Legacy Runtime commands are not part of an A2UI envelope.
    assert.deepEqual((await received.next()).value, expected);
    send(good.socket, {
      omochi: 'resources/1',
      kind: 'request',
      id: 'read',
      op: 'stat',
      resource: uri.split('/').at(-1),
    });
    assert.equal(JSON.parse((await good.messages.next()).value).status, 200);
  };
  for (const message of [...actions.invalid, ...errors.invalid]) {
    const bad = await connect();
    const closed = once(bad.socket, 'close');
    const tail = {
      ...actions.actions[0],
      action: { ...actions.actions[0].action, name: 'disconnected-peer' },
    };
    bad.socket.write(`${JSON.stringify(message)}\n${JSON.stringify(tail)}\n`);
    await closed;
    await verify(actions.actions[0]);
  }
  for (const report of errors.reports) {
    send(good.socket, report);
    await verify(actions.actions[0]);
  }
  for (const action of actions.actions) await verify(action);
} finally {
  for (const socket of clients) socket.destroy();
  await transport.close();
  resources.close();
  await rm(root, { recursive: true, force: true });
}
console.log('client frames isolated');
