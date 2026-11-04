import { overlaps, type Panel } from './contracts.js';

/** Place the edited panel first, then push colliding panels down in visual order. */
export function placePanel(panels: Panel[], changed: Panel): Panel[] {
  const placed: Panel[] = [{ ...changed, w: Math.max(1, Math.min(12, changed.w)), h: Math.min(1000, Math.max(1, changed.h)) }];
  placed[0]!.x = Math.max(0, Math.min(12 - placed[0]!.w, changed.x));
  placed[0]!.y = Math.min(100000, Math.max(0, changed.y));
  for (const panel of panels.filter(p => p.id !== changed.id).sort((a, b) => a.y - b.y || a.x - b.x)) {
    const next = { ...panel };
    let collisions = placed.filter(p => overlaps(p, next));
    while (collisions.length) {
      next.y = Math.max(...collisions.map(p => p.y + p.h));
      collisions = placed.filter(p => overlaps(p, next));
    }
    if (next.y > 100000) return panels;
    placed.push(next);
  }
  return placed.sort((a, b) => a.y - b.y || a.x - b.x);
}

export type ResizeEdge = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw';
const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(max, value));

/** Resize from the original rectangle, keeping the opposite edge anchored. */
export function resizePanel(panel: Panel, edge: ResizeEdge, dx: number, dy: number): Panel {
  let { x, y, w, h } = panel;
  if (edge.includes('w')) { x = clamp(x + dx, 0, panel.x + panel.w - 1); w = panel.x + panel.w - x; }
  if (edge.includes('e')) w = clamp(w + dx, 1, 12 - x);
  if (edge.includes('n')) { y = clamp(y + dy, Math.max(0, panel.y + panel.h - 1000), Math.min(100000, panel.y + panel.h - 1)); h = panel.y + panel.h - y; }
  if (edge.includes('s')) h = clamp(h + dy, 1, 1000);
  return { ...panel, x, y, w, h };
}

/** Explicit mobile reorder repacks rows, retaining the draft if it exceeds storage bounds. */
export function reorderPanels(panels: Panel[], id: string, direction: -1 | 1): Panel[] {
  const ordered = [...panels].sort((a, b) => a.y - b.y || a.x - b.x);
  const index = ordered.findIndex(panel => panel.id === id);
  const target = index + direction;
  if (index < 0 || target < 0 || target >= ordered.length) return panels;
  [ordered[index], ordered[target]] = [ordered[target]!, ordered[index]!];
  let x = 0, y = 0, height = 0;
  const result: Panel[] = [];
  for (const panel of ordered) {
    if (x + panel.w > 12) { y += height; x = 0; height = 0; }
    if (y > 100000) return panels;
    result.push({ ...panel, x, y });
    x += panel.w; height = Math.max(height, panel.h);
  }
  return result;
}

/** Append below the layout or into free space; retain the draft if no capacity remains. */
export function appendPanel(panels: Panel[], panel: Panel): Panel[] {
  if (panels.length >= 200) return panels;
  const bottom = Math.max(0, ...panels.map(p => p.y + p.h));
  if (bottom <= 100000) return [...panels, { ...panel, x: 0, y: bottom }];
  const rows = [0, ...new Set(panels.map(p => p.y + p.h))].filter(y => y <= 100000).sort((a, b) => a - b);
  for (const y of rows) {
    for (let x = 0; x + panel.w <= 12; x++) {
      const next = { ...panel, x, y };
      if (!panels.some(p => overlaps(p, next))) return [...panels, next];
    }
  }
  return panels;
}
