import assert from 'node:assert/strict';
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { I18nextProvider } from 'react-i18next';
import { test, type TestContext } from '@omochi-ui/core/testing';
import { PanelGrid } from '../src/PanelGrid.js';
import { createDashboardI18n } from '../src/i18n.js';
import type { Panel } from '../src/contracts.js';

const panel = (id: string, x = 0, y = 0): Panel => ({
  id,
  title: id,
  x,
  y,
  w: 6,
  h: 6,
  reference: { nodeId: 'a'.repeat(64), serviceId: 'test', surfaceId: 'main' },
});

async function fixture(
  t: TestContext,
  panels = [panel('one')],
  editing = true,
) {
  const previous = Object.getOwnPropertyDescriptor(
    globalThis,
    'IS_REACT_ACT_ENVIRONMENT',
  );
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', {
    configurable: true,
    value: true,
  });
  t.after(() => {
    if (previous)
      Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', previous);
    else Reflect.deleteProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT');
  });
  const width = Object.getOwnPropertyDescriptor(
    HTMLElement.prototype,
    'clientWidth',
  );
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', {
    configurable: true,
    get: () => 1200,
  });
  t.after(() => {
    if (width)
      Object.defineProperty(HTMLElement.prototype, 'clientWidth', width);
    else Reflect.deleteProperty(HTMLElement.prototype, 'clientWidth');
  });
  const container = document.createElement('div');
  document.body.append(container);
  t.after(() => container.remove());
  const root = createRoot(container);
  t.after(() => act(() => root.unmount()));
  const changes: Panel[][] = [];
  const i18n = createDashboardI18n({ lng: 'en', detection: { caches: [] } });
  const render = async (editing: boolean) => {
    await act(async () => {
      root.render(
        createElement(
          I18nextProvider,
          { i18n },
          createElement(PanelGrid, {
            panels,
            editing,
            busy: false,
            narrow: false,
            onChange: (next) => changes.push(next),
            children: panels.map((p) =>
              createElement(
                'article',
                { key: p.id, 'data-panel-id': p.id },
                createElement('button', { className: 'drag-handle' }, p.title),
              ),
            ),
          }),
        ),
      );
    });
  };
  await render(editing);
  const article = container.querySelector<HTMLElement>('article')!;
  Object.defineProperty(article, 'offsetParent', {
    configurable: true,
    get: () => article.parentElement,
  });
  const grip = article.querySelector('button')!;
  const mouse = async (
    type: string,
    x: number,
    target: EventTarget = document,
  ) => {
    await act(async () => {
      target.dispatchEvent(
        new MouseEvent(type, {
          bubbles: true,
          cancelable: true,
          button: 0,
          buttons: type === 'mouseup' ? 0 : 1,
          clientX: x,
          clientY: 0,
        }),
      );
    });
  };
  const touch = async (
    type: string,
    x: number,
    target: EventTarget = document,
  ) => {
    await act(async () => {
      const point = new Touch({
        identifier: 1,
        target: grip,
        clientX: x,
        clientY: 0,
      });
      target.dispatchEvent(
        new TouchEvent(type, {
          bubbles: true,
          cancelable: true,
          touches: type === 'touchend' || type === 'touchcancel' ? [] : [point],
          changedTouches: [point],
        }),
      );
    });
  };
  return { container, article, grip, changes, render, mouse, touch };
}

test("editing preserves geometry for a panel with RGL's default dropping ID", async (t) => {
  const f = await fixture(t, [panel('__dropping-elem__', 3, 4)], false);
  const { width, height } = f.article.style;
  await f.render(true);
  assert.equal(f.article.style.width, width);
  assert.equal(f.article.style.height, height);
  assert.equal(f.article.style.transform, 'translate(304px,0px)');
  assert.deepEqual(f.changes, []);
});

test('a completed grip drag commits its geometry', async (t) => {
  const f = await fixture(t);
  await f.mouse('mousedown', 0, f.grip);
  await f.mouse('mousemove', 101);
  await f.mouse('mouseup', 101);
  assert.equal(f.changes.length, 1);
  assert.equal(f.changes[0]![0]!.x, 1);
});

for (const cancellation of [
  'Escape',
  'pointercancel',
  'touchcancel',
  'blur',
  'resize',
]) {
  test(`${cancellation} cancels a grip press before the drag threshold`, async (t) => {
    const f = await fixture(t);
    const original = f.article.getAttribute('style');
    await f.mouse('mousedown', 0, f.grip);
    await f.mouse('mousemove', 2);
    await act(async () => {
      if (cancellation === 'Escape')
        window.dispatchEvent(
          new KeyboardEvent('keydown', {
            key: 'Escape',
            bubbles: true,
            cancelable: true,
          }),
        );
      else if (cancellation.endsWith('cancel'))
        f.grip.dispatchEvent(new Event(cancellation, { bubbles: true }));
      else window.dispatchEvent(new Event(cancellation));
    });
    await f.mouse('mousemove', 101);
    await f.mouse('mouseup', 101);
    assert.deepEqual(f.changes, []);
    assert.equal(f.article.getAttribute('style'), original);
    assert.equal(f.container.querySelector('.react-grid-placeholder'), null);
  });
}

test('Escape cancels a touch grip press before any movement', async (t) => {
  const f = await fixture(t);
  const original = f.article.getAttribute('style');
  await f.touch('touchstart', 0, f.grip);
  await act(async () => {
    window.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'Escape',
        bubbles: true,
        cancelable: true,
      }),
    );
  });
  await f.touch('touchmove', 101);
  await f.touch('touchend', 101);
  assert.deepEqual(f.changes, []);
  assert.equal(f.article.getAttribute('style'), original);
  assert.equal(f.container.querySelector('.react-grid-placeholder'), null);
});
