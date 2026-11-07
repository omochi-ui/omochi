import { useId, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../Button.js';
import type { ConnectionAction, ConnectionState } from './connection-controller.js';
import { nativeConnectionLink } from './native-link.js';

export function CurrentConnectionActions({ state, dispatch }: {
  state: ConnectionState;
  dispatch(action: ConnectionAction): Promise<void>;
}) {
  const { t } = useTranslation();
  const helpId = useId();
  const remembered = useRef<HTMLSpanElement>(null);
  const { active, entries, busy, invitation, feedback } = state;
  if (!active) return null;
  const link = active.kind === 'public' ? nativeConnectionLink(active.link) : null;
  const persistent = entries.find(entry => entry.id === active.id)?.persistent;
  const remember = async () => {
    await dispatch({ type: 'saveShare', id: active.id });
    requestAnimationFrame(() => {
      if (document.activeElement === document.body) remembered.current?.focus();
    });
  };
  return <div className="current-connection-actions" role="group" aria-label={t('connection.current')} data-current-connection-id={active.id}>
    {link && <>
      {persistent
        ? <span className="connection-remembered" ref={remembered} tabIndex={-1}>✓ {t('connection.remembered')}</span>
        : <Button className="connection-remember" disabled={busy} aria-describedby={helpId} title={t('connection.saveHelp')} onClick={() => { void remember(); }}>{t('connection.saveShare')}</Button>}
      <span id={helpId} className="sr-only">{t('connection.saveHelp')}</span>
      <a className="hub-button connection-open-app" href={link}>{t('connection.openApp')}</a>
    </>}
    <Button className="connection-disconnect" disabled={busy || !!invitation} onClick={() => { void dispatch({ type: 'disconnect' }); }}>{t('connection.disconnect')}</Button>
    <span className="sr-only" role="status">{feedback === 'shareSaved' && t('connection.shareSaved')}</span>
  </div>;
}
