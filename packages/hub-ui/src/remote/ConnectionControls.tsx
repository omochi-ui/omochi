import type { Ref } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../Button.js';
import type { ConnectionAction, ConnectionState } from './connection-controller.js';
import { nativeConnectionLink } from './native-link.js';
import type { ShareRecord } from './profile.js';

export function ConnectionControls({ state, dispatch, heading, onForget, onReset }: {
  state: ConnectionState;
  dispatch(action: ConnectionAction): Promise<void>;
  heading?: Ref<HTMLHeadingElement>;
  onForget?: (record: ShareRecord, trigger: HTMLElement) => void;
  onReset(trigger: HTMLElement): void;
}) {
  const { t } = useTranslation();
  const { active, browserId, records, busy, error } = state;
  const appLink = active?.kind === 'public' ? nativeConnectionLink(active.link) : null;
  return <section className="shared-connections" aria-label={t('connection.label')}>
    {active && !active.owner && <p>{t('connection.publicHelp')}</p>}
    {browserId && <>
      <h2 ref={heading} tabIndex={-1}>{t('connection.yours')}</h2>
      {!records.length && <p>{t('connection.empty')}</p>}
      {records.map(record => <article className="notice connection-record" key={record.id}>
        <span>
          {record.name}{record.owner && t('connection.ownerSuffix')}
          <small>{record.state === 'pending' ? t('connection.pending') : record.owner ? t('connection.ownerAccess') : t('connection.shareAccess')}</small>
        </span>
        <div className="toolbar">
          {active?.id === record.id
            ? <span className="current-connection">{t('connection.current')}</span>
            : <Button disabled={busy} onClick={() => { void dispatch({ type: 'select', id: record.id }); }}>
              {record.state === 'pending' ? t('connection.retryAccess') : t('connection.connect')}
            </Button>}
          {onForget && <Button disabled={busy} onClick={event => onForget(record, event.currentTarget)}>
            {t('connection.forget')}
          </Button>}
        </div>
      </article>)}
    </>}
    <div className="toolbar">
      {active && <Button disabled={busy} onClick={() => { void dispatch({ type: 'disconnect' }); }}>
        {t('connection.disconnect')}
      </Button>}
      {active && active.status !== 'Connected' && <Button disabled={busy} onClick={() => { void dispatch({ type: 'retry' }); }}>
        {t('connection.retry')}
      </Button>}
      {error && !browserId && !active && <Button disabled={busy} onClick={() => { void dispatch({ type: 'retry' }); }}>
        {t('connection.retryOpen')}
      </Button>}
    </div>
    <details className="connection-details">
      <summary>{t('connection.details')}</summary>
      <dl>
        {active && <><dt>{t('connection.hubId')}</dt><dd><code>{active.id.split('/')[0]}</code></dd></>}
        {browserId && <><dt>{t('connection.browserId')}</dt><dd><code data-browser-id={browserId}>{browserId}</code></dd></>}
      </dl>
      {appLink && <p><a href={appLink}>{t('native.open')}</a></p>}
      {!active && <Button disabled={busy} className="danger" onClick={event => onReset(event.currentTarget)}>
        {t('connection.reset')}
      </Button>}
    </details>
  </section>;
}
