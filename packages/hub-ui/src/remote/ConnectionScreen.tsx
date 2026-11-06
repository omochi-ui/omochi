import { LanguageSwitcher } from '../LanguageSwitcher.js';
import { useTranslation } from 'react-i18next';
import { DashboardNavigationProvider, useDashboardNavigation } from '../DashboardNavigation.js';
import { createFragmentInbox } from './fragment.js';
import { GatewayInvitation } from './GatewayInvitation.js';
import { OwnerBoardRepository, OwnerSharingRepository } from './owner-boards.js';
import { ShareSettings } from '../ShareSettings.js';
import type { BoardRepository } from '../board-repository.js';
import { useEffect, useId, useRef, useState } from 'react';
import type { BrowserTransport } from '@omochi-ui/hub-transport';
import { SessionFault } from '@omochi-ui/core/browser';
import App from '../App.js';
import { Button } from '../Button.js';
import { ConfirmationDialog, Modal, restoreDialogFocus } from '../dialogs.js';
import { nativeInvitationLink, removeNativeInvitationButton } from './native-link.js';
import { RemoteHubSession, type SharedView } from './session.js';
import { SharedBoardRepository } from './boards.js';
import { openHubProfile, resetHubProfile, type HubProfile, type ShareRecord } from './profile.js';
import { acceptInvitation, recoverShare } from './enrollment.js';
import { parseShareLink, type ShareLink, type SavedShare } from './link.js';
const noFragments = () => () => {};
function sameLink(a: ShareLink | null, b: ShareLink) {
  return a?.hubId === b.hubId && a.grantId === b.grantId && a.mode === b.mode && a.secret === b.secret && a.relayUrls.join('\n') === b.relayUrls.join('\n');
}
const PUBLIC_CONTEXT = 'omochi.shared.public', SELECTED = 'omochi.shared.selected';
function cache(key: string, value?: string) { try { if (value === undefined) return sessionStorage.getItem(key); if (value) sessionStorage.setItem(key, value); else sessionStorage.removeItem(key); } catch { /* Storage denial does not grant network authority. */ } return null; }
function fixed(error: unknown): SessionFault { return error instanceof SessionFault ? error : new SessionFault('StorageUnavailable'); }
function Connections({ transport, takeLink, bootstrapError, subscribeFragment = noFragments }: { transport: BrowserTransport; takeLink(): ShareLink | null; bootstrapError: SessionFault | null; subscribeFragment?: (receive: (fragment: string) => void) => () => void }) {
  const { t } = useTranslation();
  const [gatewayFragments] = useState(createFragmentInbox);
  const navigation = useDashboardNavigation();
  const [link, setLink] = useState(takeLink), [profile, setProfile] = useState<HubProfile | null>(null), [records, setRecords] = useState<ShareRecord[]>([]);
  const [error, setError] = useState<SessionFault | null>(bootstrapError), [busy, setBusy] = useState(true), [attempt, setAttempt] = useState(0), [confirmReset, setConfirmReset] = useState(false);
  const [live, setLive] = useState<{ source: RemoteHubSession; boards: BoardRepository; owner: boolean; name: string; id: string } | null>(null), [view, setView] = useState<SharedView | null>(null);
  const [proof, setProof] = useState<{ source: SavedShare; view: SharedView } | null>(null);
  const [forgetRecord, setForgetRecord] = useState<ShareRecord | null>(null);
  const [incoming, setIncoming] = useState<{ link: ShareLink } | { error: SessionFault } | null>(null);
  const [liveStatus, setLiveStatus] = useState('Connecting');
  const invitationTrigger = useRef<HTMLElement | null>(document.activeElement as HTMLElement);
  const resetTrigger = useRef<HTMLElement | null>(null);
  const invitationTitle = useId();
  const ownerPrompt = link?.mode === 'owner' || !!proof;
  const currentLink = useRef(link);
  currentLink.current = link;
  const working = useRef(true);
  const forgetTrigger = useRef<HTMLElement | null>(null), connectionsHeading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { if (!live) document.title = `${t('connection.title')} · omochi`; }, [live, t]);
  const state = useRef<{ source: RemoteHubSession | null; profile: HubProfile | null }>({ source: null, profile: null });
  const operation = useRef(new AbortController());
  useEffect(() => { removeNativeInvitationButton(document); }, []);
  const run = async (work: () => Promise<void>) => { if (working.current) return; working.current = true; setBusy(true); setError(null); try { await work(); } catch (error) { if (!operation.current.signal.aborted) setError(fixed(error)); } finally { working.current = false; setBusy(false); } };
  const disconnect = async () => { if (!(await navigation.requestLeave())) return false; await state.current.source?.stop(); state.current.source = null; setLive(null); setView(null); cache(SELECTED, ''); return true; };
  const activate = (source: SavedShare, secretHex: string, publicSecret?: string) => {
    if (operation.current.signal.aborted) return;
    const boards = new SharedBoardRepository();
    const session = new RemoteHubSession({ source, secretHex, publicSecret, owner: source.owner, onView(next) { boards.publish(next); setView(next); } }, transport);
    setLiveStatus('Connecting');
    session.subscribe(event => { if (state.current.source === session && event.kind === 'status') setLiveStatus(event.state.replace(/…$/, '')); });
    boards.bind(session); state.current.source = session; setLive({ source: session, boards: source.owner ? new OwnerBoardRepository(session) : boards, owner: !!source.owner, name: source.name, id: `${source.hubId}/${source.grantId}` });
    if (!publicSecret) cache(SELECTED, `${source.hubId}/${source.grantId}`);
  };
  const connect = async (source: SavedShare, secretHex: string, publicSecret?: string) => {
    if (await disconnect()) activate(source, secretHex, publicSecret);
  };
  const loadProfile = async () => {
    const signal = operation.current.signal;
    const handle = await openHubProfile(transport);
    if (signal.aborted) { await handle.close(); return; }
    state.current.profile = handle; setProfile(handle);
    setRecords(await handle.shares());
    return handle;
  };
  useEffect(() => subscribeFragment(fragment => {
    removeNativeInvitationButton(document);
    if (fragment.startsWith('#g:')) { gatewayFragments.receive(fragment); return; }
    try {
      const next = parseShareLink(fragment);
      // A duplicate must not restart acceptance or erase a local save proof.
      if (!sameLink(currentLink.current, next)) setIncoming({ link: next });
    } catch { setIncoming({ error: new SessionFault('InvalidInvitation') }); }
  }), [subscribeFragment, gatewayFragments]);
  useEffect(() => {
    if (!incoming || busy || proof) return;
    setIncoming(null);
    void run(async () => {
      if ('error' in incoming) throw incoming.error;
      const next = incoming.link;
      if (next.mode === 'owner') {
        invitationTrigger.current = document.activeElement as HTMLElement;
        currentLink.current = next; setLink(next);
        if (!state.current.profile) await loadProfile();
        return;
      }
      if (!(await disconnect()) || operation.current.signal.aborted) return;
      currentLink.current = next; setLink(next);
      cache(PUBLIC_CONTEXT, next.mode === 'share' ? JSON.stringify(next) : '');
      if (next.mode === 'share') {
        const identity = await transport.generateIdentity();
        activate({ ...next, name: t('connection.defaultName') }, identity.secretHex, next.secret);
      }
    });
    // Serializes navigation with acceptance/recovery; the latest link waits for local save proof.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [incoming, busy, proof]);
  useEffect(() => {
    let disposed = false; operation.current = new AbortController();
    working.current = true; setBusy(true);
    const start = async () => {
      if (link?.mode === 'share') {
        cache(PUBLIC_CONTEXT, JSON.stringify(link));
        const identity = await transport.generateIdentity();
        if (!disposed) await connect({ ...link, name: t('connection.defaultName') }, identity.secretHex, link.secret);
      } else {
        cache(PUBLIC_CONTEXT, '');
        const handle = await loadProfile();
        if (disposed || !handle) return;
        const records = await handle.shares(); setRecords(records);
        const selected = cache(SELECTED), saved = records.find(record => record.id === selected && record.state === 'accepted');
        if (saved) await connect(saved, handle.identity.secretHex);
      }
    };
    void start().catch(error => { if (!disposed) setError(fixed(error)); }).finally(() => { if (!disposed) { working.current = false; setBusy(false); } });
    const close = () => {
      disposed = true; operation.current.abort();
      const { source, profile } = state.current;
      void (async () => { await source?.stop(); await profile?.close(); })();
    };
    window.addEventListener('pagehide', close, { once: true });
    return () => { window.removeEventListener('pagehide', close); close(); };
    // Profile/endpoint ownership is restarted only by the explicit recovery action.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [transport, attempt]);
  const saveProof = async (accepted: { source: SavedShare; view: SharedView }) => {
    if (!profile) throw new SessionFault('InvalidState');
    setProof(accepted); await profile.save(accepted.source, 'accepted');
    setRecords(await profile.shares()); await connect(accepted.source, profile.identity.secretHex);
    setProof(null); setLink(null);
  };
  const dismissInvitation = () => {
    if (busy || proof) return;
    currentLink.current = null; setLink(null); setError(null);
  };
  const nativeLink = link ? nativeInvitationLink(`#${link.mode}:${btoa(String.fromCharCode(...new TextEncoder().encode(JSON.stringify(link)))).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '')}`) : null;
  const controls = (inInvitation = false) => <section className="shared-connections" aria-label={t('connection.label')}>
    {error && !ownerPrompt && <p className="notice error" role="alert" data-error-code={error.code}>{t(`sessionError.${error.code}`)}</p>}
    {live && !live.owner && <p>{t('connection.publicHelp')}</p>}
    {profile && <>
      <h2 ref={inInvitation ? undefined : connectionsHeading} tabIndex={-1}>{t('connection.yours')}</h2>
      {!records.length && <p>{t('connection.empty')}</p>}
      {records.map(record => <article className="notice connection-record" key={record.id}>
        <span>{record.name}{record.owner && t('connection.ownerSuffix')}<small>{record.state === 'pending' ? t('connection.pending') : record.owner ? t('connection.ownerAccess') : t('connection.shareAccess')}</small></span>
        <div className="toolbar">
          {live?.id === record.id ? <span className="current-connection">{t('connection.current')}</span> : <Button disabled={busy} onClick={() => { void run(async () => {
            if (record.state === 'pending') await saveProof(await recoverShare(transport, profile, record, operation.current.signal));
            else await connect(record, profile.identity.secretHex);
          }); }}>{record.state === 'pending' ? t('connection.retryAccess') : t('connection.connect')}</Button>}
          {!inInvitation && <Button disabled={busy} onClick={event => { forgetTrigger.current = event.currentTarget; setForgetRecord(record); }}>{t('connection.forget')}</Button>}
        </div>
      </article>)}
    </>}
    <div className="toolbar">
      {live && <Button disabled={busy} onClick={() => { void run(async () => {
        if (!(await disconnect())) return;
        cache(PUBLIC_CONTEXT, ''); setLink(null);
        if (profile) setRecords(await profile.shares()); else setAttempt(v => v + 1);
      }); }}>{t('connection.disconnect')}</Button>}
      {live && liveStatus !== 'Connected' && <Button disabled={busy} onClick={() => { void live.source.retry(); }}>{t('connection.retry')}</Button>}
      {error && !profile && !live && <Button disabled={busy} onClick={() => { setError(null); setAttempt(value => value + 1); }}>{t('connection.retryOpen')}</Button>}
    </div>
    <details className="connection-details">
      <summary>{t('connection.details')}</summary>
      <dl>
        {live && <><dt>{t('connection.hubId')}</dt><dd><code>{live.id.split('/')[0]}</code></dd></>}
        {profile && <><dt>{t('connection.browserId')}</dt><dd><code data-browser-id={profile.identity.endpointId}>{profile.identity.endpointId}</code></dd></>}
      </dl>
      {link?.mode === 'share' && nativeLink && <p><a href={nativeLink}>{t('native.open')}</a></p>}
      {!live && <Button disabled={busy} className="danger" onClick={event => { resetTrigger.current = event.currentTarget; setConfirmReset(true); }}>{t('connection.reset')}</Button>}
    </details>
  </section>;
  return <>
    <GatewayInvitation subscribeInvitation={gatewayFragments.subscribe} session={live?.owner ? live.source : null}
      ownerScope={live?.owner && profile ? `${live.id}/${profile.identity.endpointId}` : null}
      hubName={live?.name ?? ''} ownsProfile={!!profile} connections={controls(true)} suspended={ownerPrompt} />
    {live ? <App key={live.id} environment={{ session: live.source, boards: live.boards, authority: live.owner ? { kind: 'owner' } : { kind: 'share', permissions: view?.permissions ?? { interact: false, editLayout: false } } }}
      connectionUi={controls()} connectionName={live.owner ? live.name : view?.board.name || live.name}
      renderSharing={live.owner ? board => <ShareSettings boardId={board.id} repository={new OwnerSharingRepository(live.source)} /> : undefined} />
      : <main className="workspace connections-page"><LanguageSwitcher /><h1 tabIndex={-1}>{t('connection.title')}</h1>{controls()}</main>}
    {ownerPrompt && <Modal labelledBy={invitationTitle} title={link?.mode === 'owner' || proof?.source.owner ? t('owner.register') : t('connection.acceptTitle')} returnTo={invitationTrigger.current} onClose={dismissInvitation} dismissible={!busy && !proof} className="owner-invitation">
      <p>{t(link?.mode === 'owner' || proof?.source.owner ? 'connection.ownerHelp' : 'connection.inviteHelp')}</p>
      {link && <details className="connection-details"><summary>{t('gateway.details')}</summary><dl><dt>{t('connection.hubId')}</dt><dd><code>{link.hubId}</code></dd></dl></details>}
      {error && <p className="notice error" role="alert" data-error-code={error.code}>{t(`sessionError.${error.code}`)}</p>}
      {proof ? <><p role="status">{t('connection.saveFailed')}</p><Button className="primary" disabled={busy} onClick={() => { void run(() => saveProof(proof)); }}>{t('connection.retrySave')}</Button></>
        : <div className="dialog-actions">
          <Button className="primary" disabled={busy || !profile} onClick={() => { void run(async () => {
            if (!profile || !link || !(await disconnect())) return;
            cache(PUBLIC_CONTEXT, '');
            const accepted = await acceptInvitation(transport, profile, link, operation.current.signal);
            await saveProof(accepted);
          }); }}>{t('connection.accept')}</Button>
          {error && !profile && <Button disabled={busy} onClick={() => { void run(async () => { await loadProfile(); }); }}>{t('connection.retryOpen')}</Button>}
          {nativeLink && !busy && <a className="hub-button" href={nativeLink}>{t('native.open')}</a>}
          <Button disabled={busy} onClick={dismissInvitation}>{t('connection.dismiss')}</Button>
        </div>}
    </Modal>}
    {confirmReset && <ConfirmationDialog returnTo={resetTrigger.current} onClose={() => setConfirmReset(false)} confirmation={{
      title: t('connection.resetLabel'), body: t('connection.resetHelp'), action: t('connection.resetConfirm'),
      run: () => { setConfirmReset(false); void run(async () => {
        if (!(await disconnect())) return;
        await profile?.close(); state.current.profile = null; setProfile(null);
        await resetHubProfile(); setProof(null); setRecords([]); setAttempt(v => v + 1);
      }); },
    }} />}
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
