import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from '@omochi-ui/core/testing';

for (const outcome of ['success', 'failure', 'abort']) {
  test(`spawn handles ${outcome} without an unhandled rejection in Node`, () => {
    const result = spawnSync(
      'node',
      [
        '--unhandled-rejections=strict',
        fileURLToPath(new URL('./fixtures/runtime-task.mjs', import.meta.url)),
        outcome,
      ],
      { encoding: 'utf8', timeout: 10_000 },
    );
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), 'closed');
  });
}
