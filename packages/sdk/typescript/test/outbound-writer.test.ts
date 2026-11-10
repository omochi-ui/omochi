import assert from 'node:assert/strict';
import { test } from '@omochi-ui/core/testing';
import { OutboundWriter } from '../src/node/outbound-writer.js';

function controlled(limit = 16) {
  const writes: string[] = [];
  const completions: Array<(error?: Error | null) => void> = [];
  let disconnected = false;
  const writer = new OutboundWriter(
    (text, complete) => {
      writes.push(text);
      completions.push(complete);
    },
    () => {
      disconnected = true;
    },
    limit,
  );
  return {
    writer,
    writes,
    disconnected: () => disconnected,
    async finish(error?: Error) {
      completions.shift()!(error);
      await Promise.resolve();
    },
  };
}

test('outbound budget counts UTF-8 bytes and the active write until completion', async () => {
  const peer = controlled(8);
  peer.writer.start([]);
  peer.writer.send('🌸'); // Four bytes remain charged while the socket is blocked.
  peer.writer.send('éé');
  assert.equal(peer.disconnected(), false);
  assert.deepEqual(peer.writes, ['🌸']);
  peer.writer.send('\n');
  assert.equal(peer.disconnected(), true);
  await peer.finish();
  assert.deepEqual(
    peer.writes,
    ['🌸'],
    'late completion cannot flush discarded data',
  );
});

test('outbound completion releases its budget and preserves replay/live/resource order', async () => {
  const peer = controlled(8);
  peer.writer.start(['123456', 'abcdef', 'ABCDEF']); // Total replay exceeds budget.
  peer.writer.send('L');
  peer.writer.send('R');
  for (let i = 0; i < 5; i++) await peer.finish();
  assert.deepEqual(peer.writes, ['123456', 'abcdef', 'ABCDEF', 'L', 'R']);
  assert.equal(peer.disconnected(), false);
  peer.writer.send('12345678'); // Entire budget available again.
  await peer.finish();
  assert.equal(peer.disconnected(), false);
});

test('outbound replay transition respects queued bytes and closes only that writer', async () => {
  const slow = controlled(8);
  const healthy = controlled(8);
  slow.writer.start(['a', '12345678']);
  slow.writer.send('b');
  healthy.writer.start([]);
  healthy.writer.send('ok');
  await slow.finish();
  await healthy.finish();
  assert.equal(slow.disconnected(), true);
  assert.equal(healthy.disconnected(), false);
  assert.deepEqual(slow.writes, ['a']);
});

test('outbound errors and shutdown discard pending writes and release replay', async () => {
  for (const error of [undefined, new Error('peer disconnected')]) {
    const peer = controlled();
    let released = false;
    peer.writer.start(
      (function* () {
        try {
          yield 'first';
          yield 'never';
        } finally {
          released = true;
        }
      })(),
    );
    peer.writer.send('queued');
    if (!error) peer.writer.close();
    await peer.finish(error);
    assert.equal(released, true);
    assert.equal(peer.disconnected(), Boolean(error));
    assert.deepEqual(peer.writes, ['first']);
  }
});
