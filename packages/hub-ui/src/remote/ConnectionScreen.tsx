import { GatewayInvitation } from './GatewayInvitation.js';
import { OwnerBoardRepository, OwnerSharingRepository } from './owner-boards.js';
import { ShareSettings } from '../ShareSettings.js';
import type { BoardRepository } from '../board-repository.js';
import { useEffect, useRef, useState } from 'react';
import type { BrowserTransport } from '@omochi-ui/hub-transport';
import { SessionFault } from '@omochi-ui/core/browser';
import App from '../App.js';
import { Button } from '../Button.js';
import { RemoteHubSession, type SharedView } from './session.js';
import { SharedBoardRepository } from './boards.js';
import { openHubProfile, resetHubProfile, type HubProfile, type ShareRecord } from './profile.js';
import { acceptInvitation, recoverShare } from './enrollment.js';
import type { ShareLink, SavedShare } from './link.js';
const noGateway = () => null;
const PUBLIC_CONTEXT = 'omochi.shared.public', SELECTED = 'omochi.shared.selected';
function cache(key: string, value?: string) { try { if (value === undefined) return sessionStorage.getItem(key); if (value) sessionStorage.setItem(key, value); else sessionStorage.removeItem(key); } catch { /* Storage denial does not grant network authority. */ } return null; }
function fixed(error: unknown): SessionFault { return error instanceof SessionFault ? error : new SessionFault('StorageUnavailable'); }
export function ConnectionScreen({ transport, takeLink, bootstrapError, takeGateway = noGateway }: { transport: BrowserTransport; takeLink(): ShareLink | null; bootstrapError: SessionFault | null; takeGateway?: () => string | null }) {
  const [link, setLink] = useState(takeLink), [profile, setProfile] = useState<HubProfile | null>(null), [records, setRecords] = useState<ShareRecord[]>([]);
  const [error, setError] = useState<SessionFault | null>(bootstrapError), [busy, setBusy] = useState(false), [attempt, setAttempt] = useState(0), [confirmReset, setConfirmReset] = useState(false);
  const [live, setLive] = useState<{ source: RemoteHubSession; boards: BoardRepository; owner: boolean; name: string; id: string } | null>(null), [view, setView] = useState<SharedView | null>(null);
  const [proof, setProof] = useState<{ source: SavedShare; view: SharedView } | null>(null);
  const state = useRef<{ source: RemoteHubSession | null; profile: HubProfile | null }>({ source: null, profile: null });
  const operation = useRef(new AbortController());
  const run = async (work: () => Promise<void>) => { setBusy(true); setError(null); try { await work(); } catch (error) { if (!operation.current.signal.aborted) setError(fixed(error)); } finally { setBusy(false); } };
  const disconnect = async () => { await state.current.source?.stop(); state.current.source = null; setLive(null); setView(null); cache(SELECTED, ''); };
  const connect = async (source: SavedShare, secretHex: string, publicSecret?: string) => {
    if (live && Array.from(document.querySelectorAll('.subtitle')).some(e => e.textContent?.includes('Unsaved changes')) && !confirm('Disconnect this hub and discard unsaved dashboard edits?')) return;
    await disconnect(); if (operation.current.signal.aborted) return;
    const boards = new SharedBoardRepository();
    const session = new RemoteHubSession({ source, secretHex, publicSecret, owner: source.owner, onView(next) { boards.publish(next); setView(next); } }, transport);
    boards.bind(session); state.current.source = session; setLive({ source: session, boards: source.owner ? new OwnerBoardRepository(session) : boards, owner: !!source.owner, name: source.name, id: `${source.hubId}/${source.grantId}` });
    if (!publicSecret) cache(SELECTED, `${source.hubId}/${source.grantId}`);
  };
  useEffect(() => {
    let disposed = false; operation.current = new AbortController();
    let handle: HubProfile | null = null;
    const start = async () => {
      if (link?.mode === 'public') {
        cache(PUBLIC_CONTEXT, JSON.stringify(link));
        const identity = await transport.generateIdentity();
        if (!disposed) await connect({ ...link, name: 'Shared dashboard' }, identity.secretHex, link.secret);
      } else {
        cache(PUBLIC_CONTEXT, '');
        handle = await openHubProfile(transport);
        if (disposed) { await handle.close(); return; }
        state.current.profile = handle; setProfile(handle);
        const records = await handle.shares(); setRecords(records);
        const selected = cache(SELECTED), saved = !link && records.find(record => record.id === selected && record.state === 'accepted');
        if (saved) await connect(saved, handle.identity.secretHex);
      }
    };
    void start().catch(error => { if (!disposed) setError(fixed(error)); });
    const close = () => { disposed = true; operation.current.abort(); void (async () => { await state.current.source?.stop(); await handle?.close(); })(); };
    window.addEventListener('pagehide', close, { once: true });
    return () => { window.removeEventListener('pagehide', close); close(); };
    // Profile/endpoint ownership is restarted only by the explicit recovery action.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [transport, attempt]);
  const saveProof = async (accepted: { source: SavedShare; view: SharedView }) => {
    if (!profile) throw new SessionFault('InvalidState');
    setProof(accepted); await profile.save(accepted.source, 'accepted'); setProof(null); setLink(null);
    setRecords(await profile.shares()); await connect(accepted.source, profile.identity.secretHex);
  };
  const controls = <section className="shared-connections" aria-label="Shared dashboard connection">
    {error && <p role="alert" data-error-code={error.code}>{error.message}</p>}
    {profile && <p>Browser ID: <code data-browser-id={profile.identity.endpointId}>{profile.identity.endpointId}</code></p>}
    {(link?.mode === 'invite' || link?.mode === 'owner') && !proof && <section><h2>{link.mode === 'owner' ? 'Register owner browser' : 'Accept dashboard invitation'}</h2>{link.mode === 'owner' && <p>This grants control of all dashboards, apps and sharing on this hub.</p>}{link.mode !== 'owner' && <p>This invitation registers this browser to one shared dashboard. The owner chooses whether you may operate apps or edit names and layout. App actions can change application data.</p>}<Button className="primary" disabled={busy || !profile} onClick={() => { void run(async () => { if (!profile) return; await disconnect(); const accepted = await acceptInvitation(transport, profile, link, operation.current.signal); await saveProof(accepted); }); }}>Accept invitation</Button></section>}
    {proof && <section><p role="status">Access was confirmed, but saving this connection failed. Retry saving with the same browser identity; no new invitation is needed.</p><Button disabled={busy} onClick={() => { void run(() => saveProof(proof)); }}>Retry connection save</Button></section>}
    {link?.mode === 'public' && <p>Anyone with this link can view this dashboard. App interaction and layout editing are disabled.</p>}
    {profile && !link && !proof && <><h2>Your hub connections</h2>{!records.length && <p>Open a dashboard invitation from its owner to get started.</p>}{records.map(record => <article className="notice" key={record.id}><span>{record.name}{record.owner && ' · Owner'}<small>{record.state === 'pending' ? 'Enrollment needs confirmation. Retry existing access, or request a new invitation.' : record.owner ? 'Registered owner access' : 'Registered shared access'}</small></span><Button disabled={busy} onClick={() => { void run(async () => { if (record.state === 'pending') await saveProof(await recoverShare(transport, profile, record, operation.current.signal)); else await connect(record, profile.identity.secretHex); }); }}>{record.state === 'pending' ? 'Retry existing access' : 'Connect'}</Button><Button disabled={busy} onClick={() => { void run(async () => { await profile.forget(record.id); setRecords(await profile.shares()); }); }}>Forget local connection</Button></article>)}</>}
    {live && <Button disabled={busy} onClick={() => { void run(async () => { if (Array.from(document.querySelectorAll('.subtitle')).some(e => e.textContent?.includes('Unsaved changes')) && !confirm('Discard unsaved dashboard edits and disconnect?')) return; await disconnect(); cache(PUBLIC_CONTEXT, ''); setLink(null); if (profile) setRecords(await profile.shares()); else setAttempt(v => v + 1); }); }}>Disconnect shared dashboard</Button>}
    {live && <Button disabled={busy} onClick={() => { void live.source.retry(); }}>Retry hub connection</Button>}
    {error && !profile && !live && <Button disabled={busy} onClick={() => { setError(null); setAttempt(value => value + 1); }}>Retry opening connection</Button>}
    {!live && <Button disabled={busy} className="danger" onClick={() => setConfirmReset(true)}>Reset browser identity…</Button>}
    {confirmReset && <section role="group" aria-label="Confirm browser identity reset"><p>Reset this browser identity and saved connections? Boards stay on their hubs. Owners must revoke the previous browser’s owner and share grants separately; resetting does not revoke them.</p><Button className="danger" disabled={busy} onClick={() => { void run(async () => { await disconnect(); await profile?.close(); state.current.profile = null; setProfile(null); await resetHubProfile(); setProof(null); setRecords([]); setConfirmReset(false); setAttempt(v => v + 1); }); }}>Confirm identity reset</Button><Button disabled={busy} onClick={() => setConfirmReset(false)}>Cancel reset</Button></section>}
  </section>;
  return <><GatewayInvitation takeInvitation={takeGateway} session={live?.owner ? live.source : null} hubName={live ? `${live.name} (${live.id.split("/")[0]})` : ""} ownsProfile={!!profile} />{live ? <App key={live.id} environment={{ session: live.source, boards: live.boards, authority: live.owner ? { kind: 'owner' } : { kind: 'share', permissions: view?.permissions ?? { interact: false, editLayout: false } } }} connectionUi={controls} renderSharing={live.owner ? board => <ShareSettings boardId={board.id} repository={new OwnerSharingRepository(live.source)} /> : undefined} /> : <main className="workspace"><h1>Shared dashboards</h1>{controls}</main>}</>;
}
