import assert from 'node:assert/strict';
import { MessageProcessor, getValue, type A2uiMessage } from '@a2ui/web_core/v0_9';
import { test } from '@omochi-ui/core/testing';
import { BASIC_CATALOG_ID, SessionFault, jsonBytes, planDataPatch, routeClientMessage, type RenderBudget,
  type LiveSession, type LiveEvent, type RenderEvent, type ApplyResult, type ClientMessage } from '@omochi-ui/core/browser';
import { resourceCatalog } from '../src/resources.js';
import { applyDirectCreate, applyDirectDataPatch } from '../src/data-patch.js';
import { createActionMessage, type Processor } from '../src/client.js';
import { createA2uiSession } from '../src/session.js';

const surfaceId = `${'a'.repeat(64)}~app/main`;
const create: A2uiMessage = { version: 'v0.9', createSurface: { surfaceId, catalogId: BASIC_CATALOG_ID, sendDataModel: true } };
const update = (path: string, value?: unknown): A2uiMessage => ({
  version: 'v0.9', updateDataModel: { surfaceId, path, ...(value === undefined ? {} : { value }) },
});
function accounting(capacity = 16 * 1024 * 1024) {
  let used = 0;
  let peak = 0;
  const budget: RenderBudget = { reserve(bytes) {
    if (used + bytes > capacity) throw new SessionFault('CapacityExceeded');
    used += bytes; peak = Math.max(peak, used);
    let live = true;
    return () => { if (live) { live = false; used -= bytes; } };
  } };
  return { budget, get used() { return used; }, get peak() { return peak; } };
}

test('installed processor gets missing direct roots and shared pointer semantics on incremental/replay paths', t => {
  const processor = new MessageProcessor([resourceCatalog]);
  t.after(() => processor.model.dispose());
  const charge = accounting();
  const fixtures: [unknown, string, unknown?][] = [
    [undefined, '/0', 'first'], [undefined, '/a/b', 1], [undefined, '/a/b'],
    [[1, 2, 3], '/1'], [1, '/a/b', null], [[1], '/4', 5],
    [{}, '//x', 1], [{}, '/a~1b/~0', 'escaped'], [{}, '/01', 1],
    [{ x: 1 }, '/', null], [null, '/x', 2], [{ x: 1 }, '/'],
  ];
  for (const [root, path, value] of fixtures) {
    if (processor.model.getSurface(surfaceId)) processor.model.deleteSurface(surfaceId);
    applyDirectCreate(processor, create, charge.budget);
    assert.equal(processor.model.getSurface(surfaceId)!.dataModel.get('/'), undefined);
    if (root !== undefined) applyDirectDataPatch(processor, update('/', root), charge.budget);
    applyDirectDataPatch(processor, update(path, value), charge.budget);
    const expected = planDataPatch(root, path, value !== undefined, value).materialize();
    assert.deepEqual(processor.model.getSurface(surfaceId)!.dataModel.get('/'), expected);
    assert.equal(charge.used, 0);
  }
});

test('partial server patches preserve bound form drafts/signals and action metadata; root replacement clears drafts', t => {
  const processor = new MessageProcessor([resourceCatalog]);
  t.after(() => processor.model.dispose());
  const charge = accounting();
  applyDirectCreate(processor, create, charge.budget);
  applyDirectDataPatch(processor, update('/', { draft: 'server', remote: 0 }), charge.budget);
  processor.processMessages([{ version: 'v0.9', updateComponents: { surfaceId, components: [
    { id: 'root', component: 'TextField', value: { path: '/draft' }, label: 'Name' },
  ] } }]);
  const surface = processor.model.getSurface(surfaceId)!;
  const signal = surface.dataModel.getSignal('/draft');
  surface.dataModel.set('/draft', 'typing unfinished');
  applyDirectDataPatch(processor, update('/remote', 1), charge.budget);
  assert.equal(getValue(signal), 'typing unfinished');
  const raw = createActionMessage({ name: 'save', surfaceId, sourceComponentId: 'root', context: {}, timestamp: '2026-11-03T00:00:00Z' }, processor.getClientDataModel());
  assert.deepEqual(routeClientMessage(raw)?.message.metadata, {
    a2uiClientDataModel: { version: 'v0.9', surfaces: { main: { draft: 'typing unfinished', remote: 1 } } },
  });
  applyDirectDataPatch(processor, update('/', { remote: 2 }), charge.budget);
  assert.equal(getValue(signal), undefined);
  assert.equal(charge.used, 0);
});

