import assert from 'node:assert/strict';
import { test } from '@omochi-ui/core/testing';
import { MessageProcessor, ComponentContext, DataContext, GenericBinder, ButtonApi, Catalog, createFunctionImplementation, signal, type A2uiClientAction } from '@a2ui/web_core/v0_9';
import { z } from 'zod';
import { resourceCatalog } from '../src/resources.js';
import { assertResolvedInput, resolveInput } from '../src/expressions.js';

const merge = (...objects: unknown[]) => ({ call: 'omochiMergeObjects', args: { objects }, returnType: 'object' });
function setup(input: unknown, catalog = resourceCatalog) {
  const actions: A2uiClientAction[] = [];
  const errors: string[] = [];
  const processor = new MessageProcessor([catalog], action => {
    try { assertResolvedInput(action.context); actions.push(action); }
    catch (error) { errors.push(String(error)); }
  });
  processor.processMessages([
    { version: 'v0.9', createSurface: { surfaceId: 'main', catalogId: catalog.id } },
    { version: 'v0.9', updateComponents: { surfaceId: 'main', components: [
      { id: 'button', component: 'Button', child: 'label', action: { event: { name: 'run', context: { input } } } },
      { id: 'label', component: 'Text', text: 'Run' },
    ] } },
  ]);
  const surface = processor.model.surfacesMap.get('main')!;
  surface.onError.subscribe(error => { errors.push(String(error.message)); });
  const binder = new GenericBinder<{ action: () => void }>(new ComponentContext(surface, 'button'), ButtonApi.schema);
  return { actions, errors, surface, context: new DataContext(surface, '/'), binder, click: () => binder.snapshot.action() };
}

test('shared merge fixture resolves through the actual action binder at click time', async t => {
  const fixture = await Bun.file(new URL('../../../spec/fixtures/action-input.json', import.meta.url)).json();
  for (const example of [fixture.merge, fixture.taggedNewtypeMerge]) {
    const s = setup(example.expression);
    t.after(() => { s.binder.dispose(); s.surface.dispose(); });
    s.surface.dataModel.set('/', example.model);
    s.click();
    await Promise.resolve();
    assert.deepEqual(s.actions[0]?.context.input, example.resolved);
    s.surface.dataModel.set('/$local/nested', { count: 9, dynamic: 'new key' });
    s.click();
    await Promise.resolve();
    assert.deepEqual(s.actions[1]?.context.input, { ...example.resolved, count: 9, dynamic: 'new key' });
    assert.deepEqual(s.errors, []);
  }
});

test('merge errors prevent sending and allow a subsequent successful click', async t => {
  const s = setup(merge({ count: 1 }, { path: '/value' }));
  t.after(() => { s.binder.dispose(); s.surface.dispose(); });
  for (const value of [null, [], 1, 'bad', { count: 2 }]) {
    s.surface.dataModel.set('/value', value);
    s.click();
    await Promise.resolve();
    assert.equal(s.actions.length, 0);
  }
  s.surface.dataModel.set('/value', { other: 2 });
  s.click();
  await Promise.resolve();
  assert.deepEqual(s.actions[0]?.context.input, { count: 1, other: 2 });
  assert.ok(s.errors.some(message => message.includes('duplicate input key')));
  assert.throws(() => resolveInput(merge({ path: '/missing' }), s.context), /missing input reference/);
});

test('nested merges preserve data and do not reinterpret resolved objects', t => {
  const s = setup(merge());
  t.after(() => { s.binder.dispose(); s.surface.dispose(); });
  s.surface.dataModel.set('/value', JSON.parse('{"path":"/do-not-resolve","__proto__":{"polluted":true}}'));
  const result = resolveInput(merge({ title: 'x' }, merge({}, { path: '/value' })), s.context);
  assert.deepEqual(result, JSON.parse('{"title":"x","path":"/do-not-resolve","__proto__":{"polluted":true}}'));
  assert.equal(Object.getPrototypeOf(result), Object.prototype);
  assert.equal(({} as Record<string, unknown>).polluted, undefined);
  assert.deepEqual(resolveInput(merge(), s.context), {});
});

test('ordinary function array arguments resolve at click time and missing references prevent sending', async t => {
  const fixture = await Bun.file(new URL('../../../spec/fixtures/action-input.json', import.meta.url)).json();
  const example = fixture.functionMerge;
  const s = setup(example.expression);
  t.after(() => { s.binder.dispose(); s.surface.dispose(); });
  s.surface.dataModel.set('/', example.model);
  s.click();
  await Promise.resolve();
  assert.deepEqual(s.actions[0]?.context.input, example.resolved);
  s.surface.dataModel.set('/', {});
  s.click();
  await Promise.resolve();
  assert.equal(s.actions.length, 1);
  assert.ok(s.errors.some(message => message.includes("Validation failed for function 'and'")));
  s.surface.dataModel.set('/flag', true);
  s.click();
  await Promise.resolve();
  assert.deepEqual(s.actions[1]?.context.input, { allowed: true });
});

test('ordinary functions resolve nested arguments once and preserve model data and signal results', async t => {
  const echoFunction = createFunctionImplementation({
    name: 'testEcho', returnType: 'any', schema: z.object({ value: z.unknown() }),
  }, ({ value }) => signal(value));
  const catalog = new Catalog(resourceCatalog.id, [...resourceCatalog.components.values()],
    [...resourceCatalog.functions.values(), echoFunction], resourceCatalog.themeSchema);
  const echo = (value: unknown) => ({ call: 'testEcho', args: { value }, returnType: 'any' });
  const s = setup(merge({
    nested: echo({ items: [{ path: '/count' }, { call: 'not', args: { value: false } }] }),
    pathData: echo(echo({ path: '/pathData' })),
    callData: echo({ path: '/callData' }),
    truthyData: { call: 'not', args: { value: { path: '/pathData' } } },
  }), catalog);
  t.after(() => { s.binder.dispose(); s.surface.dispose(); });
  const pathData = { path: '/missing' };
  const callData = { call: 'not', args: { value: true } };
  s.surface.dataModel.set('/', { count: 3, pathData, callData });
  s.click();
  await Promise.resolve();
  assert.deepEqual(s.actions[0]?.context.input, { nested: { items: [3, true] }, pathData, callData, truthyData: false });
  assert.deepEqual(s.errors, []);
});

test('ordinary function failures and excessive argument nesting prevent sending', async t => {
  const s = setup(merge({ allowed: { call: 'and', args: { values: { path: '/values' } } } }));
  t.after(() => { s.binder.dispose(); s.surface.dispose(); });
  s.surface.dataModel.set('/values', false);
  s.click();
  await Promise.resolve();
  assert.equal(s.actions.length, 0);
  assert.ok(s.errors.some(message => message.includes('and')));
  s.surface.dataModel.set('/values', [true, true]);
  s.click();
  await Promise.resolve();
  assert.deepEqual(s.actions[0]?.context.input, { allowed: true });
  let nested: unknown = false;
  for (let i = 0; i < 101; i++) nested = { call: 'not', args: { value: nested } };
  assert.throws(() => resolveInput(nested, s.context), /maximum expression depth/);
});
