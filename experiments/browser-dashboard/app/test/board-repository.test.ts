import assert from 'node:assert/strict';
import { IDBFactory, IDBObjectStore } from 'fake-indexeddb';
import { test, waitFor, type TestContext } from '@omochi-ui/core/testing';
import {
  BrowserBoardRepository,
  LazyBrowserBoardRepository,
} from '../src/browser-boards.js';
import {
  exportBoard,
  parseBoard,
} from '../../../../packages/hub-ui/src/board-codec.js';
import type { DashboardInput } from '../../../../packages/hub-ui/src/contracts.js';

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
async function browser(t: TestContext) {
  const factory = new IDBFactory();
  const open = async () => {
    const request = factory.open('omochi.browser.v1', 1);
    request.onupgradeneeded = () => {
      for (const [name, keyPath] of [
        ['boards', 'id'],
        ['identity', 'id'],
        ['gateways', 'nodeId'],
        ['preferences', 'id'],
      ])
        request.result.createObjectStore(name!, { keyPath });
    };
    const db = await new Promise<IDBDatabase>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    const repo = new BrowserBoardRepository(db);
    t.after(() => repo.close());
    return { repo, db };
  };
  const opened = await open();
  return { ...opened, open };
}

test('browser board writes are import-as-copy with UUID/revision/timestamps, detached inputs and CAS across separate connections', async (t) => {
  const f = await browser(t),
    other = await f.open();
  const source = input(),
    first = await f.repo.create(source),
    copy = await other.repo.create(parseBoard(exportBoard(first, [])).input);
  assert.notEqual(first.id, copy.id);
  assert.equal(first.revision, 1);
  assert.equal(copy.revision, 1);
  assert.equal(first.name, 'Example');
  source.panels[0]!.title = 'mutated';
  assert.equal((await f.repo.get(first.id)).panels[0]!.title, 'Title');
  assert.deepEqual(Object.keys(await f.repo.get(first.id)).sort(), [
    'id',
    'name',
    'panels',
    'revision',
  ]);
  assert.deepEqual(await f.repo.presets(), []);
  const updates = await Promise.allSettled([
    f.repo.update(first.id, 1, { ...input(), name: 'A' }),
    other.repo.update(first.id, 1, { ...input(), name: 'B' }),
  ]);
  assert.equal(
    updates.filter((update) => update.status === 'fulfilled').length,
    1,
  );
  assert.equal(
    updates.filter(
      (update) =>
        update.status === 'rejected' && update.reason.code === 'Conflict',
    ).length,
    1,
  );
  const saved = await f.repo.get(first.id);
  assert.equal(saved.revision, 2);
  await assert.rejects(other.repo.delete(first.id, 1), { code: 'Conflict' });
  assert.equal((await other.repo.get(first.id)).revision, 2);
  await other.repo.delete(first.id, 2);
  await assert.rejects(f.repo.get(first.id), { code: 'NotFound' });
  await assert.rejects(f.repo.update(first.id, 2, input()), {
    code: 'NotFound',
  });
  assert.equal((await f.repo.list()).length, 1);
});