test('scratch admission is exact, occurs before processor mutation and releases on processor errors', t => {
  const processor = new MessageProcessor([resourceCatalog]);
  t.after(() => processor.model.dispose());
  const bytes = jsonBytes({ version: 'v0.9', updateDataModel: { surfaceId, path: '/' } });
  const insufficient = accounting(bytes - 1);
  assert.throws(() => applyDirectCreate(processor, create, insufficient.budget), { code: 'CapacityExceeded' });
  assert.equal(processor.model.surfacesMap.size, 0);
  const exact = accounting(bytes);
  applyDirectCreate(processor, create, exact.budget);
  assert.equal(exact.peak, bytes); assert.equal(exact.used, 0);
  assert.throws(() => applyDirectCreate(processor, create, exact.budget));
  assert.equal(exact.used, 0);
  const denied = accounting(0);
  assert.throws(() => applyDirectDataPatch(processor, update('/100000', 1), denied.budget), { code: 'CapacityExceeded' });
  assert.equal(processor.model.getSurface(surfaceId)!.dataModel.get('/'), undefined);
  const normal = accounting();
  const original = processor.processMessages;
  processor.processMessages = () => { throw new Error('injected processor failure'); };
  assert.throws(() => applyDirectDataPatch(processor, update('/', 1), normal.budget), /injected/);
  assert.equal(normal.used, 0);
  processor.processMessages = original;
});

test('native processor bypass retains its existing default root and sparse-delete behavior', t => {
  const processor: Processor = new MessageProcessor([resourceCatalog]);
  t.after(() => processor.model.dispose());
  processor.processMessages([create]);
  assert.deepEqual(processor.model.getSurface(surfaceId)!.dataModel.get('/'), {});
  processor.processMessages([update('/', [1, 2, 3]), update('/1')]);
  const root = processor.model.getSurface(surfaceId)!.dataModel.get('/');
  assert.equal(root.length, 3);
  assert.equal(root[2], 3);
});

class DirectSource implements LiveSession {
  constructor(readonly imagePolicy: LiveSession['imagePolicy'] = 'sdk-png') {}
  apply!: (event: RenderEvent) => ApplyResult;
  attached = false;
  listener!: (event: LiveEvent) => void;
  actions: ClientMessage[] = [];
  startCalls = 0;
  stopCalls = 0;
  stopGate: Promise<void> = Promise.resolve();
  stopped = false;
  subscribe(listener: (event: LiveEvent) => void) {
    this.listener = listener;
    listener({ kind: 'images', enabled: true });
    listener({ kind: 'services', services: [] });
    listener({ kind: 'status', sourceId: 'A', state: 'connecting' });
    listener({ kind: 'status', sourceId: 'B', state: 'online' });
    return () => { this.stopped = true; };
  }
  attachRenderer(apply: (event: RenderEvent) => ApplyResult) {
    if (this.attached) throw new SessionFault('InvalidState');
    this.attached = true;
    this.apply = apply;
  }
  async start() { this.startCalls++; }
  setServices() {}
  async retry() {}
  async action(_id: string, message: ClientMessage) { this.actions.push(message); }
  async image(): Promise<Response> { throw new SessionFault('UnsupportedMedia'); }
  async stop() {
    this.stopCalls++;
    await this.stopGate;
    this.apply({ kind: 'remove', service: 'A', epoch: 1, reason: 'stop' });
    this.apply({ kind: 'remove', service: 'B', epoch: 1, reason: 'stop' });
  }
}
const commitCreate = (id: string): A2uiMessage[] => [
  { version: 'v0.9', createSurface: { surfaceId: id, catalogId: BASIC_CATALOG_ID, sendDataModel: true } },
  { version: 'v0.9', updateDataModel: { surfaceId: id, path: '/' } },
];

