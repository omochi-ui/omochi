import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from '@omochi-ui/core/testing';

for (const runtime of ['node', 'bun']) {
  for (const mode of [
    'custom',
    'late-rejection',
    'callback',
    'action',
    'stdio',
    'websocket',
    'unix',
    'task-finally',
    'task-callback',
    'task-callback-immediate',
    'task-promise',
    'task-multiple',
    'task-throw',
    'cleanup-error',
    'handler-error',
  ]) {
    test(`Runtime shutdown cleans up ${mode} in ${runtime}`, () => {
      const result = spawnSync(
        runtime,
        [
          '--unhandled-rejections=strict',
          fileURLToPath(
            new URL('./fixtures/runtime-shutdown.mjs', import.meta.url),
          ),
          mode,
        ],
        { encoding: 'utf8', timeout: 10_000 },
      );
      assert.ifError(result.error);
      assert.equal(result.status, 0, result.stderr);
      assert.equal(result.stdout.trim(), 'closed');
    });
  }
}
