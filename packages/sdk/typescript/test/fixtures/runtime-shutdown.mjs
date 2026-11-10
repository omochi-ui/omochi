// Strict rejection handling and natural process exit catch detached reads/listeners.
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { access, mkdtemp, rm } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { PassThrough } from 'node:stream';
import { setImmediate } from 'node:timers/promises';
import { WebSocket } from 'ws';
import { App, CallbackTransport, StdioTransport, schema } from '@omochi-ui/sdk';
import {
  UnixSocketServerTransport,
  WebSocketServerTransport,
} from '@omochi-ui/sdk/node';

const mode = process.argv[2];
const app = new App({});
let actions = 0;
app.action('increment', schema.literal(null), () => {
  actions++;
});
const action = { type: 'invoke', action: 'increment', input: null };
let runtime;
if (mode === 'action')
  app.action('stop', schema.literal(null), async () => {
    await runtime.shutdown();
  });
runtime = app.build();
runtime.resources.bytes(new Uint8Array([1]), 'image/png');

if (mode === 'custom' || mode === 'late-rejection' || mode === 'callback') {
  const reading = Promise.withResolvers();
  const pending = Promise.withResolvers();
  let returned = 0;
  let reads = 0;
  let closed = false;
  const source = {
    [Symbol.asyncIterator]() {
      return this;
    },
    next() {
      reads++;
      reading.resolve();
      return pending.promise;
    },
    return() {
      returned++;
      return Promise.reject(new Error('cleanup rejected'));
    },
  };
  const sent = [];
  const transport =
    mode === 'callback'
      ? new CallbackTransport(
          async (message) => {
            sent.push(message);
          },
          () => source,
        )
      : {
          capabilities: new Set(),
          send: async () => {},
          close() {
            closed = true;
          },
          [Symbol.asyncIterator]() {
            return source;
          },
        };
  const started = runtime.start(transport);
  await reading.promise;
  await runtime.shutdown();
  await runtime.waitClosed();
  await started;
  assert.equal(returned, 1);
  if (mode === 'late-rejection') pending.reject(new Error('late read failure'));
  else
    pending.resolve({
      done: false,
      value:
        mode === 'callback'
          ? {
              omochi: 'resources/1',
              kind: 'request',
              id: 'late',
              op: 'stat',
              resource: '0'.repeat(32),
            }
          : action,
    });
  await setImmediate();
  assert.equal(actions, 0);
  assert.equal(reads, 1);
  assert.equal(closed, false, 'caller-owned close is not a runtime hook');
  assert.equal(sent.length, mode === 'callback' ? 1 : 0);
} else if (mode === 'action') {
  await runtime.start(
    new CallbackTransport(
      async () => {},
      async function* () {
        yield { type: 'invoke', action: 'stop', input: null };
        yield action;
      },
    ),
  );
  assert.equal(actions, 0, 'queued actions are discarded on shutdown');
} else if (mode === 'stdio') {
  const input = new PassThrough();
  Object.defineProperty(process, 'stdin', { value: input });
  const transport = new StdioTransport();
  transport.send = async () => {};
  const started = runtime.start(transport);
  await runtime.ready();
  await setImmediate();
  assert.ok(input.listenerCount('readable') > 0);
  await runtime.shutdown();
  await started;
  assert.equal(input.listenerCount('data'), 0);
  assert.equal(input.listenerCount('readable'), 0);
  assert.equal(input.destroyed, false);
  input.destroy();
} else if (mode === 'websocket' || mode === 'unix') {
  const root =
    mode === 'unix'
      ? await mkdtemp(path.join(tmpdir(), 'omochi-shutdown-'))
      : null;
  const transport = root
    ? new UnixSocketServerTransport({ serviceId: 'app', runtimeDir: root })
    : new WebSocketServerTransport({ port: 0 });
  try {
    const started = runtime.start(transport);
    await runtime.ready();
    await transport.ready;
    const socketPath = transport.socketPath;
    const client = root
      ? createConnection(socketPath)
      : new WebSocket(transport.url);
    if (root) client.resume();
    await once(client, root ? 'connect' : 'open');
    const disconnected = once(client, 'close');
    await runtime.shutdown();
    await runtime.waitClosed();
    await started;
    await disconnected;
    if (root)
      await assert.rejects(access(path.join(root, 'app')), { code: 'ENOENT' });
    const url = root ? null : new URL(transport.url);
    const probe = root
      ? createConnection(socketPath)
      : createConnection({ host: url.hostname, port: Number(url.port) });
    try {
      await assert.rejects(once(probe, 'connect'), {
        code: root ? 'ENOENT' : 'ECONNREFUSED',
      });
    } finally {
      probe.destroy();
    }
  } finally {
    await transport.close();
    if (root) await rm(root, { recursive: true, force: true });
  }
} else if (
  [
    'task-callback',
    'task-callback-immediate',
    'task-promise',
    'task-multiple',
  ].includes(mode)
) {
  const trigger = Promise.withResolvers();
  const requested = Promise.withResolvers();
  const taskCleanup = Promise.withResolvers();
  const tasksFinished = Promise.withResolvers();
  const backgroundCleanup = Promise.withResolvers();
  const initiators = mode === 'task-multiple' ? 2 : 1;
  let requests = 0;
  let finished = 0;
  let backgroundFinished = false;
  let stoppedReceiving = 0;
  let startFinished = false;
  let closed = false;
  const closing = runtime.waitClosed().then(() => {
    closed = true;
  });
  const started = runtime
    .start({
      capabilities: new Set(),
      send: async () => {},
      async stopReceiving() {
        stoppedReceiving++;
      },
      // The test transport waits without producing messages.
      // oxlint-disable-next-line require-yield
      async *[Symbol.asyncIterator]() {
        await new Promise(() => {});
      },
    })
    .then(() => {
      startFinished = true;
    });
  await runtime.ready();
  runtime.spawn(async (signal) => {
    await new Promise((resolve) =>
      signal.addEventListener('abort', resolve, { once: true }),
    );
    await backgroundCleanup.promise;
    backgroundFinished = true;
  });
  async function stopFromTask(signal) {
    if (mode !== 'task-callback-immediate') await trigger.promise;
    const request = runtime.shutdown();
    if (signal)
      assert.equal(
        signal.aborted,
        true,
        'shutdown aborts the initiating callback too',
      );
    await request;
    if (++requests === initiators) requested.resolve();
    await taskCleanup.promise;
    if (++finished === initiators) tasksFinished.resolve();
  }
  if (mode !== 'task-promise') runtime.spawn(stopFromTask);
  if (mode === 'task-promise' || mode === 'task-multiple')
    runtime.spawn(stopFromTask());
  trigger.resolve();
  await requested.promise;
  await setImmediate();
  assert.equal(stoppedReceiving, 1);
  assert.equal(
    closed,
    false,
    "waitClosed includes the initiating tasks' cleanup",
  );
  assert.equal(startFinished, false, 'start waits for task cleanup');
  runtime.resources.bytes(new Uint8Array([2]), 'image/png');
  taskCleanup.resolve();
  await tasksFinished.promise;
  await setImmediate();
  assert.equal(
    closed,
    false,
    'waitClosed also includes other background tasks',
  );
  assert.equal(startFinished, false);
  backgroundCleanup.resolve();
  await closing;
  await started;
  assert.equal(backgroundFinished, true);
  assert.equal(finished, initiators);
} else if (mode === 'task-throw') {
  const failure = new Error('synchronous task failure');
  let taskSignal;
  assert.throws(
    () =>
      runtime.spawn((signal) => {
        taskSignal = signal;
        throw failure;
      }),
    (error) => error === failure,
  );
  await runtime.shutdown();
  await runtime.waitClosed();
  assert.equal(
    taskSignal.aborted,
    false,
    'a failed callback must release its controller',
  );
} else if (mode === 'task-finally') {
  let cleaned = false;
  runtime.spawn(async (signal) => {
    try {
      await new Promise((resolve) =>
        signal.addEventListener('abort', resolve, { once: true }),
      );
    } finally {
      await runtime.shutdown();
      cleaned = true;
    }
  });
  await runtime.shutdown();
  await runtime.waitClosed();
  assert.equal(cleaned, true);
} else if (mode === 'cleanup-error' || mode === 'handler-error') {
  const primary = new Error('receive failed');
  const cleanup = new Error('stop failed');
  const reading = Promise.withResolvers();
  let cleaned = false;
  runtime.spawn(async (signal) => {
    await new Promise((resolve) =>
      signal.addEventListener('abort', resolve, { once: true }),
    );
    cleaned = true;
  });
  const started = runtime.start({
    capabilities: new Set(),
    send: async () => {},
    async stopReceiving() {
      throw cleanup;
    },
    // The test transport waits or throws without producing messages.
    // oxlint-disable-next-line require-yield
    async *[Symbol.asyncIterator]() {
      reading.resolve();
      if (mode === 'handler-error') throw primary;
      await new Promise(() => {});
    },
  });
  const rejected = assert.rejects(
    started,
    (error) => error === (mode === 'handler-error' ? primary : cleanup),
  );
  await reading.promise;
  if (mode === 'cleanup-error') await runtime.shutdown();
  await rejected;
  await setImmediate(); // Cleanup failure stays observed before a late waitClosed().
  await assert.rejects(runtime.waitClosed(), (error) => error === cleanup);
  assert.equal(cleaned, true);
}
assert.throws(
  () => runtime.resources.bytes(new Uint8Array(), 'image/png'),
  /resource registry closed/,
);
await runtime.shutdown();
console.log('closed');