for (const [name, root, hasData] of [
  ['create-only', undefined, false], ['removed', undefined, true], ['null', null, true],
  ['object', { saved: 1 }, true], ['array', ['saved'], true], ['primitive', 1, true],
] as const) {
  test(`direct ${name} snapshot roots survive commit, incremental updates and reconnect`, async t => {
    const source = new DirectSource();
    const session = createA2uiSession(source);
    t.after(() => session.close());
    const processor = session.getSnapshot().processor!;
    const published: unknown[] = [];
    session.subscribe(() => published.push(processor.model.getSurface(surfaceId)?.dataModel.get('/')));
    const messages = [...commitCreate(surfaceId), ...(hasData ? [update('/', root)] : [])];
    const commit = (epoch: number) => source.apply({ kind: 'commit', service: 'A', epoch, messages, leases: [] });
    assert.deepEqual(commit(1), { ok: true });
    assert.deepEqual(published, [root], 'only the completed snapshot is published');
    const initial = processor.model.getSurface(surfaceId)!;
    assert.deepEqual(initial.dataModel.get('/'), root);
    assert.deepEqual(getValue(initial.dataModel.getSignal('/')), root);
    const charge = accounting();
    assert.deepEqual(source.apply({ kind: 'update', service: 'A', epoch: 1,
      message: update('/0', 'first'), budget: charge.budget }), { ok: true });
    const expected = planDataPatch(root, '/0', true, 'first').materialize();
    assert.deepEqual(initial.dataModel.get('/'), expected);
    assert.equal(charge.used, 0);
    assert.deepEqual(commit(2), { ok: true });
    const reconnected = processor.model.getSurface(surfaceId)!;
    assert.notEqual(reconnected, initial);
    assert.deepEqual(reconnected.dataModel.get('/'), root);
    assert.deepEqual(published, [root, expected, root]);
    assert.deepEqual(source.apply({ kind: 'update', service: 'A', epoch: 2,
      message: update('/0', 'first'), budget: charge.budget }), { ok: true });
    assert.deepEqual(reconnected.dataModel.get('/'), expected);
    assert.equal(charge.used, 0);
  });
}

for (const policy of ['native', 'hub'] as const) {
  test(`${policy} snapshot commits keep the upstream root behavior`, async t => {
    const source = new DirectSource(policy);
    const session = createA2uiSession(source);
    t.after(() => session.close());
    for (const messages of [[create], commitCreate(surfaceId), [...commitCreate(surfaceId), update('/', null)]]) {
      assert.deepEqual(source.apply({ kind: 'commit', service: 'A', epoch: 1, messages, leases: [] }), { ok: true });
      assert.deepEqual(session.getSnapshot().processor!.model.getSurface(surfaceId)!.dataModel.get('/'), {});
    }
  });
}

test('authoritative renderer batches commits, preserves B, and removes the union after a partial A commit fails', async () => {
  const source = new DirectSource();
  const session = createA2uiSession(source);
  assert.equal(source.startCalls, 1);
  assert.equal(session.getSnapshot(), session.getSnapshot());
  assert.equal(session.getSnapshot().endpoint, 'browser:gateway');
  assert.equal(session.getSnapshot().status, 'Connected');
  assert(session.getSnapshot().images);
  let changes = 0;
  const unsubscribe = session.subscribe(() => changes++);
  source.apply({ kind: 'commit', service: 'B', epoch: 1, messages: commitCreate('b/main'), leases: [] });
  source.apply({ kind: 'commit', service: 'A', epoch: 1, messages: [...commitCreate('a/old'), ...commitCreate('a/another')], leases: [] });
  assert.equal(changes, 2);
  const before = session.getSnapshot();
  const failed = source.apply({ kind: 'commit', service: 'A', epoch: 2, messages: [
    ...commitCreate('a/new'),
    { version: 'v0.9', updateComponents: { surfaceId: 'missing', components: [{ id: 'root', component: 'Text', text: 'bad' }] } },
  ], leases: [] });
  assert.deepEqual(failed, { ok: false, code: 'RenderError' });
  assert.equal(session.getSnapshot(), before, 'partial processor state is never published');
  assert.equal(changes, 2);
  // Even a throwing disposal callback must not prevent cleanup of this service.
  session.getSnapshot().processor!.model.getSurface('a/new')!.dispose = () => { throw new Error('injected disposal'); };
  assert.deepEqual(source.apply({ kind: 'remove', service: 'A', epoch: 2, reason: 'failure' }), { ok: true });
  assert.deepEqual(session.getSnapshot().surfaceIds, ['b/main']);
  assert.equal(changes, 3);
  source.apply({ kind: 'remove', service: 'A', epoch: 2, reason: 'repeat' });
  assert.deepEqual(session.getSnapshot().surfaceIds, ['b/main']);
  unsubscribe();
  await session.close();
  assert.equal(session.getSnapshot().processor, null);
  assert.equal(source.stopped, true);
});

