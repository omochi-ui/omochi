import { LanguageSwitcher } from '../LanguageSwitcher.js';
import { useTranslation } from 'react-i18next';
import { DashboardNavigationProvider, useDashboardNavigation } from '../DashboardNavigation.js';
import { GatewayInvitation } from './GatewayInvitation.js';
import { OwnerBoardRepository, OwnerSharingRepository } from './owner-boards.js';
import { ShareSettings } from '../ShareSettings.js';
import type { BoardRepository } from '../board-repository.js';
import { useEffect, useRef, useState } from 'react';
import type { BrowserTransport } from '@omochi-ui/hub-transport';
import { SessionFault } from '@omochi-ui/core/browser';
import App from '../App.js';
import { Button } from '../Button.js';
import { ConfirmationDialog, restoreDialogFocus } from '../dialogs.js';
import { RemoteHubSession, type SharedView } from './session.js';
import { SharedBoardRepository } from './boards.js';
import { openHubProfile, resetHubProfile, type HubProfile, type ShareRecord } from './profile.js';
import { acceptInvitation, recoverShare } from './enrollment.js';
import type { ShareLink, SavedShare } from './link.js';
const noGateway = () => null;
const PUBLIC_CONTEXT = 'omochi.shared.public', SELECTED = 'omochi.shared.selected';
function cache(key: string, value?: string) { try { if (value === undefined) return sessionStorage.getItem(key); if (value) sessionStorage.setItem(key, value); else sessionStorage.removeItem(key); } catch { /* Storage denial does not grant network authority. */ } return null; }
function fixed(error: unknown): SessionFault { return error instanceof SessionFault ? error : new SessionFault('StorageUnavailable'); }
function Connections({ transport, takeLink, bootstrapError, takeGateway = noGateway }: { transport: BrowserTransport; takeLink(): ShareLink | null; bootstrapError: SessionFault | null; takeGateway?: () => string | null }) {
  const { t } = useTranslation();
  const navigation = useDashboardNavigation();
  const [link, setLink] = useState(takeLink), [profile, setProfile] = useState<HubProfile | null>(null), [records, setRecords] = useState<ShareRecord[]>([]);
  const [error, setError] = useState<SessionFault | null>(bootstrapError), [busy, setBusy] = useState(false), [attempt, setAttempt] = useState(0), [confirmReset, setConfirmReset] = useState(false);
  const [live, setLive] = useState<{ source: RemoteHubSession; boards: BoardRepository; owner: boolean; name: string; id: string } | null>(null), [view, setView] = useState<SharedView | null>(null);
  const [proof, setProof] = useState<{ source: SavedShare; view: SharedView } | null>(null);
  const [forgetRecord, setForgetRecord] = useState<ShareRecord | null>(null);
  const forgetTrigger = useRef<HTMLElement | null>(null), connectionsHeading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { if (!live) document.title = `${t('connection.title')} · omochi`; }, [live, t]);
  const state = useRef<{ source: RemoteHubSession | null; profile: HubProfile | null }>({ source: null, profile: null });
  const operation = useRef(new AbortController());
  const run = async (work: () => Promise<void>) => { setBusy(true); setError(null); try { await work(); } catch (error) { if (!operation.current.signal.aborted) setError(fixed(error)); } finally { setBusy(false); } };
  const disconnect = async () => { if (!(await navigation.requestLeave())) return false; await state.current.source?.stop(); state.current.source = null; setLive(null); setView(null); cache(SELECTED, ''); return true; };
  const connect = async (source: SavedShare, secretHex: string, publicSecret?: string) => {
    if (!(await disconnect())) return; if (operation.current.signal.aborted) return;
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
        if (!disposed) await connect({ ...link, name: t('connection.defaultName') }, identity.secretHex, link.secret);
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
  const controls = <section className="shared-connections" aria-label={t('connection.label')}>
    {error && <p role="alert" data-error-code={error.code}>{t(`sessionError.${error.code}`)}</p>}
    {profile && <p>{t('connection.browserId')} <code data-browser-id={profile.identity.endpointId}>{profile.identity.endpointId}</code></p>}
    {(link?.mode === 'invite' || link?.mode === 'owner') && !proof && <section><h2>{link.mode === 'owner' ? t('owner.register') : t('connection.acceptTitle')}</h2>{link.mode === 'owner' && <p>{t('connection.ownerHelp')}</p>}{link.mode !== 'owner' && <p>{t('connection.inviteHelp')}</p>}<Button className="primary" disabled={busy || !profile} onClick={() => { void run(async () => { if (!profile) return; if (!(await disconnect())) return; const accepted = await acceptInvitation(transport, profile, link, operation.current.signal); await saveProof(accepted); }); }}>{t('connection.accept')}</Button></section>}
    {proof && <section><p role="status">{t('connection.saveFailed')}</p><Button disabled={busy} onClick={() => { void run(() => saveProof(proof)); }}>{t('connection.retrySave')}</Button></section>}
    {link?.mode === 'public' && <p>{t('connection.publicHelp')}</p>}
    {profile && !link && !proof && <><h2 ref={connectionsHeading} tabIndex={-1}>{t('connection.yours')}</h2>{!records.length && <p>{t('connection.empty')}</p>}{records.map(record => <article className="notice" key={record.id}><span>{record.name}{record.owner && t('connection.ownerSuffix')}<small>{record.state === 'pending' ? t('connection.pending') : record.owner ? t('connection.ownerAccess') : t('connection.shareAccess')}</small></span><Button disabled={busy} onClick={() => { void run(async () => { if (record.state === 'pending') await saveProof(await recoverShare(transport, profile, record, operation.current.signal)); else await connect(record, profile.identity.secretHex); }); }}>{record.state === 'pending' ? t('connection.retryAccess') : t('connection.connect')}</Button><Button disabled={busy} onClick={event => { forgetTrigger.current = event.currentTarget; setForgetRecord(record); }}>{t('connection.forget')}</Button></article>)}</>}
    {live && <Button disabled={busy} onClick={() => { void run(async () => { if (!(await disconnect())) return; cache(PUBLIC_CONTEXT, ''); setLink(null); if (profile) setRecords(await profile.shares()); else setAttempt(v => v + 1); }); }}>{t('connection.disconnect')}</Button>}
    {live && <Button disabled={busy} onClick={() => { void live.source.retry(); }}>{t('connection.retry')}</Button>}
    {error && !profile && !live && <Button disabled={busy} onClick={() => { setError(null); setAttempt(value => value + 1); }}>{t('connection.retryOpen')}</Button>}
    {!live && <Button disabled={busy} className="danger" onClick={() => setConfirmReset(true)}>{t('connection.reset')}</Button>}
    {confirmReset && <section role="group" aria-label={t('connection.resetLabel')}><p>{t('connection.resetHelp')}</p><Button className="danger" disabled={busy} onClick={() => { void run(async () => { if (!(await disconnect())) return; await profile?.close(); state.current.profile = null; setProfile(null); await resetHubProfile(); setProof(null); setRecords([]); setConfirmReset(false); setAttempt(v => v + 1); }); }}>{t('connection.resetConfirm')}</Button><Button disabled={busy} onClick={() => setConfirmReset(false)}>{t('connection.resetCancel')}</Button></section>}
  </section>;
  return <><GatewayInvitation takeInvitation={takeGateway} session={live?.owner ? live.source : null} hubName={live ? `${live.name} (${live.id.split("/")[0]})` : ""} ownsProfile={!!profile} />{live ? <App key={live.id} environment={{ session: live.source, boards: live.boards, authority: live.owner ? { kind: 'owner' } : { kind: 'share', permissions: view?.permissions ?? { interact: false, editLayout: false } } }} connectionUi={controls} renderSharing={live.owner ? board => <ShareSettings boardId={board.id} repository={new OwnerSharingRepository(live.source)} /> : undefined} /> : <main className="workspace"><LanguageSwitcher /><h1>{t('connection.title')}</h1>{controls}</main>}
    {forgetRecord && <ConfirmationDialog returnTo={forgetTrigger.current} onClose={() => setForgetRecord(null)} confirmation={{
      title: t('connection.forgetTitle', { name: forgetRecord.name }),
      body: t('connection.forgetHelp'),
      action: t('connection.forgetConfirm'),
      run: () => { void run(async () => {
        if (!profile) throw new SessionFault('InvalidState');
        await profile.forget(forgetRecord.id);
        if (cache(SELECTED) === forgetRecord.id) cache(SELECTED, '');
        setRecords(await profile.shares());
      }).finally(() => restoreDialogFocus(connectionsHeading.current)); },
    }} />}
  </>;
}

export function ConnectionScreen(props: Parameters<typeof Connections>[0]) {
  return <DashboardNavigationProvider><Connections {...props} /></DashboardNavigationProvider>;
}
