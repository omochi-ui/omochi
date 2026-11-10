// A real subprocess makes an unhandled readline error or leaked socket fail the test.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { access, mkdtemp, rm } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { LocalServerTransport } from '@omochi-ui/sdk/node';

const root = await mkdtemp(path.join(tmpdir(), 'omochi-disconnect-'));
const transport = new LocalServerTransport({
  serviceId: 'demo',
  runtimeDir: root,
});
let client;
let lines;
let peer;
try {
  await transport.ready;
  const snapshot = [
    {
      version: 'v0.9',
      createSurface: {
        surfaceId: 'main',
        catalogId:
          'https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json',
      },
    },
    {
      version: 'v0.9',
      updateDataModel: {
        surfaceId: 'main',
        path: '/',
        value: { text: 'x'.repeat(1024 * 1024) },
      },
    },
  ];
  for (const message of snapshot) await transport.send(message);

  // MSG_PEEK waits for replay to start but leaves it unread, forcing a reset on close.
  peer = spawn(
    'python3',
    [
      '-c',
      `
import socket, sys
with socket.socket(socket.AF_UNIX) as client:
    client.settimeout(5)
    client.connect(sys.argv[1])
    assert client.recv(1, socket.MSG_PEEK)
`,
      transport.endpoint.path,
    ],
    { stdio: ['ignore', 'ignore', 'pipe'] },
  );
  let stderr = '';
  peer.stderr.setEncoding('utf8').on('data', (data) => {
    stderr += data;
  });
  const status = await new Promise((resolve, reject) => {
    peer.once('error', reject);
    peer.once('close', resolve);
  });
  assert.equal(status, 0, stderr);
  for (let attempt = 0; transport.clients.size && attempt < 100; attempt++)
    await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(
    transport.clients.size,
    0,
    'disconnected replay releases its connection',
  );

  client = createConnection(transport.endpoint.path);
  lines = createInterface({ input: client, crlfDelay: Infinity });
  const messages = lines[Symbol.asyncIterator]();
  for (const expected of snapshot) {
    assert.deepEqual(JSON.parse((await messages.next()).value), expected);
  }
  const received = transport[Symbol.asyncIterator]();
  const action = {
    version: 'v0.9',
    action: {
      name: 'increment',
      surfaceId: 'main',
      sourceComponentId: 'increment',
      timestamp: '2026-08-12T00:00:00.000Z',
      context: {},
    },
  };
  client.write(`${JSON.stringify(action)}\n`);
  assert.deepEqual((await received.next()).value, action);

  const closed = new Promise((resolve) => client.once('close', resolve));
  await transport.close();
  await closed;
  assert.equal((await received.next()).done, true);
  await assert.rejects(access(path.join(root, 'svc-demo~')), {
    code: 'ENOENT',
  });
} finally {
  peer?.kill();
  lines?.close();
  client?.destroy();
  await transport.close();
  await rm(root, { recursive: true, force: true });
}
console.log('disconnect isolated');
