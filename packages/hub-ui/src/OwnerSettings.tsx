import { useTranslation } from 'react-i18next';
import { useState } from 'react';
import { Button } from './Button.js';
import { HttpSharingRepository } from './sharing-repository.js';
interface Owner { id: string; peer: string | null; invitationExpiresAt: number }
export function OwnerSettings() {
  const { t } = useTranslation();
  const [owners, setOwners] = useState<Owner[]>([]), [url, setUrl] = useState(''), [error, setError] = useState(false), [busy, setBusy] = useState(false);
  const repository = new HttpSharingRepository();
  const reload = async () => setOwners(await repository.request('/api/owners') as Owner[]);
  const run = async (operation: () => Promise<void>) => { setBusy(true); setError(false); try { await operation(); } catch { setError(true); } finally { setBusy(false); } };
  return <section aria-label={t('owner.title')}><h2>{t('owner.title')}</h2><p>{t('owner.description')}</p>
    {error && <p role="alert">{t('owner.error')}</p>}
    <Button disabled={busy} onClick={() => { void run(async () => { const result = await repository.request('/api/owners', 'POST') as { url: string }; setUrl(result.url); await reload(); }); }}>{t('owner.register')}</Button>
    <Button disabled={busy} onClick={() => { void run(reload); }}>{t('owner.refresh')}</Button>
    {url && <><label htmlFor="owner-link">{t('owner.link')}</label><textarea id="owner-link" readOnly value={url} onFocus={event => event.target.select()} /><Button onClick={() => setUrl('')}>{t('owner.hide')}</Button></>}
    {owners.map(owner => <article key={owner.id}><span>{owner.peer ?? t('owner.pending')}</span><Button disabled={busy} onClick={() => { if (confirm(t('owner.confirm'))) void run(async () => { await repository.request(`/api/owners/${encodeURIComponent(owner.id)}`, 'DELETE'); setUrl(''); await reload(); }); }}>{t('owner.revoke')}</Button></article>)}
  </section>;
}
