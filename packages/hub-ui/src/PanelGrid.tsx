import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  type Ref,
} from 'react';
import { flushSync } from 'react-dom';
import GridLayout, {
  noCompactor,
  useContainerWidth,
  verticalCompactor,
  type EventCallback,
  type Layout,
} from 'react-grid-layout';
import {
  defaultConstraints,
  type LayoutConstraint,
} from 'react-grid-layout/core';
import { useTranslation } from 'react-i18next';
import { Button } from './Button.js';
import type { Panel } from './contracts.js';
import {
  applyGridLayout,
  GRID_COLUMNS,
  GRID_GAP,
  GRID_ROW_HEIGHT,
  toGridLayout,
} from './layout.js';

const constraints: LayoutConstraint[] = [
  ...defaultConstraints,
  {
    name: 'stored-row-limit',
    constrainPosition: (_item, x, y) => ({ x, y: Math.min(100000, y) }),
  },
];
const handles = ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'] as const;
// Persisted panel IDs must contain non-whitespace text. RGL filters this ID
// from layout callbacks, so keep its placeholder outside that namespace.
const droppingItem = { i: '', x: 0, y: 0, w: 1, h: 1 };

/** RGL owns pointer geometry; the editor owns the draft and explicit commits. */
export function PanelGrid({
  panels,
  editing,
  busy,
  narrow,
  onChange,
  children,
}: {
  panels: Panel[];
  editing: boolean;
  busy: boolean;
  narrow: boolean;
  onChange: (panels: Panel[]) => void;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const { width, containerRef } = useContainerWidth();
  const savedLayout = useMemo(() => toGridLayout(panels), [panels]);
  const [layout, setLayout] = useState(savedLayout);
  const gesture = useRef<{
    layout: Layout;
    event: Event;
    cancelled: boolean;
  } | null>(null);
  const restoreLayout = useRef<Layout | null>(null);
  const enabled = editing && !busy && !narrow;

  useEffect(() => {
    setLayout(savedLayout);
  }, [savedLayout]);

  useEffect(() => {
    if (restoreLayout.current) {
      const original = restoreLayout.current;
      restoreLayout.current = null;
      setLayout(original);
    }
  }, [layout]);

  const cancel = () => {
    const current = gesture.current;
    if (!current) return;
    current.cancelled = true;
    // Finish react-draggable's document listeners before restoring controlled
    // geometry. Flushing both steps avoids remounting live SurfaceView children.
    flushSync(() => {
      document.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }));
      if (
        typeof TouchEvent !== 'undefined' &&
        current.event instanceof TouchEvent
      ) {
        document.dispatchEvent(
          new TouchEvent('touchend', {
            bubbles: true,
            changedTouches: [...current.event.changedTouches],
          }),
        );
      }
    });
    gesture.current = null;
    setLayout(current.layout);
  };
  useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && gesture.current) {
        event.preventDefault();
        cancel();
      }
    };
    window.addEventListener('keydown', escape);
    window.addEventListener('blur', cancel);
    window.addEventListener('resize', cancel);
    return () => {
      window.removeEventListener('keydown', escape);
      window.removeEventListener('blur', cancel);
      window.removeEventListener('resize', cancel);
    };
  });
  const start: EventCallback = (
    _layout,
    _old,
    _item,
    _placeholder,
    event,
    element,
  ) => {
    gesture.current = { layout: savedLayout, event, cancelled: false };
    element
      ?.querySelector<HTMLElement>('.drag-handle')
      ?.focus({ preventScroll: true });
  };
  const stop = (next: Layout) => {
    if (!gesture.current || gesture.current.cancelled) return;
    gesture.current = null;
    const updated = applyGridLayout(panels, next);
    // Let RGL finish publishing its result before restoring a rejected or
    // unchanged layout. Its synchronization compares the last controlled props.
    if (updated === panels) restoreLayout.current = savedLayout;
    setLayout(next);
    if (updated !== panels) onChange(updated);
  };

  return (
    <div ref={containerRef} onPointerCancel={cancel} onTouchCancel={cancel}>
      <GridLayout
        width={width}
        layout={layout}
        className={`panel-grid ${editing ? 'editing' : ''}`}
        gridConfig={{
          cols: GRID_COLUMNS,
          rowHeight: GRID_ROW_HEIGHT,
          margin: [GRID_GAP, GRID_GAP],
          containerPadding: [0, 0],
        }}
        constraints={constraints}
        droppingItem={droppingItem}
        compactor={editing && !narrow ? verticalCompactor : noCompactor}
        // Track the entire press so cancellation also removes pending document listeners.
        dragConfig={{
          enabled,
          handle: '.drag-handle',
          allowMobileScroll: true,
          threshold: 0,
        }}
        resizeConfig={{
          enabled,
          handles,
          handleComponent: (edge, ref) => (
            <Button
              static
              ref={ref as Ref<HTMLButtonElement>}
              className={`react-resizable-handle resize-edge resize-${edge}`}
              aria-label={t('view.resizeHandle', { edge: t(`edge.${edge}`) })}
              tabIndex={-1}
              disabled={!enabled}
            />
          ),
        }}
        onLayoutChange={setLayout}
        onDragStart={start}
        onResizeStart={start}
        onDragStop={stop}
        onResizeStop={stop}
      >
        {children}
      </GridLayout>
    </div>
  );
}
