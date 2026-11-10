import { useEffect, useId, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../Button.js';
import type {
  ConnectionAction,
  ConnectionState,
} from './connection-controller.js';
import { nativeConnectionLink } from './native-link.js';

const REMEMBERED_NOTICE_MS = 5_000;

export function CurrentConnectionActions({
  state,
  dispatch,
}: {
  state: ConnectionState;
  dispatch(action: ConnectionAction): Promise<void>;
}) {
  const { t } = useTranslation();
  const helpId = useId();
  const openApp = useRef<HTMLAnchorElement>(null);
  const pendingSave = useRef<{ id: string; button: HTMLButtonElement } | null>(
    null,
  );
  const [notice, setNotice] = useState<{ id: string } | null>(null);
  const { active, entries, busy, invitation, feedback } = state;
  const activeId = active?.id;
  const persistent = entries.find((entry) => entry.id === activeId)?.persistent;
  useEffect(() => {
    const request = pendingSave.current;
    if (!request) return;
    if (request.id !== activeId) {
      pendingSave.current = null;
      return;
    }
    if (busy) return;
    pendingSave.current = null;
    if (feedback === 'shareSaved' && persistent) setNotice({ id: request.id });
    if (document.activeElement === document.body) {
      (request.button.isConnected ? request.button : openApp.current)?.focus();
    }
  }, [activeId, busy, feedback, persistent]);
  useEffect(() => {
    if (!notice) return;
    if (notice.id !== activeId || !persistent) {
      setNotice(null);
      return;
    }
    const timer = window.setTimeout(
      () => setNotice(null),
      REMEMBERED_NOTICE_MS,
    );
    return () => window.clearTimeout(timer);
  }, [notice, activeId, persistent]);
  if (!active) return null;
  const link =
    active.kind === 'public' ? nativeConnectionLink(active.link) : null;
  const showRemembered = persistent && notice?.id === active.id;
  const remember = (button: HTMLButtonElement) => {
    pendingSave.current = { id: active.id, button };
    void dispatch({ type: 'saveShare', id: active.id });
  };
  return (
    <div
      className="current-connection-actions"
      role="group"
      aria-label={t('connection.current')}
      data-current-connection-id={active.id}
    >
      {link && (
        <>
          {persistent ? (
            showRemembered && (
              <span className="connection-remembered">
                ✓ {t('connection.remembered')}
              </span>
            )
          ) : (
            <Button
              className="connection-remember"
              disabled={busy}
              aria-describedby={helpId}
              title={t('connection.saveHelp')}
              onClick={(event) => remember(event.currentTarget)}
            >
              {t('connection.saveShare')}
            </Button>
          )}
          <span id={helpId} className="sr-only">
            {t('connection.saveHelp')}
          </span>
          <a
            className="hub-button connection-open-app"
            ref={openApp}
            href={link}
          >
            {t('connection.openApp')}
          </a>
        </>
      )}
      <Button
        className="connection-disconnect"
        disabled={busy || !!invitation}
        onClick={() => {
          void dispatch({ type: 'disconnect' });
        }}
      >
        {t('connection.disconnect')}
      </Button>
      <span className="sr-only" role="status">
        {showRemembered && t('connection.shareSaved')}
      </span>
    </div>
  );
}
