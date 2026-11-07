import { useState, type Ref } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../Button.js';
import { Modal } from '../dialogs.js';
import type { ConnectionAction, ConnectionState, ConnectionEntry } from './connection-controller.js';
import { nativeConnectionLink } from './native-link.js';

export function ConnectionControls({ state, dispatch, heading, onForget, onReset }: {
  state: ConnectionState;
  dispatch(action: ConnectionAction): Promise<void>;
  heading?: Ref<HTMLHeadingElement>;
  onForget?: (record: ConnectionEntry, trigger: HTMLElement) => void;
  onReset(trigger: HTMLElement): void;
}) {
  const { t } = useTranslation();
  const { active, browserId, entries, busy, error, listError } = state;
  const [manage, setManage] = useState<{ entry: ConnectionEntry; trigger: HTMLElement } | null>(null);
  const [name, setName] = useState('');
  const [invalid, setInvalid] = useState(false);
  const appLink = active?.kind === 'public' ? nativeConnectionLink(active.link) : null;
  const owners = entries.filter(e => e.owner && e.hubId === active?.id.split('/')[0]);
  const label = (entry: ConnectionEntry) => entry.name || t('connection.defaultName');
  const select = (id: string) => { void dispatch({ type: 'select', id }); };
  return <section className="shared-connections" aria-label={t('connection.label')}>
    <h2 ref={heading} tabIndex={-1}>{t('connection.title')}</h2>
    {active && !active.owner && <div className="share-context">
      <p>{t('connection.viewingShare')}</p>

      {owners.length === 1 ? <Button disabled={busy} onClick={() => select(owners[0]!.id)}>{t('connection.returnOwner')}</Button>
        : owners.length > 1 ? <details><summary>{t('connection.returnOwner')}</summary>{owners.map(owner => <Button key={owner.id} disabled={busy} onClick={() => select(owner.id)}>{label(owner)} · {owner.grantId.slice(0, 8)}</Button>)}</details> : <p>{t('connection.chooseConnection')}</p>}
    </div>}
    {listError && <div role="alert"><p>{t(`sessionError.${listError.code}`)}</p><Button disabled={busy} onClick={() => { void dispatch({ type: 'refresh' }); }}>{t('connection.retryOpen')}</Button></div>}
    {!entries.length && <p>{busy ? t('connection.loading') : t('connection.empty')}</p>}
    <div className="connection-list">
      {entries.map(entry => <article className={`connection-record ${active?.id === entry.id ? 'selected' : ''}`} key={entry.id} data-connection-id={entry.id} data-persistent={entry.persistent}>
        <Button static className="connection-select" aria-current={active?.id === entry.id ? 'true' : undefined} disabled={busy} onClick={() => select(entry.id)}>
          <strong>{label(entry)}</strong>
          {entry.link && active?.id === entry.id && <small>{active.view?.permissions.interact ? t('connection.canInteract') : t('connection.viewOnly')} · {active.view?.permissions.editLayout ? t('connection.canEdit') : t('connection.noEdit')}</small>}
          <small>{entry.owner ? t('connection.ownerRole') : t('connection.shareRole')} · {entry.hubId.slice(0, 8)}{entries.filter(e => e.hubId === entry.hubId && e.owner === entry.owner).length > 1 ? ` / ${entry.grantId.slice(0, 4)}` : ''}</small>
          {(entry.state === 'pending' || active?.id === entry.id || !entry.persistent) && <small>{entry.state === 'pending' ? t('connection.pending') : active?.id === entry.id ? `${active.status === 'Connected' ? '✓' : active.status === 'Connecting' || active.status === 'Reconnecting' ? '…' : '!'} ${t(`status.${active.status}` as 'status.Connected')}` : !entry.persistent ? t('connection.tabOnly') : t('connection.saved')}</small>}
        </Button>
        <Button disabled={busy} className="connection-manage" aria-label={t('connection.manage', { name: label(entry) })} aria-haspopup="dialog" onClick={event => { setName(label(entry)); setInvalid(false); setManage({ entry, trigger: event.currentTarget }); }}>…</Button>
      </article>)}
    </div>
    <div className="toolbar connection-actions">
      {active && <Button disabled={busy || !!state.invitation} onClick={() => { void dispatch({ type: 'disconnect' }); }}>{t('connection.disconnect')}</Button>}
      {active && active.status !== 'Connected' && <Button disabled={busy} onClick={() => { void dispatch({ type: 'retry' }); }}>{t('connection.retry')}</Button>}
      {error && !active && <Button disabled={busy} onClick={() => { void dispatch({ type: 'retry' }); }}>{t('connection.retryOpen')}</Button>}
    </div>
    <details className="connection-details">
      <summary>{t('connection.details')}</summary>
      <dl>{active && <><dt>{t('connection.hubId')}</dt><dd><code>{active.id.split('/')[0]}</code></dd></>}
        {!active && browserId && <><dt>{t('connection.browserId')}</dt><dd><code data-browser-id={browserId}>{browserId}</code></dd></>}
      </dl>
      {appLink && <p><a href={appLink}>{t('native.open')}</a></p>}
      {!active && <Button disabled={busy} className="danger" onClick={event => onReset(event.currentTarget)}>{t('connection.reset')}</Button>}
    </details>
    {manage && <Modal labelledBy="connection-manage-title" title={t('connection.manage', { name: label(manage.entry) })} returnTo={manage.trigger} onClose={() => setManage(null)}>
      <form onSubmit={event => { event.preventDefault(); if (!name.trim()) { setInvalid(true); return; } void dispatch({ type: 'rename', id: manage.entry.id, name }); }}>
        <label htmlFor="connection-name">{t('connection.name')}</label>
        <input id="connection-name" value={name} maxLength={200} aria-invalid={invalid} aria-describedby={invalid ? 'connection-name-error' : undefined} onChange={e => setName(e.target.value)} />
        {invalid && <p id="connection-name-error" role="alert">{t('workspace.nameRequired')}</p>}
        <Button type="submit" disabled={busy}>{t('connection.rename')}</Button>
      </form>
      <p role="status">{state.feedback && t(`connection.${state.feedback}`)}</p>
      {error && <p role="alert">{t(`sessionError.${error.code}`)}</p>}
      <dl><dt>{t('connection.hubId')}</dt><dd><code>{manage.entry.hubId}</code></dd><dt>{t('connection.grantId')}</dt><dd><code>{manage.entry.grantId}</code></dd></dl>
      {manage.entry.link && <><p>{t('connection.saveHelp')}</p><Button disabled={busy} onClick={() => { void dispatch({ type: 'saveShare', id: manage.entry.id }); }}>{t('connection.saveShare')}</Button></>}
      {onForget && <Button disabled={busy} className="danger" onClick={() => { const current = manage; setManage(null); onForget(current.entry, current.trigger); }}>{t('connection.forget')}</Button>}
    </Modal>}
  </section>;
}
