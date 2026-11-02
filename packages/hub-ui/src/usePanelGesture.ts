import { useEffect, useRef, useState, type PointerEvent } from 'react';
import type { Panel } from './contracts.js';
import { placePanel, resizePanel, type ResizeEdge } from './layout.js';

export function usePanelGesture({
  panels,
  editing,
  busy,
  narrow,
  onBegin,
  onCommit,
}: {
  panels: Panel[];
  editing: boolean;
  busy: boolean;
  narrow: boolean;
  onBegin: () => void;
  onCommit: (panels: Panel[]) => void;
}) {
  const grid = useRef<HTMLDivElement>(null);
  const [preview, setPreview] = useState<Panel[] | null>(null);
  const previewRef = useRef<Panel[] | null>(null);
  const gesture = useRef<{
    id: string;
    mode: 'move' | ResizeEdge;
    x: number;
    y: number;
    scrollY: number;
    column: number;
    row: number;
    panel: Panel;
    panels: Panel[];
  } | null>(null);
  const cancelGesture = () => {
    gesture.current = null;
    previewRef.current = null;
    setPreview(null);
  };
  const beginGesture = (event: PointerEvent<HTMLButtonElement>, panel: Panel, mode: 'move' | ResizeEdge) => {
    if (narrow || !editing || busy || !grid.current || event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.focus();
    event.currentTarget.setPointerCapture(event.pointerId);
    const style = getComputedStyle(grid.current);
    const gap = parseFloat(style.columnGap);
    gesture.current = {
      id: panel.id,
      mode,
      x: event.clientX,
      y: event.clientY,
      scrollY: window.scrollY,
      column: (grid.current.clientWidth + gap) / 12,
      row: parseFloat(style.gridAutoRows) + parseFloat(style.rowGap),
      panel,
      panels,
    };
    onBegin();
  };
  const moveGesture = (event: PointerEvent<HTMLButtonElement>) => {
    const g = gesture.current;
    if (!g || narrow) return;
    const dx = Math.round((event.clientX - g.x) / g.column);
    const dy = Math.round((event.clientY - g.y + window.scrollY - g.scrollY) / g.row);
    const panel =
      g.mode === 'move'
        ? { ...g.panel, x: g.panel.x + dx, y: Math.min(100000, g.panel.y + dy) }
        : resizePanel(g.panel, g.mode, dx, dy);
    previewRef.current = placePanel(g.panels, panel);
    setPreview(previewRef.current);
  };
  const endGesture = () => {
    if (gesture.current && previewRef.current) {
      const panels = previewRef.current;
      onCommit(panels);
    }
    cancelGesture();
  };
  useEffect(() => {
    cancelGesture();
  }, [narrow, editing]);
  return {
    grid,
    preview,
    beginGesture,
    moveGesture,
    endGesture,
    cancelGesture,
    cancelOnEscape: (event: React.KeyboardEvent) => {
      if (event.key === 'Escape' && gesture.current) {
        event.preventDefault();
        cancelGesture();
      }
    },
  };
}
