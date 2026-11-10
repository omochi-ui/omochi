import assert from 'node:assert/strict';
import { test } from '@omochi-ui/core/testing';
import { jsonBytes, type A2uiMessage } from '@omochi-ui/core/browser';
import { A2uiSnapshot } from '../../../../packages/core/src/a2ui-snapshot.js';
import { BoundedSnapshot } from '../src/snapshot.js';
import { BytePool, RenderDeliveryPool, LIMITS } from '../src/budget.js';

const prefix = `${'a'.repeat(64)}~app/`;
const create = (surfaceId = 'main'): A2uiMessage => ({
  version: 'v0.9',
  createSurface: { surfaceId, catalogId: 'test' },
});
const update = (path: string, value?: unknown): A2uiMessage => ({
  version: 'v0.9',
  updateDataModel: {
    surfaceId: 'main',
    path,
    ...(value === undefined ? {} : { value }),
  },
});
function setup(
  capacity = LIMITS.snapshots,
  deliveryCapacity = LIMITS.delivery,
) {
  const snapshots = new BytePool(capacity);
  const delivery = new RenderDeliveryPool(deliveryCapacity);
  return {
    snapshots,
    delivery,
    state: new BoundedSnapshot({ snapshots, delivery }),
  };
}

test('bounded replay matches native snapshot, with detached renderer root initialization', () => {
  const { state, snapshots, delivery } = setup();
  const native = new A2uiSnapshot();
  const messages: A2uiMessage[] = [
    create(),
    {
      version: 'v0.9',
      updateComponents: {
        surfaceId: 'main',
        components: [
          { id: 'a', component: 'Text', text: 'old' },
          { id: 'b', component: 'Text', text: 'second' },
        ],
      },
    },
    {
      version: 'v0.9',
      updateComponents: {
        surfaceId: 'main',
        components: [
          { id: 'a', component: 'Text', text: 'replaced' },
          { id: 'c', component: 'Text', text: 'third' },
          { id: 'c', component: 'Text', text: 'last wins' },
        ],
      },
    },
    update('/0', 'first'),
    update('/3', { x: 'last' }),
    update('/0'),
    { version: 'v0.9', deleteSurface: { surfaceId: 'unknown' } },
  ];
  for (const message of messages) {
    state.apply(message);
    native.apply(message);
  }
  assert.equal(
    state.byteSize,
    native.messages().reduce((sum, message) => sum + jsonBytes(message), 0),
  );
  const payload = state.renderPayload(prefix);
  assert.equal(
    delivery.usedBytes,
    payload.messages.reduce((sum, message) => sum + jsonBytes(message), 0),
  );
  const withoutInitialization = payload.messages
    .filter((_, index) => index !== 1)
    .map((message) => {
      const copy = structuredClone(message);
      const body = Object.values(copy)[1] as { surfaceId: string };
      body.surfaceId = body.surfaceId.slice(prefix.length);
      return copy;
    });
  assert.deepEqual(
    withoutInitialization,
    JSON.parse(JSON.stringify(native.messages())),
  );
  assert.throws(() => state.renderPayload(prefix), {
    code: 'CapacityExceeded',
  });
  const last = payload.messages.at(-1)!;
  assert.ok('updateDataModel' in last);
  (last.updateDataModel.value as unknown[])[0] = 'client edit';
  payload.release();
  payload.release();
  const next = state.renderPayload(prefix);
  assert.notEqual(JSON.stringify(next.messages), JSON.stringify([last]));
  assert.deepEqual(
    (next.messages.at(-1) as typeof last).updateDataModel.value,
    [null, null, { x: 'last' }],
  );
  next.release();
  state.dispose();
  state.dispose();
  assert.equal(snapshots.usedBytes, 0);
  assert.equal(delivery.usedBytes, 0);
  assert.throws(() => state.apply(create()), { code: 'InvalidState' });
});

test('duplicate create, unknown update and delete/recreate retain ordering and missing-root semantics', () => {
  const { state, snapshots } = setup();
  state.apply(create());
  const size = state.byteSize;
  assert.throws(() => state.apply(create()), { code: 'ProtocolError' });
  assert.equal(state.byteSize, size);
  state.apply({ version: 'v0.9', deleteSurface: { surfaceId: 'main' } });
  assert.equal(state.byteSize, 0);
  assert.throws(() => state.apply(update('/x', 1)), { code: 'ProtocolError' });
  state.apply(create());
  const initial = state.renderPayload(prefix);
  assert.equal(initial.messages.length, 2);
  assert.deepEqual(initial.messages[1], {
    version: 'v0.9',
    updateDataModel: { surfaceId: prefix + 'main', path: '/' },
  });
  initial.release();
  state.apply(update('/a/b')); // Deletion still creates missing intermediate parents.
  const payload = state.renderPayload(prefix);
  assert.deepEqual(payload.messages.at(-1), {
    version: 'v0.9',
    updateDataModel: {
      surfaceId: prefix + 'main',
      path: '/',
      value: { a: {} },
    },
  });
  payload.release();
  state.dispose();
  assert.equal(snapshots.usedBytes, 0);
});

