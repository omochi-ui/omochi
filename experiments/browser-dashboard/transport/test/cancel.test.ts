import assert from 'node:assert/strict';
import { test } from '@omochi-ui/core/testing';
import { createTransportFacade } from '../src/index.js';
import { deferred, fixture, signal } from './fixtures.js';

async function connected() {
  const f = fixture();
  const endpoint = await createTransportFacade(f.module).openEndpoint('0'.repeat(64), []);
  const connection = await endpoint.connect('{}', 'omochi/gateway/4', signal());
  const stream = await connection.open({ kind: 'control', protocol: 4 }, signal());
  return { ...f, endpoint, connection, stream, raw: f.endpoint.connections[0]!.streams[0]!, rawConnection: f.endpoint.connections[0]! };
}

test('same-direction overlap rejects, opposite-direction operations coexist, and abort joins both before disposal', async () => {
  const f = await connected();
  f.raw.blockWrites = true; f.raw.cancelGate = deferred<void>();
  const cancel = new AbortController();
  const read = f.stream.receive(cancel.signal);
  const write = f.stream.send('{}', signal());
  await assert.rejects(f.stream.receive(signal()), { code: 'ProtocolError' });
  await assert.rejects(f.stream.send('{}', signal()), { code: 'ProtocolError' });
  await assert.rejects(f.stream.finish(), { code: 'ProtocolError' });
  assert.equal(f.raw.active, 2);
  cancel.abort();
  let closed = false;
  const closing = f.endpoint.close().then(() => { closed = true; });
  await Promise.resolve();
  assert.equal(closed, false); assert.equal(f.raw.frees, 0);
  f.raw.cancelGate.resolve();
  await assert.rejects(read, { code: 'Cancelled' });
  await assert.rejects(write, { code: 'Cancelled' });
  await closing;
  assert.equal(f.raw.active, 0); assert.equal(f.raw.frees, 1); assert.equal(f.rawConnection.frees, 1);
  await assert.rejects(f.stream.receive(signal()), { code: 'Cancelled' });
});

test('cancel/close are idempotent and a pre-aborted read never starts Rust I/O', async () => {
  const f = await connected(); const cancel = new AbortController(); cancel.abort();
  await assert.rejects(f.stream.receive(cancel.signal), { code: 'Cancelled' });
  assert.equal(f.raw.reads, 0);
  assert.equal(f.stream.cancel(), f.stream.cancel());
  await f.stream.cancel(); assert.equal(f.raw.frees, 1);
  assert.equal(f.connection.close(), f.connection.close()); await f.connection.close();
  assert.equal(f.endpoint.close(), f.endpoint.close()); await f.endpoint.close();
});

test('closing an endpoint cancels and joins an in-flight dial without creating orphan connections', async () => {
  const f = fixture(); f.endpoint.connectGate = deferred();
  const endpoint = await createTransportFacade(f.module).openEndpoint('0'.repeat(64), []);
  const connecting = endpoint.connect('{}', 'omochi/gateway/4', signal());
  await endpoint.close();
  await assert.rejects(connecting, { code: 'Cancelled' });
  assert.equal(f.endpoint.cancelled.length, 1); assert.equal(f.endpoint.frees, 1);
});

test('closing a connection cancels pending stream open, while a fulfilled late dial is also disposed', async () => {
  const f = fixture();
  const endpoint = await createTransportFacade(f.module).openEndpoint('0'.repeat(64), []);
  const connection = await endpoint.connect('{}', 'omochi/gateway/4', signal());
  const raw = f.endpoint.connections[0]!; raw.openGate = deferred();
  const opening = connection.open({ kind: 'control', protocol: 4 }, signal());
  await connection.close();
  await assert.rejects(opening, { code: 'Cancelled' });
  assert.equal(raw.cancelled.length, 1); assert.equal(raw.frees, 1);
  const late = endpoint.connect('{}', 'omochi/gateway/4', signal());
  await endpoint.close();
  await assert.rejects(late, { code: 'Cancelled' });
  assert.equal(f.endpoint.connections[1]!.frees, 1);
});

test('an immediate signal abort reaches the operation registered before its first await', async () => {
  const f = fixture(); f.endpoint.connectGate = deferred();
  const endpoint = await createTransportFacade(f.module).openEndpoint('0'.repeat(64), []);
  const cancel = new AbortController();
  const connecting = endpoint.connect('{}', 'omochi/gateway/4', cancel.signal);
  cancel.abort();
  await assert.rejects(connecting, { code: 'Cancelled' });
  assert.equal(f.endpoint.cancelled.length, 1);
  await endpoint.close();
});
