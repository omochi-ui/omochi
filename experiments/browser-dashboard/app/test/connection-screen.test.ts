import assert from 'node:assert/strict';
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { test } from '@omochi-ui/core/testing';
import type { BrowserTransport } from '@omochi-ui/browser-transport';
import { ConnectionScreen } from '../src/ConnectionScreen.js';
import { LazyBrowserBoardRepository } from '../src/browser-boards.js';

test('hosted connection screen mounts with dashboard navigation and translated controls', async t => {
  const actEnvironment = Object.getOwnPropertyDescriptor(globalThis, 'IS_REACT_ACT_ENVIRONMENT');
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { configurable: true, value: true });
  t.after(() => {
    if (actEnvironment) Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', actEnvironment);
    else Reflect.deleteProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT');
  });
  const unavailable = async (): Promise<never> => { throw new Error('Storage unavailable in this test'); };
  const transport: BrowserTransport = {
    generateIdentity: unavailable, identityFromSecret: unavailable, openEndpoint: unavailable,
  };
  const boards = new LazyBrowserBoardRepository(unavailable);
  t.after(() => boards.close());
  const container = document.createElement('div');
  document.body.append(container);
  t.after(() => container.remove());
  const root = createRoot(container);
  t.after(() => act(() => root.unmount()));

  await act(async () => {
    root.render(createElement(ConnectionScreen, { transport, boards, takeInvitation: () => null, bootstrapError: null }));
  });

  assert.equal(container.querySelector('h2')?.textContent, 'Your application connections');
  assert.equal(container.querySelector('.connection-settings summary')?.textContent, 'Connections');
  assert.equal(container.querySelector('h1')?.textContent, 'Dashboards');
  assert.equal(document.documentElement.lang, 'en');
  assert.ok(container.querySelector('button'));
});
