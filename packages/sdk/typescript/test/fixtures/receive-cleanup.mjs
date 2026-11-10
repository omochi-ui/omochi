// Use strict Node rejection handling to check detached receive and cleanup work.
import assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';
import { setImmediate } from 'node:timers/promises';
import { App, CallbackTransport, StdioTransport, schema } from '@omochi-ui/sdk';

const mode = process.argv[2];
const failure = new Error('handler failed');
const app = new App({});
const action = { type: 'invoke', action: 'fail', input: null };

if (mode === 'stdio') {
  const input = new PassThrough();
  Object.defineProperty(process, 'stdin', { value: input });
  app.action('fail', schema.literal(null), async () => {
    // Let the pump block in readline's next read before the handler fails.
    await setImmediate();
    throw failure;
  });
  const transport = new StdioTransport();
  transport.send = async () => {};
  const outcome = app.start(transport);
  input.write(`${JSON.stringify(action)}\n`);
  await assert.rejects(outcome, (error) => error === failure);
  assert.equal(input.destroyed, false);
  assert.equal(
    input.listenerCount('data'),
    0,
    'readline must close before stdin ends',
  );
  input.destroy();
} else {
  const reading = Promise.withResolvers();
  const pendingRead = Promise.withResolvers();
  const cleanup = Promise.withResolvers();
  let reads = 0;
  let returns = 0;
  const source = {
    [Symbol.asyncIterator]() {
      return this;
    },
    async next() {
      if (++reads === 1) return { done: false, value: action };
      reading.resolve();
      if (mode === 'exhausted') return { done: true };
      return pendingRead.promise;
    },
    return() {
      returns++;
      if (mode === 'throwing') throw new Error('cleanup threw');
      if (mode === 'delayed' || mode === 'rejected') return cleanup.promise;
      return Promise.resolve({ done: true });
    },
  };
  app.action('fail', schema.literal(null), async () => {
    await reading.promise;
    if (mode !== 'exhausted') throw failure;
  });
  const outcome = app.start(
    new CallbackTransport(
      async () => {},
      () => source,
    ),
  );
  if (mode === 'exhausted') await outcome;
  else await assert.rejects(outcome, (error) => error === failure);
  assert.equal(reads, 2);
  assert.equal(returns, mode === 'exhausted' ? 0 : 1);

  // Settle only after Runtime.start finishes: cleanup cannot gate error delivery.
  if (mode === 'delayed') {
    cleanup.resolve({ done: true });
    pendingRead.resolve({ done: false, value: action });
  } else if (mode === 'rejected') {
    cleanup.reject(new Error('cleanup rejected'));
    pendingRead.reject(new Error('read rejected after shutdown'));
  }
  // Drain rejection reporting and any resumed pump without a timed sleep.
  await setImmediate();
  assert.equal(reads, 2, 'a stopped source must not be read again');
  assert.equal(
    returns,
    mode === 'exhausted' ? 0 : 1,
    'source cleanup runs only once',
  );
}
console.log('closed');
