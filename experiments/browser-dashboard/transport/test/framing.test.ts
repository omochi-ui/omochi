import assert from 'node:assert/strict';
import { test } from '@omochi-ui/core/testing';
import { createTransportFacade } from '../src/index.js';
import { fixture, signal } from './fixtures.js';

test('pure identity functions need no endpoint/relay and reject malformed seeds with fixed errors', async () => {
  const f = fixture();
  const transport = createTransportFacade(f.module);
  assert.equal(
    (await transport.identityFromSecret('0'.repeat(64))).secretHex,
    '0'.repeat(64),
  );
  await transport.generateIdentity();
  for (const bad of ['', 'A'.repeat(64), '0'.repeat(63), 'g'.repeat(64)]) {
    await assert.rejects(transport.identityFromSecret(bad), {
      code: 'StorageCorrupt',
    });
  }
  assert.equal(f.endpointCalls, 0);
});

test('normalized frame DTOs retain exact wire-byte counts and generation strings until Rust encoding', async () => {
  const f = fixture();
  const endpoint = await createTransportFacade(f.module).openEndpoint(
    '0'.repeat(64),
    [],
  );
  const connection = await endpoint.connect('{}', 'omochi/gateway/4', signal());
  const stream = await connection.open(
    { kind: 'service', service: 'app', generation: '18446744073709551615' },
    signal(),
  );
  assert.equal(
    JSON.parse(f.endpoint.connections[0]!.headers[0]!).generation,
    '18446744073709551615',
  );
  const raw = f.endpoint.connections[0]!.streams[0]!;
  const frame = stream.receive(signal());
  raw.read!.resolve(
    JSON.stringify({ json: '{"kind":"snapshotStart"}', wireBytes: 24 }),
  );
  assert.deepEqual(await frame, {
    json: '{"kind":"snapshotStart"}',
    wireBytes: 24,
  });
  await stream.finish();
  assert.equal(raw.finishes, 1);
  await assert.rejects(stream.send('{}', signal()), { code: 'Cancelled' });
  const eof = stream.receive(signal());
  raw.read!.resolve(null);
  assert.equal(await eof, null);
  await endpoint.close();
  assert.equal(raw.frees, 1);
});

test('invalid bridge output and partial-frame timeout discard the whole stream', async () => {
  for (const input of [
    'bad JSON',
    JSON.stringify({ json: '{}', wireBytes: 0 }),
    JSON.stringify({ json: '{}', wireBytes: 4194305 }),
    JSON.stringify({ json: '{}', wireBytes: 1, extra: true }),
  ]) {
    const f = fixture();
    const endpoint = await createTransportFacade(f.module).openEndpoint(
      '0'.repeat(64),
      [],
    );
    const connection = await endpoint.connect(
      '{}',
      'omochi/gateway/4',
      signal(),
    );
    const stream = await connection.open(
      { kind: 'control', protocol: 4 },
      signal(),
    );
    const raw = f.endpoint.connections[0]!.streams[0]!;
    const received = stream.receive(signal());
    raw.read!.resolve(input);
    await assert.rejects(received, { code: 'ProtocolError' });
    await assert.rejects(stream.receive(signal()), { code: 'Cancelled' });
    await endpoint.close();
    assert.equal(raw.cancels, 1);
    assert.equal(raw.frees, 1);
  }
  const f = fixture();
  const endpoint = await createTransportFacade(f.module).openEndpoint(
    '0'.repeat(64),
    [],
  );
  const connection = await endpoint.connect('{}', 'omochi/gateway/4', signal());
  const stream = await connection.open(
    { kind: 'control', protocol: 4 },
    signal(),
  );
  const raw = f.endpoint.connections[0]!.streams[0]!;
  const received = stream.receive(signal());
  raw.read!.reject('Timeout');
  await assert.rejects(received, { code: 'Timeout' });
  await endpoint.close();
});

test('application close identity is preserved while raw remote reason text is never exposed', async () => {
  const f = fixture();
  const endpoint = await createTransportFacade(f.module).openEndpoint(
    '0'.repeat(64),
    [],
  );
  const connection = await endpoint.connect('{}', 'omochi/gateway/4', signal());
  f.endpoint.connections[0]!.end.resolve(
    JSON.stringify({ code: '1', reason: 'untrusted secret from remote' }),
  );
  assert.deepEqual(await connection.closed(), {
    code: '1',
    reason: 'Not authorized',
  });
  await endpoint.close();
  assert.deepEqual(await connection.closed(), {
    code: '1',
    reason: 'Not authorized',
  });
});