test('old+candidate reservation rejects before materialization and preserves old state/charge', () => {
  let reserveCalls = 0;
  const backing = new BytePool(LIMITS.snapshots);
  const snapshots = {
    get availableBytes() {
      return backing.availableBytes;
    },
    reserve(bytes: number) {
      reserveCalls++;
      if (reserveCalls === 2) throw new Error('injected reservation failure');
      return backing.reserve(bytes);
    },
  };
  const state = new BoundedSnapshot({
    snapshots,
    delivery: new RenderDeliveryPool(),
  });
  state.apply(create());
  const before = backing.usedBytes;
  assert.throws(
    () => state.apply(update('/100000', 'huge sparse candidate')),
    /injected/,
  );
  assert.equal(backing.usedBytes, before);
  assert.equal(state.byteSize, before);
  const payload = state.renderPayload(prefix);
  assert.equal(payload.messages.length, 2);
  payload.release();
  state.dispose();
  assert.equal(backing.usedBytes, 0);

  const small = setup(jsonBytes(create()) * 2 - 1);
  small.state.apply(create());
  assert.throws(() => small.state.apply(update('/', 1)), {
    code: 'CapacityExceeded',
  });
  assert.equal(small.snapshots.usedBytes, jsonBytes(create()));
  small.state.dispose();
  assert.equal(small.snapshots.usedBytes, 0);
});

test('profile pool exact/+1 admission, one delivery slot and synchronous scratch charges', () => {
  const pool = new BytePool(LIMITS.snapshots);
  const reservation = pool.reserve(LIMITS.snapshots);
  assert.throws(() => pool.reserve(1), { code: 'CapacityExceeded' });
  reservation.release();
  reservation.release();
  assert.equal(pool.availableBytes, LIMITS.snapshots);
  const delivery = new RenderDeliveryPool(100);
  const payload = delivery.acquire(40);
  const scratch = delivery.reserve(60);
  assert.throws(() => delivery.reserve(1), { code: 'CapacityExceeded' });
  assert.throws(() => delivery.acquire(0), { code: 'CapacityExceeded' });
  scratch();
  scratch();
  payload.release();
  assert.equal(delivery.usedBytes, 0);
});

test('many wire-sized patches reach exact 16 MiB; +1 and prefix-induced overflow roll back', () => {
  const { state, snapshots, delivery } = setup();
  state.apply(create());
  for (let i = 0; i < 15; i++)
    state.apply(update(`/p${i}`, 'x'.repeat(1024 * 1024)));
  // Derive the exact remaining room including the final key and all replay wrappers.
  state.apply(update('/last', ''));
  const remaining = LIMITS.snapshot - state.byteSize;
  state.apply(update('/last', 'x'.repeat(remaining)));
  assert.equal(state.byteSize, LIMITS.snapshot);
  assert.equal(snapshots.usedBytes, LIMITS.snapshot);
  assert.throws(() => state.apply(update('/last', 'x'.repeat(remaining + 1))), {
    code: 'CapacityExceeded',
  });
  assert.equal(state.byteSize, LIMITS.snapshot);
  assert.throws(() => state.renderPayload(prefix), {
    code: 'CapacityExceeded',
  });
  assert.equal(delivery.usedBytes, 0);
  state.dispose();
  assert.equal(snapshots.usedBytes, 0);
});

test('delivery exact/+1 counts route escaping and synthetic initialization', () => {
  const first = setup();
  first.state.apply(create('a"b'));
  const payload = first.state.renderPayload(prefix);
  const bytes = payload.messages.reduce(
    (sum, message) => sum + jsonBytes(message),
    0,
  );
  payload.release();
  first.state.dispose();
  const exact = setup(LIMITS.snapshots, bytes);
  exact.state.apply(create('a"b'));
  const admitted = exact.state.renderPayload(prefix);
  admitted.release();
  exact.state.dispose();
  const short = setup(LIMITS.snapshots, bytes - 1);
  short.state.apply(create('a"b'));
  assert.throws(() => short.state.renderPayload(prefix), {
    code: 'CapacityExceeded',
  });
  assert.equal(short.delivery.usedBytes, 0);
  short.state.dispose();
});
