import { useId, useLayoutEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from './Button.js';
import type { Panel } from './contracts.js';
import { resizePanel } from './layout.js';

export function PanelLayoutControls({ panel, busy, onChange }: {
  panel: Panel; busy: boolean; onChange: (panel: Panel) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const controls = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);

  useLayoutEffect(() => {
    const menu = controls.current;
    const button = trigger.current;
    if (!open || !menu || !button) return;
    const position = () => {
      const anchor = button.getBoundingClientRect();
      const above = anchor.top - menu.offsetHeight - 8;
      const top = above >= 16 ? above : anchor.bottom + 8;
      menu.style.top = `${Math.max(16, Math.min(top, window.innerHeight - menu.offsetHeight - 16))}px`;
      menu.style.left = `${Math.max(16, Math.min(anchor.left, document.documentElement.clientWidth - menu.offsetWidth - 16))}px`;
    };
    position();
    const observer = new ResizeObserver(position);
    observer.observe(menu); observer.observe(button);
    window.addEventListener('resize', position);
    window.addEventListener('scroll', position, true);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', position);
      window.removeEventListener('scroll', position, true);
    };
  }, [open, panel.x, panel.y, panel.w, panel.h]);

  const control = (action: 'shorter' | 'up' | 'narrower' | 'left' | 'right' | 'wider' | 'down' | 'taller', next: Panel, boundary: boolean) => (
    <Button aria-label={t(`view.${action}`)} title={t(`view.${action}`)} disabled={busy || boundary} onClick={() => onChange(next)}>
      {t(`view.adjustment.${action}`)}
    </Button>
  );

  return <>
    <Button ref={trigger} popoverTarget={id} disabled={busy}>{t('view.adjust')}</Button>
    <div ref={controls} id={id} popover="auto" className="adjust-controls" role="group" aria-label={t('view.adjustLabel', { name: panel.title })}
      onToggle={event => setOpen(event.newState === 'open')}>
      <div className="adjust-row">{control('shorter', resizePanel(panel, 's', 0, -1), panel.h === 1)}</div>
      <div className="adjust-row">{control('up', { ...panel, y: panel.y - 1 }, panel.y === 0)}</div>
      <div className="adjust-row adjust-horizontal">
        {control('narrower', resizePanel(panel, 'e', -1, 0), panel.w === 1)}
        {control('left', { ...panel, x: panel.x - 1 }, panel.x === 0)}
        {control('right', { ...panel, x: panel.x + 1 }, panel.x + panel.w === 12)}
        {control('wider', resizePanel(panel, 'e', 1, 0), panel.x + panel.w === 12)}
      </div>
      <div className="adjust-row">{control('down', { ...panel, y: panel.y + 1 }, panel.y >= 100000)}</div>
      <div className="adjust-row">{control('taller', resizePanel(panel, 's', 0, 1), panel.h === 1000)}</div>
    </div>
  </>;
}