test('board notifications contain no records, arrive only after complete and never fire for quota/abort/conflict failures', async (t) => {
  const f = await browser(t),
    other = await f.open();
  let own = 0,
    remote = 0;
  const post = BroadcastChannel.prototype.postMessage,
    messages: unknown[] = [];
  BroadcastChannel.prototype.postMessage = function (value: unknown) {
    messages.push(value);
    return post.call(this, value);
  };
  t.after(() => {
    BroadcastChannel.prototype.postMessage = post;
  });
  f.repo.subscribe(() => {
    own++;
    throw new Error('observer');
  });
  other.repo.subscribe(() => {
    remote++;
  });
  const saved = await f.repo.create(input());
  await waitFor(() => remote === 1);
  assert.equal(own, 1);
  const original = IDBObjectStore.prototype.put;
  t.after(() => {
    IDBObjectStore.prototype.put = original;
  });
  IDBObjectStore.prototype.put = function (value: unknown, key?: IDBValidKey) {
    if (this.name === 'boards')
      throw new DOMException('quota', 'QuotaExceededError');
    return key === undefined
      ? original.call(this, value)
      : original.call(this, value, key);
  };
  await assert.rejects(
    f.repo.update(saved.id, 1, { ...input(), name: 'not saved' }),
    { code: 'StorageUnavailable' },
  );
  assert.equal((await f.repo.get(saved.id)).name, 'Example');
  IDBObjectStore.prototype.put = function (value: unknown, key?: IDBValidKey) {
    const request =
      key === undefined
        ? original.call(this, value)
        : original.call(this, value, key);
    if (this.name === 'boards')
      request.addEventListener('success', () => this.transaction.abort(), {
        once: true,
      });
    return request;
  };
  await assert.rejects(f.repo.update(saved.id, 1, input()), {
    code: 'StorageUnavailable',
  });
  IDBObjectStore.prototype.put = original;
  await assert.rejects(f.repo.update(saved.id, 0, input()), {
    code: 'Conflict',
  });
  assert.equal(own, 1);
  assert.equal(remote, 1);
  assert.equal((await f.repo.get(saved.id)).revision, 1);
  assert.deepEqual(messages, [{ kind: 'changed' }]);
});

test('lazy browser storage normalizes denied opens, retries without changing input, and never opens for invalid boards', async (t) => {
  const f = await browser(t);
  let opens = 0,
    denied = true;
  const repo = new LazyBrowserBoardRepository(() => {
    opens++;
    if (denied) throw new DOMException('denied', 'SecurityError');
    return Promise.resolve(f.db);
  });
  t.after(() => repo.close());
  await assert.rejects(repo.create({ name: '', panels: [] }), {
    code: 'InvalidBoard',
  });
  assert.equal(opens, 0);
  await assert.rejects(repo.list(), { code: 'StorageUnavailable' });
  assert.equal(opens, 1);
  denied = false;
  let changed = 0;
  repo.subscribe(() => {
    changed++;
  });
  const created = await repo.create(input());
  assert.equal(created.revision, 1);
  assert.equal(changed, 1);
  assert.equal(opens, 2);
  assert.equal((await repo.get(created.id)).name, 'Example');
  assert.equal(opens, 2);
  repo.close();
  await assert.rejects(repo.list(), { code: 'StorageUnavailable' });
});

test('browser board storage validates only boards, isolates profile corruption, detects corrupt rows and exhausted revisions', async (t) => {
  const f = await browser(t),
    saved = await f.repo.create(input());
  await new Promise<void>((resolve, reject) => {
    const tx = f.db.transaction(
      ['identity', 'gateways', 'boards'],
      'readwrite',
    );
    tx.objectStore('identity').put({ id: 'default', arbitrary: 'corrupt' });
    tx.objectStore('gateways').put({ nodeId: 'invalid', secret: 'never read' });
    const get = tx.objectStore('boards').get(saved.id);
    get.onsuccess = () =>
      tx
        .objectStore('boards')
        .put({ ...get.result, revision: Number.MAX_SAFE_INTEGER });
    tx.oncomplete = () => resolve();
    tx.onabort = reject;
  });
  assert.equal((await f.repo.get(saved.id)).revision, Number.MAX_SAFE_INTEGER);
  await assert.rejects(
    f.repo.update(saved.id, Number.MAX_SAFE_INTEGER, input()),
    { code: 'Conflict' },
  );
  await new Promise<void>((resolve, reject) => {
    const tx = f.db.transaction('boards', 'readwrite'),
      get = tx.objectStore('boards').get(saved.id);
    get.onsuccess = () =>
      tx.objectStore('boards').put({ ...get.result, liveData: 'not allowed' });
    tx.oncomplete = () => resolve();
    tx.onabort = reject;
  });
  await assert.rejects(f.repo.list(), { code: 'StorageCorrupt' });
  await assert.rejects(f.repo.get(saved.id), { code: 'StorageCorrupt' });
  await assert.rejects(f.repo.delete(saved.id, Number.MAX_SAFE_INTEGER), {
    code: 'StorageCorrupt',
  });
});
