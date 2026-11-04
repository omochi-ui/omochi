import assert from 'node:assert/strict';
import { test } from '@omochi-ui/core/testing';
import { appendPanel, placePanel, reorderPanels } from '../src/layout.js';
import { overlaps, type Panel } from '../src/contracts.js';
import { validateBoardInput } from '../src/board-codec.js';
const panel = (id: string, x: number, y: number): Panel => ({ id, title: id, x, y, w: 6, h: 6,
  reference: { nodeId: 'a'.repeat(64), serviceId: id, surfaceId: 'main' } });
test('moving a panel pushes collisions down without moving adjacent columns', () => {
  const a = panel('a', 0, 0), b = panel('b', 0, 6), c = panel('c', 6, 0);
  const result = placePanel([a, b, c], { ...b, y: 0 });
  assert.equal(result.find(p => p.id === 'a')!.y, 6);
  assert.equal(result.find(p => p.id === 'c')!.y, 0);
  assert.equal(a.y, 0);
  for (const p of result) for (const q of result) if (p !== q) assert.equal(overlaps(p, q), false);
});
test('resizing pushes all collisions and movement stays in the twelve-column grid', () => {
  const a = panel('a', 0, 0), b = panel('b', 6, 0), c = panel('c', 6, 6);
  const result = placePanel([a, b, c], { ...a, w: 12, h: 10 });
  assert.equal(result.find(p => p.id === 'b')!.y, 10);
  assert.equal(result.find(p => p.id === 'c')!.y, 16);
  assert.equal(placePanel([a], { ...a, x: 99 })[0]!.x, 6);
  assert.equal(placePanel([a], { ...a, x: -2, y: -1 })[0]!.y, 0);
});

test('resizing every edge anchors the opposite edge and clamps bounds', async () => {
  const { resizePanel } = await import('../src/layout.js');
  const p = { ...panel('a', 3, 4), w: 6, h: 6 };
  assert.deepEqual(resizePanel(p, 'nw', -20, -20), { ...p, x: 0, y: 0, w: 9, h: 10 });
  assert.deepEqual(resizePanel(p, 'se', 20, 2000), { ...p, w: 9, h: 1000 });
  for (const edge of ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'] as const) {
    const resized = resizePanel(p, edge, 1, 1);
    assert.ok(resized.w >= 1 && resized.h >= 1 && resized.x + resized.w <= 12);
    if (edge.includes('w')) assert.equal(resized.x + resized.w, p.x + p.w);
    if (edge.includes('n')) assert.equal(resized.y + resized.h, p.y + p.h);
  }
});
test('gesture previews computed from the initial layout are reversible', () => {
  const initial = [panel('a', 0, 0), panel('b', 0, 6), panel('c', 0, 12)];
  const moved = placePanel(initial, { ...initial[2]!, y: 0 });
  assert.equal(moved.find(p => p.id === 'a')!.y, 6);
  assert.deepEqual(placePanel(initial, initial[2]!), initial);
});
test('mobile reorder preserves sizes and packs rows without overlap', async () => {
  const { reorderPanels } = await import('../src/layout.js');
  const initial = [panel('a', 0, 0), { ...panel('b', 6, 0), h: 3 }, { ...panel('c', 0, 6), w: 12 }];
  const result = reorderPanels(initial, 'c', -1);
  validate(result);
  assert.deepEqual(result.map(p => p.id), ['a', 'c', 'b']);
  assert.deepEqual(result.map(p => [p.x, p.y]), [[0, 0], [0, 6], [0, 12]]);
  for (const p of result) {
    assert.equal(p.w, initial.find(q => q.id === p.id)!.w);
    assert.equal(p.h, initial.find(q => q.id === p.id)!.h);
    for (const q of result) if (p !== q) assert.equal(overlaps(p, q), false);
  }
  assert.equal(reorderPanels(initial, 'a', -1), initial);
});

function validate(panels: Panel[]): void {
  assert.deepEqual(validateBoardInput({ name: 'Layout', panels }).panels, panels);
}

test('mobile reorder keeps the valid draft when repacking would exceed the stored row limit', () => {
  let x = 0, y = 0;
  const initial = [7, 5, 7, 5, ...Array<number>(132).fill(12)].map((w, index) => {
    if (x + w > 12) { y += 750; x = 0; }
    const next = { ...panel(String(index), x, y), w, h: 750 };
    x += w;
    return next;
  });
  assert.equal(Math.max(...initial.map(p => p.y)), 99750);
  validate(initial);
  const result = reorderPanels(initial, '2', -1);
  validate(result);
  assert.equal(result, initial);
});

test('appending preserves bottom placement including the maximum allowed start row', () => {
  for (const bottom of [6, 100000]) {
    const initial = [{ ...panel('a', 0, bottom - 6), w: 12 }];
    const result = appendPanel(initial, panel('b', 0, 0));
    validate(result);
    assert.deepEqual(result, [...initial, panel('b', 0, bottom)]);
  }
});

test('appending finds bounded free space when the bottom exceeds the stored row limit', () => {
  for (const width of [6, 12]) {
    const initial = [{ ...panel('a', 0, 0), w: width }, { ...panel('b', 0, 100000), w: 12, h: 1 }];
    validate(initial);
    const result = appendPanel(initial, panel('c', 0, 0));
    validate(result);
    assert.deepEqual(result.slice(0, -1), initial);
    assert.deepEqual(result.at(-1), width === 6 ? panel('c', 6, 0) : panel('c', 0, 6));
  }
});

test('appending keeps the valid draft when no bounded placement or panel capacity remains', () => {
  const full = Array.from({ length: 101 }, (_, index) => ({ ...panel(String(index), 0, index * 1000), w: 12, h: 1000 }));
  const maximum = Array.from({ length: 200 }, (_, index) => panel(String(index), 0, index * 6));
  for (const initial of [full, maximum]) {
    validate(initial);
    const result = appendPanel(initial, panel('new', 0, 0));
    validate(result);
    assert.equal(result, initial);
  }
});
