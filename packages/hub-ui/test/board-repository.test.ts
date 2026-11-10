import assert from 'node:assert/strict';
import { test } from '@omochi-ui/core/testing';
import { HttpBoardRepository } from '../src/http-boards.js';
import type { BoardRepository } from '../src/board-repository.js';
import type { DashboardInput } from '../src/contracts.js';

const input = (): DashboardInput => ({
  name: ' Example ',
  panels: [
    {
      id: 'one',
      title: 'Title',
      x: 0,
      y: 100000,
      w: 6,
      h: 1000,
      reference: {
        nodeId: 'a'.repeat(64),
        serviceId: 's'.repeat(64),
        surfaceId: 'main/nested',
      },
    },
  ],
});
test('HTTP repository preserves server IDs/order and maps native status errors without exposing source error bodies', async (t) => {
  const oldFetch = globalThis.fetch;
  t.after(() => {
    globalThis.fetch = oldFetch;
  });
  const requests: { url: string; options?: RequestInit }[] = [];
  const rows = [
    { id: 'server-generated', revision: 3, ...input(), name: 'server order' },
  ];
  let status = 200;
  globalThis.fetch = (async (url, options) => {
    requests.push({ url: String(url), options });
    if (status !== 200)
      return Response.json(
        { error: 'raw secret must not surface' },
        { status },
      );
    return Response.json(
      options?.method === 'DELETE'
        ? { deleted: true }
        : options?.method === 'POST' || options?.method === 'PUT'
          ? rows[0]
          : String(url).endsWith('/api/presets')
            ? []
            : String(url).endsWith('/api/dashboards')
              ? rows
              : rows[0],
    );
  }) as typeof fetch;
  const repo: BoardRepository = new HttpBoardRepository(
    'https://local.example',
  );
  let changed = 0;
  t.after(
    repo.subscribe(() => {
      changed++;
    }),
  );
  assert.deepEqual(await repo.list(), rows);
  assert.deepEqual(await repo.presets(), []);
  assert.equal((await repo.get('server-generated')).id, 'server-generated');
  await assert.rejects(repo.get('wrong-response-id'), {
    code: 'StorageCorrupt',
  });
  assert.equal((await repo.create(input())).id, 'server-generated');
  await repo.update('server-generated', 3, input());
  await repo.delete('server-generated', 3);
  assert.equal(changed, 3);
  assert.equal(requests.at(-1)!.options!.body, '{"revision":3}');
  assert.equal(
    (requests.at(-1)!.options!.headers as Record<string, string>)[
      'Content-Type'
    ],
    'application/json',
  );
  for (const [code, expected] of [
    [404, 'NotFound'],
    [409, 'Conflict'],
    [400, 'InvalidBoard'],
    [413, 'InvalidBoard'],
    [503, 'StorageUnavailable'],
    [500, 'StorageUnavailable'],
  ] as const) {
    status = code;
    await assert.rejects(repo.list(), (error: unknown) => {
      assert(error instanceof Error && 'code' in error);
      assert.equal(error.code, expected);
      assert(!error.message.includes('raw secret'));
      return true;
    });
  }
  const count = requests.length;
  await assert.rejects(repo.create({ name: '', panels: [] }), {
    code: 'InvalidBoard',
  });
  assert.equal(requests.length, count);
});
