import {
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from './Button.js';
import { DialogHeader, restoreDialogFocus } from './dialogs.js';

/** Connection status stays visible; management opens without moving the board. */
export function ConnectionMenu({
  name,
  status,
  statusLabel,
  available,
  children,
  readOnly = false,
}: {
  name: string;
  status: string;
  statusLabel: string;
  available: number;
  children?: ReactNode;
  readOnly?: boolean;
}) {
  const { t } = useTranslation();
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const [open, setOpen] = useState(false);
  const connected = status === 'Connected';
  const pending = status === 'Connecting' || status === 'Reconnecting';
  const manageable = !!children && !readOnly;
  const announcement = `${t('workspace.connectionStatus', { target: name, status: statusLabel })}. ${t('workspace.apps', { count: available })}`;
  const close = () => {
    dialog.current?.close();
    setOpen(false);
    restoreDialogFocus(trigger.current);
  };

  useLayoutEffect(() => {
    const panel = dialog.current;
    if (!open || !manageable || !panel) return;
    const position = () => {
      const anchor = trigger.current?.getBoundingClientRect();
      if (!anchor) return;
      const top = Math.max(
        16,
        Math.min(anchor.bottom + 8, window.innerHeight - 200),
      );
      const left = Math.max(
        16,
        Math.min(
          anchor.left,
          document.documentElement.clientWidth - panel.offsetWidth - 16,
        ),
      );
      panel.style.top = `${top}px`;
      panel.style.left = `${left}px`;
      panel.style.maxHeight = `${Math.max(0, window.innerHeight - top - 16)}px`;
    };
    panel.showModal();
    position();
    const resize = new ResizeObserver(position);
    if (trigger.current) resize.observe(trigger.current);
    window.addEventListener('resize', position);
    window.addEventListener('scroll', position, true);
    return () => {
      resize.disconnect();
      window.removeEventListener('resize', position);
      window.removeEventListener('scroll', position, true);
      panel.close();
      restoreDialogFocus(trigger.current);
    };
  }, [open, manageable]);

  const summary = (
    <>
      <span className="connection-symbol" aria-hidden="true">
        {connected ? '✓' : pending ? '…' : '!'}
      </span>
      <strong className="connection-label">{name}</strong>
      <span className="connection-count">
        {connected
          ? t('workspace.appCount', { count: available })
          : statusLabel}
      </span>
    </>
  );
  return (
    <div
      className={`connection-control ${connected ? 'is-connected' : pending ? 'is-pending' : 'is-disconnected'}`}
      data-connection-state={status}
    >
      <span className="sr-only" role="status">
        {announcement}
      </span>
      {manageable ? (
        <Button
          ref={trigger}
          className="connection-chip"
          aria-label={`${name}: ${t('workspace.connections')}. ${statusLabel}. ${t('workspace.apps', { count: available })}`}
          aria-haspopup="dialog"
          aria-expanded={open}
          aria-controls={id}
          onClick={() => setOpen(true)}
        >
          {summary}
          <svg
            className="connection-chevron"
            viewBox="0 0 16 16"
            width="16"
            height="16"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            aria-hidden="true"
            focusable="false"
          >
            <path d="m5 6 3 3 3-3" />
          </svg>
        </Button>
      ) : (
        <div className="connection-chip connection-summary">{summary}</div>
      )}
      {children && (
        <dialog
          ref={dialog}
          id={id}
          className="connection-popover"
          aria-labelledby={`${id}-title`}
          onCancel={(event) => {
            event.preventDefault();
            close();
          }}
          onClick={(event) => {
            if (event.target !== event.currentTarget) return;
            const rect = event.currentTarget.getBoundingClientRect();
            if (
              event.clientX < rect.left ||
              event.clientX > rect.right ||
              event.clientY < rect.top ||
              event.clientY > rect.bottom
            )
              close();
          }}
        >
          <header className="connection-popover-heading">
            <span className="connection-eyebrow">
              {t('workspace.connections')}
            </span>
            <DialogHeader id={`${id}-title`} title={name} onClose={close} />
          </header>
          <p className="connection-health">
            <span className="connection-symbol" aria-hidden="true">
              {connected ? '✓' : pending ? '…' : '!'}
            </span>
            <span>
              <strong>{statusLabel}</strong>
              <small>{t('workspace.apps', { count: available })}</small>
            </span>
          </p>
          {children}
        </dialog>
      )}
    </div>
  );
}
