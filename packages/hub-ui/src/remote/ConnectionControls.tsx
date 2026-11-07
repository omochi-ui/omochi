import { useState, type Ref } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../Button.js';
import { Modal } from '../dialogs.js';
import type { ConnectionAction, ConnectionState, ConnectionEntry } from './connection-controller.js';
import { nativeConnectionLink } from './native-link.js';

export function ConnectionControls({ state, dispatch, heading, showHeading = true, onForget }: {
  state: ConnectionState;
  dispatch(action: ConnectionAction): Promise<void>;
  heading?: Ref<HTMLHeadingElement>;
  showHeading?: boolean;
  onForget?: (record: ConnectionEntry, trigger: HTMLElement) => void;
}) {
  const { t } = useTranslation();
  const { active, entries, busy, error, listError } = state;
  const [manage, setManage] = useState<{ entry: ConnectionEntry; trigger: HTMLElement } | null>(null);
  const [name, setName] = useState('');
  const [invalid, setInvalid] = useState(false);
  const managedEntry = manage && (entries.find(entry => entry.id === manage.entry.id) ?? manage.entry);
  const managedActive = active?.id === managedEntry?.id ? active : null;
  const appLink = managedEntry?.link ? nativeConnectionLink(managedEntry.link) : null;
  const label = (entry: ConnectionEntry) => entry.name || t('connection.defaultName');
  const select = (id: string) => { void dispatch({ type: 'select', id }); };
  return <section className="shared-connections" aria-label={t('connection.label')}>
    <h2 ref={heading} hidden={!showHeading} tabIndex={-1}>{t('connection.title')}</h2>
    {listError && <div role="alert"><p>{t(`sessionError.${listError.code}`)}</p><Button disabled={busy} onClick={() => { void dispatch({ type: 'refresh' }); }}>{t('connection.retryOpen')}</Button></div>}
    {!entries.length && <p>{busy ? t('connection.loading') : t('connection.empty')}</p>}
    <div className="connection-list">
      {entries.map(entry => <article className={`connection-record ${active?.id === entry.id ? 'selected' : ''}`} key={entry.id} data-connection-id={entry.id} data-persistent={entry.persistent}>
        <Button static className="connection-select" aria-current={active?.id === entry.id ? 'true' : undefined} disabled={busy} onClick={() => select(entry.id)}>
          <strong>{label(entry)}</strong>
          <small>{entry.owner ? t('connection.ownerRole') : t('connection.shareRole')} · {entry.hubId.slice(0, 8)}{entries.filter(e => e.hubId === entry.hubId && e.owner === entry.owner).length > 1 ? ` / ${entry.grantId.slice(0, 4)}` : ''}</small>
          {(entry.state === 'pending' || active?.id === entry.id || !entry.persistent) && <small>{entry.state === 'pending' ? t('connection.pending') : active?.id === entry.id ? `${active.status === 'Connected' ? '✓' : active.status === 'Connecting' || active.status === 'Reconnecting' ? '…' : '!'} ${t(`status.${active.status}` as 'status.Connected')}` : !entry.persistent ? t('connection.tabOnly') : t('connection.saved')}</small>}
        </Button>
        <Button disabled={busy} className="connection-manage" aria-label={t('connection.manage', { name: label(entry) })} aria-haspopup="dialog" onClick={event => { setName(label(entry)); setInvalid(false); setManage({ entry, trigger: event.currentTarget }); }}>…</Button>
      </article>)}
    </div>
    {(active && active.status !== 'Connected' || error && !active) && <div className="toolbar connection-actions">
      {active && active.status !== 'Connected' && <Button disabled={busy} onClick={() => { void dispatch({ type: 'retry' }); }}>{t('connection.retry')}</Button>}
      {error && !active && <Button disabled={busy} onClick={() => { void dispatch({ type: 'retry' }); }}>{t('connection.retryOpen')}</Button>}
    </div>}
    {manage && managedEntry && <Modal className="connection-dialog" labelledBy="connection-manage-title" title={t('connection.manageTitle')} returnTo={manage.trigger} onClose={() => setManage(null)}>
      {managedActive && <section className="connection-session">
        <p>{t('connection.current')} · {t(`status.${managedActive.status}` as 'status.Connected')}</p>
        {!managedActive.owner && managedActive.view && <p>{managedActive.view.permissions.interact ? t('connection.canInteract') : t('connection.viewOnly')} · {managedActive.view.permissions.editLayout ? t('connection.canEdit') : t('connection.noEdit')}</p>}
        <div className="toolbar">
          <Button className="connection-disconnect" disabled={busy || !!state.invitation} onClick={() => { setManage(null); void dispatch({ type: 'disconnect' }); }}>{t('connection.disconnect')}</Button>
        </div>
      </section>}
      <form onSubmit={event => { event.preventDefault(); if (!name.trim()) { setInvalid(true); return; } void dispatch({ type: 'rename', id: manage.entry.id, name }); }}>
        <label htmlFor="connection-name">{t('connection.name')}</label>
        <input id="connection-name" value={name} maxLength={200} aria-invalid={invalid} aria-describedby={invalid ? 'connection-name-error' : undefined} onChange={e => setName(e.target.value)} />
        {invalid && <p id="connection-name-error" role="alert">{t('workspace.nameRequired')}</p>}
        <Button type="submit" disabled={busy}>{t('connection.rename')}</Button>
      </form>
      <p role="status">{state.feedback && t(`connection.${state.feedback}`)}</p>
      {error && <p role="alert">{t(`sessionError.${error.code}`)}</p>}
      <section className="connection-details">
        <dl><dt>{t('connection.hubId')}</dt><dd><code>{managedEntry.hubId}</code></dd><dt>{t('connection.grantId')}</dt><dd><code>{managedEntry.grantId}</code></dd></dl>
        {appLink && <p><a href={appLink}>{t('native.open')}</a></p>}
      </section>
      {managedEntry.link && !managedEntry.persistent && <section className="connection-save"><p>{t('connection.saveHelp')}</p><Button disabled={busy} onClick={() => { void dispatch({ type: 'saveShare', id: managedEntry.id }); }}>{t('connection.saveShare')}</Button></section>}
      {onForget && <div className="toolbar connection-forget"><Button disabled={busy} className="danger" onClick={() => { const current = manage; setManage(null); onForget(managedEntry, current.trigger); }}>{t('connection.forget')}</Button></div>}
    </Modal>}
  </section>;
}
