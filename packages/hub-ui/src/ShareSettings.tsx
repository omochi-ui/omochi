import { useEffect, useRef, useState } from 'react';
import { Button } from './Button.js';
interface Grant { id: string; boardId: string; public: boolean; peer: string | null; invitationExpiresAt: number; permissions: { interact: boolean; editLayout: boolean } }
interface Host { hubId?: string; relayUrls?: string[]; enabled?: boolean }
async function request(path: string, method = 'GET', body?: unknown): Promise<unknown> {
  const response = await fetch(path, { method, headers: { 'Content-Type': 'application/json' }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
  if (!response.ok) { await response.body?.cancel(); throw new Error(method === 'DELETE' ? 'Could not confirm revocation. The share may already be removed. Refresh the list before retrying.' : 'Could not update sharing. Check the hub and try again.'); }
  return response.json();
}
/** Owner-only UI. Shared clients are not given an owner repository or this entry. */
export function ShareSettings({ boardId }: { boardId: string }) {
  const dialog = useRef<HTMLDialogElement>(null), linkText = useRef<HTMLTextAreaElement>(null);
  const [open, setOpen] = useState(false), [busy, setBusy] = useState(false), [error, setError] = useState('');
  const [host, setHost] = useState<Host | null>(null), [grants, setGrants] = useState<Grant[]>([]);
  const [mode, setMode] = useState<'invite' | 'public'>('invite'), [interact, setInteract] = useState(false), [editLayout, setEditLayout] = useState(false);
  const [link, setLink] = useState<{ id: string; url: string } | null>(null), [confirm, setConfirm] = useState<string | null>(null), [message, setMessage] = useState('');
  const refresh = async () => {
    const [info, records] = await Promise.all([request('/api/sharing'), request('/api/shares')]);
    if (!info || typeof info !== 'object' || !Array.isArray(records)) throw new Error('Sharing information is unavailable.');
    setHost(info as Host); setGrants((records as Grant[]).filter(grant => grant.boardId === boardId));
  };
  const run = async (operation: () => Promise<void>) => { setBusy(true); setError(''); try { await operation(); } catch (error) { setError(error instanceof Error ? error.message : 'Sharing is unavailable.'); } finally { setBusy(false); } };
  useEffect(() => { if (open) dialog.current?.showModal(); else dialog.current?.close(); }, [open]);
  const ready = !!host?.hubId && !!host.relayUrls?.length;
  return <>
    <Button disabled={busy} onClick={() => { setOpen(true); setLink(null); setConfirm(null); setMessage(''); void run(refresh); }}>Share dashboard</Button>
    <dialog ref={dialog} className="share-dialog" aria-labelledby="share-heading" onClose={() => { setOpen(false); setLink(null); }} onCancel={event => { if (busy) event.preventDefault(); }}>
      <h2 id="share-heading">Share dashboard</h2>
      <p>Recipients see this dashboard’s saved views and live app data. Owner changes to the views and layout are reflected in every share.</p>
      {error && <p role="alert">{error}</p>}
      <p role="status">{message}</p>
      {!ready && <p role="status">{host?.enabled === false ? 'Enable remote sharing on your hub with an HTTPS dashboard host to create links.' : 'Waiting for the hub’s relay address. Refresh when the connection is ready.'}</p>}
      <fieldset disabled={busy || !ready}>
        <legend>Create a share</legend>
        <label>Access type <select aria-label="Access type" value={mode} onChange={e => setMode(e.target.value as 'invite' | 'public')}><option value="invite">Invite one browser</option><option value="public">Anyone with the link · readonly</option></select></label>
        {mode === 'invite' ? <>
          <p>The first browser to accept is registered. Send a separate invitation to each recipient. Invitations expire after 3 minutes; accepted access lasts until revoked.</p>
          <label><input type="checkbox" checked={interact} onChange={e => setInteract(e.target.checked)} /> Allow app interaction</label>
          <label><input type="checkbox" checked={editLayout} onChange={e => setEditLayout(e.target.checked)} /> Allow editing names, positions and sizes</label>
          <p>Recipients cannot add or remove shared views, change their sources, or manage sharing. App actions can change application data.</p>
        </> : <p>Anyone who receives or is forwarded this link can view the full state and published images of these views. They cannot operate apps or edit the board.</p>}
        <Button className="primary" onClick={() => { void run(async () => {
          const result = await request('/api/shares', 'POST', { boardId, public: mode === 'public', permissions: { interact: mode === 'invite' && interact, editLayout: mode === 'invite' && editLayout } }) as { grant: Grant; url?: string };
          if (typeof result.url !== 'string' || !result.url.startsWith('https:')) { await refresh(); throw new Error('Share created, but the relay address is not ready. Revoke it and create a new link when ready.'); }
          setLink({ id: result.grant.id, url: result.url }); setMessage('Link created. Copy it now; this hub does not retain the link secret.'); await refresh();
        }); }}>Create sharing link</Button>
      </fieldset>
      {link && <section><label htmlFor="sharing-link">Sharing link</label><textarea ref={linkText} id="sharing-link" aria-label="Sharing link" readOnly rows={3} value={link.url} onFocus={e => e.target.select()} /><Button onClick={() => { void run(async () => { if (navigator.clipboard) { await navigator.clipboard.writeText(link.url); setMessage('Link copied.'); } else { linkText.current?.focus(); linkText.current?.select(); setMessage('Link selected. Copy the selected text.'); } }); }}>Copy link</Button></section>}
      <h3>Existing shares</h3>
      {!grants.length && <p>No shares for this dashboard.</p>}
      {grants.map(grant => <article className="notice" key={grant.id} data-share-id={grant.id}>
        <span>{grant.public ? 'Public link · readonly' : grant.peer ? 'Registered browser' : 'Pending invitation'}{!grant.public && <small>{grant.peer ?? (grant.invitationExpiresAt * 1000 <= Date.now() ? 'Invitation expired' : 'Waiting for acceptance')}</small>}{grant.permissions.interact && <small>App interaction allowed</small>}{grant.permissions.editLayout && <small>Layout editing allowed</small>}</span>
        <Button className="danger" disabled={busy} onClick={() => setConfirm(grant.id)}>Revoke access…</Button>
      </article>)}
      {confirm && <section role="group" aria-label="Confirm revocation"><p>Revoke this share? Connected recipients will lose access. Data already received cannot be recalled.</p><Button className="danger" disabled={busy} onClick={() => { void run(async () => { await request(`/api/shares/${encodeURIComponent(confirm)}`, 'DELETE'); if (link?.id === confirm) setLink(null); setConfirm(null); setMessage('Access revoked.'); await refresh(); }); }}>Confirm revoke</Button><Button disabled={busy} onClick={() => setConfirm(null)}>Cancel revocation</Button></section>}
      <div className="dialog-actions"><Button disabled={busy} onClick={() => { void run(refresh); }}>Refresh shares</Button><Button disabled={busy} onClick={() => setOpen(false)}>Close sharing</Button></div>
    </dialog>
  </>;
}
