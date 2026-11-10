import { overlaps, type Panel } from './contracts.js';
import {
  cloneLayout,
  moveElement,
  verticalCompactor,
  type Layout,
} from 'react-grid-layout/core';

export const GRID_COLUMNS = 12;
export const GRID_ROW_HEIGHT = 40;
export const GRID_GAP = 16;

/** Keep library metadata out of the persisted board contract. */
export function toGridLayout(panels: readonly Panel[]): Layout {
  return panels.map(({ id, x, y, w, h }) => ({
    i: id,
    x,
    y,
    w,
    h,
    minW: 1,
    maxW: 12,
    minH: 1,
    maxH: 1000,
  }));
}

export function applyGridLayout(panels: Panel[], layout: Layout): Panel[] {
  const byId = new Map(layout.map((item) => [item.i, item]));
  if (layout.length !== panels.length || byId.size !== panels.length)
    return panels;
  const next = panels.map((panel) => {
    const item = byId.get(panel.id);
    return item
      ? { ...panel, x: item.x, y: item.y, w: item.w, h: item.h }
      : panel;
  });
  if (
    panels.some((panel) => !byId.has(panel.id)) ||
    next.some(
      (panel) =>
        ![panel.x, panel.y, panel.w, panel.h].every(Number.isInteger) ||
        panel.x < 0 ||
        panel.y < 0 ||
        panel.y > 100000 ||
        panel.w < 1 ||
        panel.x + panel.w > 12 ||
        panel.h < 1 ||
        panel.h > 1000,
    ) ||
    next.some((panel, index) =>
      next.slice(index + 1).some((other) => overlaps(panel, other)),
    )
  )
    return panels;
  return next.every((panel, i) => {
    const original = panels[i]!;
    return (
      panel.x === original.x &&
      panel.y === original.y &&
      panel.w === original.w &&
      panel.h === original.h
    );
  })
    ? panels
    : next;
}

export function compactPanels(panels: Panel[]): Panel[] {
  return applyGridLayout(
    panels,
    verticalCompactor.compact(toGridLayout(panels), GRID_COLUMNS),
  );
}

/** Use the same vertical packing and movement rules as pointer editing. */
export function placePanel(panels: Panel[], changed: Panel): Panel[] {
  const original = panels.find((panel) => panel.id === changed.id);
  if (!original) return panels;
  const next = {
    ...changed,
    w: clamp(changed.w, 1, 12),
    h: clamp(changed.h, 1, 1000),
  };
  next.x = clamp(next.x, 0, 12 - next.w);
  next.y = clamp(next.y, 0, 100000);
  const layout = cloneLayout(toGridLayout(panels));
  const item = layout.find((item) => item.i === next.id)!;
  let placed: Layout;
  if (item.w === next.w && item.h === next.h) {
    placed = moveElement(
      layout,
      item,
      next.x,
      next.y,
      true,
      false,
      'vertical',
      GRID_COLUMNS,
    );
  } else {
    Object.assign(item, { x: next.x, y: next.y, w: next.w, h: next.h });
    placed = layout;
  }
  const result = applyGridLayout(
    panels,
    verticalCompactor.compact(placed, GRID_COLUMNS),
  );
  // Geometry validation failure must also preserve the original panel metadata.
  if (
    result === panels &&
    (next.x !== original.x ||
      next.y !== original.y ||
      next.w !== original.w ||
      next.h !== original.h)
  )
    return panels;
  return result.map((panel) =>
    panel.id === next.id
      ? { ...next, x: panel.x, y: panel.y, w: panel.w, h: panel.h }
      : panel,
  );
}

export type ResizeEdge = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw';
const clamp = (value: number, min: number, max: number) =>
  Math.max(min, Math.min(max, value));

/** Resize from the original rectangle, keeping the opposite edge anchored. */
export function resizePanel(
  panel: Panel,
  edge: ResizeEdge,
  dx: number,
  dy: number,
): Panel {
  let { x, y, w, h } = panel;
  if (edge.includes('w')) {
    x = clamp(x + dx, 0, panel.x + panel.w - 1);
    w = panel.x + panel.w - x;
  }
  if (edge.includes('e')) w = clamp(w + dx, 1, 12 - x);
  if (edge.includes('n')) {
    y = clamp(
      y + dy,
      Math.max(0, panel.y + panel.h - 1000),
      Math.min(100000, panel.y + panel.h - 1),
    );
    h = panel.y + panel.h - y;
  }
  if (edge.includes('s')) h = clamp(h + dy, 1, 1000);
  return { ...panel, x, y, w, h };
}

/** Explicit mobile reorder repacks rows, retaining the draft if it exceeds storage bounds. */
export function reorderPanels(
  panels: Panel[],
  id: string,
  direction: -1 | 1,
): Panel[] {
  const ordered = [...panels].sort((a, b) => a.y - b.y || a.x - b.x);
  const index = ordered.findIndex((panel) => panel.id === id);
  const target = index + direction;
  if (index < 0 || target < 0 || target >= ordered.length) return panels;
  [ordered[index], ordered[target]] = [ordered[target]!, ordered[index]!];
  let x = 0,
    y = 0,
    height = 0;
  const result: Panel[] = [];
  for (const panel of ordered) {
    if (x + panel.w > 12) {
      y += height;
      x = 0;
      height = 0;
    }
    if (y > 100000) return panels;
    result.push({ ...panel, x, y });
    x += panel.w;
    height = Math.max(height, panel.h);
  }
  // Vertical compaction can lift a later panel into an earlier row's gap,
  // undoing the requested order when the workspace sorts by y/x.
  return result;
}

/** Append below the layout or into free space; retain the draft if no capacity remains. */
export function appendPanel(panels: Panel[], panel: Panel): Panel[] {
  if (panels.length >= 200) return panels;
  const bottom = Math.max(0, ...panels.map((p) => p.y + p.h));
  if (bottom <= 100000)
    return compactPanels([...panels, { ...panel, x: 0, y: bottom }]);
  const rows = [0, ...new Set(panels.map((p) => p.y + p.h))]
    .filter((y) => y <= 100000)
    .sort((a, b) => a - b);
  for (const y of rows) {
    for (let x = 0; x + panel.w <= 12; x++) {
      const next = { ...panel, x, y };
      if (!panels.some((p) => overlaps(p, next)))
        return compactPanels([...panels, next]);
    }
  }
  return panels;
}
