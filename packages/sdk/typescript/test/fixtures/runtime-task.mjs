// Exercise the built public API in a fresh Node process without rejection handlers.
import assert from 'node:assert/strict';
import { setImmediate } from 'node:timers/promises';
import { App } from '@omochi-ui/sdk';

const runtime = new App({}).build();
let signal;
let cleaned = false;
let closed = false;
const closing = runtime.waitClosed().then(() => {
  closed = true;
});

if (process.argv[2] === 'abort') {
  let releaseCleanup;
  const cleanup = new Promise((resolve) => {
    releaseCleanup = resolve;
  });
  runtime.spawn(async (taskSignal) => {
    signal = taskSignal;
    try {
      await new Promise((_, reject) => {
        taskSignal.addEventListener('abort', () => reject(taskSignal.reason), {
          once: true,
        });
      });
    } finally {
      await cleanup;
      cleaned = true;
    }
  });

  await runtime.shutdown();
  assert.equal(signal.aborted, true);
  await setImmediate();
  assert.equal(closed, false);
  assert.equal(cleaned, false);
  releaseCleanup();
} else {
  const fail = process.argv[2] === 'failure';
  runtime.spawn(async (taskSignal) => {
    signal = taskSignal;
    try {
      if (fail) throw new Error('background task failed');
    } finally {
      cleaned = true;
    }
  });
  // Both the callback and already-created Promise overload must own rejection.
  runtime.spawn(
    fail ? Promise.reject(new Error('promise task failed')) : Promise.resolve(),
  );
  await setImmediate();
  await runtime.shutdown();
  assert.equal(
    signal.aborted,
    false,
    'settled tasks must no longer be aborted during shutdown',
  );
}

await closing;
assert.equal(cleaned, true);
assert.equal(closed, true);
assert.throws(
  () => runtime.resources.bytes(new Uint8Array(), 'image/png'),
  /resource registry closed/,
);
console.log('closed');