test('live-source updates normalize current drafts, return capacity errors and close through final remove callbacks', async () => {
  const source = new DirectSource();
  const session = createA2uiSession(source);
  const controller = new AbortController();
  const lease = Object.freeze({ id: 'local-lease', surfaceId, signal: controller.signal });
  const charge = accounting();
  assert.deepEqual(source.apply({ kind: 'update', service: 'A', epoch: 1, message: create, lease, budget: charge.budget }), { ok: true });
  assert.equal(session.getSnapshot().processor!.model.getSurface(surfaceId)!.dataModel.get('/'), undefined);
  source.apply({ kind: 'update', service: 'A', epoch: 1, message: update('/', { draft: 'server', remote: 1 }), budget: charge.budget });
  const model = session.getSnapshot().processor!.model.getSurface(surfaceId)!;
  model.dataModel.set('/draft', 'editing');
  source.apply({ kind: 'update', service: 'A', epoch: 1, message: update('/remote', 2), budget: charge.budget });
  assert.deepEqual(model.dataModel.get('/'), { draft: 'editing', remote: 2 });
  await model.dispatchAction({ event: { name: 'save', context: {} } }, 'root');
  assert.equal(source.actions.length, 1);
  assert.deepEqual(source.actions[0]!.metadata, { a2uiClientDataModel: { version: 'v0.9', surfaces: { [surfaceId]: { draft: 'editing', remote: 2 } } } });
  const denied = source.apply({ kind: 'update', service: 'A', epoch: 1, message: update('/100000', 1), budget: accounting(0).budget });
  assert.deepEqual(denied, { ok: false, code: 'CapacityExceeded' });
  assert.deepEqual(model.dataModel.get('/'), { draft: 'editing', remote: 2 });
  let finish!: () => void;
  source.stopGate = new Promise(resolve => { finish = resolve; });
  const closing = session.close();
  assert.equal(session.close(), closing);
  assert.equal(source.stopped, false, 'sink/subscription stay attached until stop finishes');
  await model.dispatchAction({ event: { name: 'save', context: {} } }, 'root');
  assert.equal(source.actions.length, 1, 'no new action is dispatched during close');
  controller.abort(); finish();
  await closing;
  assert.equal(source.stopCalls, 1); assert.equal(source.stopped, true);
  assert.equal(charge.used, 0);
  assert.deepEqual(session.getSnapshot().surfaceIds, []);
});

test('an unknown direct catalog component fails the authoritative apply without exposing another service', async () => {
  const source = new DirectSource();
  const session = createA2uiSession(source);
  source.apply({ kind: 'commit', service: 'B', epoch: 1, messages: commitCreate('b/main'), leases: [] });
  const result = source.apply({ kind: 'commit', service: 'A', epoch: 1, messages: [
    ...commitCreate('a/main'),
    { version: 'v0.9', updateComponents: { surfaceId: 'a/main', components: [{ id: 'root', component: 'UnregisteredComponent' }] } },
  ], leases: [] });
  assert.deepEqual(result, { ok: false, code: 'RenderError' });
  source.apply({ kind: 'remove', service: 'A', epoch: 1, reason: 'catalog error' });
  assert.deepEqual(session.getSnapshot().surfaceIds, ['b/main']);
  await session.close();
});
