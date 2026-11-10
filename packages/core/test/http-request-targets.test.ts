import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from '../src/testing.js';

for (const runtime of ['bun', 'node']) {
  for (const endpoint of ['resources', 'services']) {
    test(`isolates invalid ${endpoint} request targets in ${runtime}`, () => {
      const result = spawnSync(
        runtime,
        [
          '--unhandled-rejections=strict',
          fileURLToPath(
            new URL('./fixtures/http-request-targets.mjs', import.meta.url),
          ),
          endpoint,
        ],
        { encoding: 'utf8', timeout: 10_000 },
      );
      assert.ifError(result.error);
      assert.equal(result.status, 0, result.stderr);
      assert.equal(result.stdout.trim(), 'request targets isolated');
    });
  }
}
