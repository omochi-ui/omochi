import assert from 'node:assert/strict';
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { test } from '@omochi-ui/core/testing';
import { BoardFault, type BoardRepository } from '../src/board-repository.js';
import { useDashboards } from '../src/useDashboards.js';

test('background board invalidation preserves mounted content and save-conflict recovery', async t => {
  const actEnvironment = Object.getOwnPropertyDescriptor(globalThis, 'IS_REACT_ACT_ENVIRONMENT');
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { configurable: true, value: true });
  t.after(() => {
    if (actEnvironment) Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', actEnvironment);
    else Reflect.deleteProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT');
  });
  let changed = () => {};
  let completeRead: ((boards: []) => void) | undefined;
  const repository: BoardRepository = {
    list: async () => [], presets: async () => [],
    get: async () => { throw new Error('unused'); },
    create: async () => { throw new Error('unused'); },
    update: async () => { throw new Error('unused'); }, delete: async () => {},
    subscribe: callback => { changed = callback; return () => {}; },
  };
  let state!: ReturnType<typeof useDashboards>;
  function Workspace() {
    state = useDashboards(repository);
    return createElement('div', null, state.loading ? 'Loading' : createElement('input', { defaultValue: 'app state' }));
  }
  const container = document.createElement('div');
  document.body.append(container);
  t.after(() => container.remove());
  const root = createRoot(container);
  t.after(() => act(() => root.unmount()));
  await act(async () => { root.render(createElement(Workspace)); });
  const input = container.querySelector('input')!;
  input.value = 'unsent input'; input.focus();
  const conflict = new BoardFault('Conflict');
  await act(async () => { state.setError(conflict); });
  repository.list = () => new Promise(resolve => { completeRead = resolve; });
  await act(async () => { changed(); });
  assert.equal(state.loading, false);
  assert.equal(container.querySelector('input'), input);
  await act(async () => { completeRead!([]); });
  assert.equal(state.error, conflict);
  assert.equal(input.value, 'unsent input');
  assert.equal(document.activeElement, input);
  repository.list = async () => [];
  await act(async () => { await state.reload(); });
  assert.equal(state.error, null, 'explicit recovery clears the previous conflict');
});
