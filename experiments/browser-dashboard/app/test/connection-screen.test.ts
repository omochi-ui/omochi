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

  assert.equal(container.querySelector('h1')?.textContent, 'Dashboards');
  assert.equal(document.documentElement.lang, 'en');
  const trigger = container.querySelector<HTMLButtonElement>('button[aria-haspopup="dialog"]');
  assert.ok(trigger);
  assert.match(trigger.getAttribute('aria-label') ?? '', /^Gateways: Connections\./);
  assert.equal(trigger.getAttribute('aria-expanded'), 'false');
  const menu = document.getElementById(trigger.getAttribute('aria-controls') ?? '');
  assert.ok(menu instanceof HTMLDialogElement);
  assert.equal(menu.open, false);

  await act(async () => { trigger.click(); });
  assert.equal(trigger.getAttribute('aria-expanded'), 'true');
  assert.equal(menu.open, true);
  assert.equal(menu.querySelector('.dialog-header h2')?.textContent, 'Gateways');
  assert.equal(menu.querySelector('.connections-page h2')?.textContent, 'Your application connections');

  const close = menu.querySelector<HTMLButtonElement>('button[aria-label="Close"]');
  assert.ok(close);
  await act(async () => { close.click(); });
  assert.equal(menu.open, false);
  assert.equal(trigger.getAttribute('aria-expanded'), 'false');

  await act(async () => { trigger.click(); });
  await act(async () => { menu.dispatchEvent(new Event('cancel', { cancelable: true })); });
  assert.equal(menu.open, false);
  assert.equal(trigger.getAttribute('aria-expanded'), 'false');
});
