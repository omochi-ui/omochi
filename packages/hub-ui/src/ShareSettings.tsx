import { SharingFault } from './sharing-repository.js';
import type { TranslationMessage } from './i18n.js';
import { localizedError } from './localized-error.js';
import { useTranslation } from 'react-i18next';
import { useEffect, useRef, useState } from 'react';
import { Button } from './Button.js';
import { DialogHeader } from './dialogs.js';
interface Grant { id: string; boardId: string; permissions: { interact: boolean; editLayout: boolean } }
interface Host { hubId?: string; relayUrls?: string[]; enabled?: boolean }
import type { SharingRepository } from './sharing-repository.js';
/** Owner-only UI. Shared clients are not given an owner repository or this entry. */
export function ShareSettings({ boardId, repository }: { boardId: string; repository: SharingRepository }) {
  const { t } = useTranslation();
  const request = (path: string, method?: string, body?: unknown) => repository.request(path, method, body);
  const dialog = useRef<HTMLDialogElement>(null), linkText = useRef<HTMLTextAreaElement>(null);
  const [open, setOpen] = useState(false), [busy, setBusy] = useState(false), [error, setError] = useState<Error | null>(null);
  const [host, setHost] = useState<Host | null>(null), [grants, setGrants] = useState<Grant[]>([]);
  const [interact, setInteract] = useState(false), [editLayout, setEditLayout] = useState(false);
  const [link, setLink] = useState<{ id: string; url: string } | null>(null), [confirm, setConfirm] = useState<string | null>(null), [message, setMessage] = useState<TranslationMessage | null>(null);
  const refresh = async () => {
    const [info, records] = await Promise.all([request('/api/sharing'), request('/api/shares')]);
    if (!info || typeof info !== 'object' || !Array.isArray(records)) throw new SharingFault('infoUnavailable');
    setHost(info as Host); setGrants((records as Grant[]).filter(grant => grant.boardId === boardId));
  };
  const run = async (operation: () => Promise<void>) => { setBusy(true); setError(null); try { await operation(); } catch (error) { setError(error instanceof Error ? error : new SharingFault('unavailable')); } finally { setBusy(false); } };
  useEffect(() => { if (open) dialog.current?.showModal(); else dialog.current?.close(); }, [open]);
  const ready = !!host?.hubId && !!host.relayUrls?.length;
  return <>
    <Button disabled={busy} onClick={() => { setOpen(true); setLink(null); setConfirm(null); setMessage(null); void run(refresh); }}>{t('share.title')}</Button>
    <dialog ref={dialog} className="picker share-dialog" aria-labelledby="share-heading" onClose={() => { setOpen(false); setLink(null); }} onCancel={event => { if (busy) event.preventDefault(); }}>
      <DialogHeader id="share-heading" title={t('share.title')} onClose={() => setOpen(false)} disabled={busy} />
      <p>{t('share.description')}</p>
      {error && <p role="alert">{localizedError(error, t)}</p>}
      <p role="status">{message && t(message.key, { ...message.values })}</p>
      {!ready && <p role="status">{host?.enabled === false ? t('share.enable') : t('share.waiting')}</p>}
      <fieldset disabled={busy || !ready}>
        <legend>{t('share.create')}</legend>
        <p>{t('share.publicHelp')}</p>
        <label className="checkbox-field"><input type="checkbox" checked={interact} onChange={e => setInteract(e.target.checked)} /> {t('share.interact')}</label>
        <label className="checkbox-field"><input type="checkbox" checked={editLayout} onChange={e => setEditLayout(e.target.checked)} /> {t('share.edit')}</label>
        <p>{t('share.permissionsHelp')}</p>
        <Button className="primary" onClick={() => { void run(async () => {
          const result = await request('/api/shares', 'POST', { boardId, permissions: { interact, editLayout } }) as { grant: Grant; url?: string };
          if (typeof result.url !== 'string' || !result.url.startsWith('https:')) { await refresh(); throw new SharingFault('relayUnavailable'); }
          setLink({ id: result.grant.id, url: result.url }); setMessage({ key: 'share.created' }); await refresh();
        }); }}>{t('share.createLink')}</Button>
      </fieldset>
      {link && <section><label htmlFor="sharing-link">{t('share.link')}</label><textarea ref={linkText} id="sharing-link" aria-label={t('share.link')} readOnly rows={3} value={link.url} onFocus={e => e.target.select()} /><Button onClick={() => { void run(async () => { if (navigator.clipboard) { await navigator.clipboard.writeText(link.url); setMessage({ key: 'share.copied' }); } else { linkText.current?.focus(); linkText.current?.select(); setMessage({ key: 'share.selected' }); } }); }}>{t('share.copy')}</Button></section>}
      <h3>{t('share.existing')}</h3>
      {!grants.length && <p>{t('share.empty')}</p>}
      {grants.map(grant => <article className="notice" key={grant.id} data-share-id={grant.id}>
        <span>{t('share.public')}{grant.permissions.interact && <small>{t('share.interactionAllowed')}</small>}{grant.permissions.editLayout && <small>{t('share.editingAllowed')}</small>}</span>
        <Button className="danger" disabled={busy} onClick={() => setConfirm(grant.id)}>{t('share.revoke')}</Button>
      </article>)}
      {confirm && <section role="group" aria-label={t('share.confirmLabel')}><p>{t('share.confirmHelp')}</p><div className="toolbar"><Button className="danger" disabled={busy} onClick={() => { void run(async () => { await request(`/api/shares/${encodeURIComponent(confirm)}`, 'DELETE'); if (link?.id === confirm) setLink(null); setConfirm(null); setMessage({ key: 'share.revoked' }); await refresh(); }); }}>{t('share.confirm')}</Button><Button disabled={busy} onClick={() => setConfirm(null)}>{t('share.cancel')}</Button></div></section>}
      <div className="dialog-actions"><Button disabled={busy} onClick={() => { void run(refresh); }}>{t('share.refresh')}</Button></div>
    </dialog>
  </>;
}
