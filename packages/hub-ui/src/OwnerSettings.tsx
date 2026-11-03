import { useState } from 'react';
import { Button } from './Button.js';
import { HttpSharingRepository } from './sharing-repository.js';
interface Owner { id: string; peer: string | null; invitationExpiresAt: number }
export function OwnerSettings() {
  const [owners, setOwners] = useState<Owner[]>([]), [url, setUrl] = useState(''), [error, setError] = useState(''), [busy, setBusy] = useState(false);
  const repository = new HttpSharingRepository();
  const reload = async () => setOwners(await repository.request('/api/owners') as Owner[]);
  const run = async (operation: () => Promise<void>) => { setBusy(true); setError(''); try { await operation(); } catch { setError('Could not confirm owner access changes. Refresh and retry.'); } finally { setBusy(false); } };
  return <section aria-label="Owner browser access"><h2>Owner browser access</h2><p>Registered browsers may manage all dashboards, operate apps and manage sharing. Only this local screen can register or revoke owners.</p>
    {error && <p role="alert">{error}</p>}
    <Button disabled={busy} onClick={() => { void run(async () => { const result = await repository.request('/api/owners', 'POST') as { url: string }; setUrl(result.url); await reload(); }); }}>Register owner browser</Button>
    <Button disabled={busy} onClick={() => { void run(reload); }}>Refresh owners</Button>
    {url && <><label htmlFor="owner-link">Owner registration link · expires in 3 minutes</label><textarea id="owner-link" readOnly value={url} onFocus={event => event.target.select()} /><Button onClick={() => setUrl('')}>Hide owner link</Button></>}
    {owners.map(owner => <article key={owner.id}><span>{owner.peer ?? 'Pending owner invitation'}</span><Button disabled={busy} onClick={() => { if (confirm('Revoke this owner browser or pending invitation?')) void run(async () => { await repository.request(`/api/owners/${encodeURIComponent(owner.id)}`, 'DELETE'); setUrl(''); await reload(); }); }}>Revoke owner access</Button></article>)}
  </section>;
}
