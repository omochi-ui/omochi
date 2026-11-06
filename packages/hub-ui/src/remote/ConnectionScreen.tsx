import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import App from '../App.js';
import { DashboardNavigationProvider } from '../DashboardNavigation.js';
import { LanguageSwitcher } from '../LanguageSwitcher.js';
import { ShareSettings } from '../ShareSettings.js';
import { ConfirmationDialog, restoreDialogFocus } from '../dialogs.js';
import { ConnectionControls } from './ConnectionControls.js';
import { GatewayInvitation } from './GatewayInvitation.js';
import { ConnectionError, OwnerInvitation } from './OwnerInvitation.js';
import { OwnerSharingRepository } from './owner-boards.js';
import type { ShareRecord } from './profile.js';
import { useConnections, type ConnectionScreenProps } from './useConnections.js';

function Connections(props: ConnectionScreenProps) {
  const { t } = useTranslation();
  const { state, dispatch, subscribeGateway } = useConnections(props);
  const { active, invitation, browserId, busy, error } = state;
  const fallbackName = active?.kind === 'saved' ? active.name : t('connection.defaultName');
  const connectionName = active?.owner ? fallbackName : active?.view?.board.name || fallbackName;
  const [resetTrigger, setResetTrigger] = useState<HTMLElement | null>(null);
  const [forget, setForget] = useState<{ record: ShareRecord; trigger: HTMLElement } | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    if (!active) document.title = `${t('connection.title')} · omochi`;
  }, [active, t]);

  const controls = <ConnectionControls
    state={state}
    dispatch={dispatch}
    heading={heading}
    onForget={(record, trigger) => setForget({ record, trigger })}
    onReset={setResetTrigger}
  />;
  const notice = !invitation && <ConnectionError error={error} />;

  return <>
    <GatewayInvitation
      subscribeInvitation={subscribeGateway}
      session={active?.owner ? active.session : null}
      ownerScope={active?.owner && browserId ? `${active.id}/${browserId}` : null}
      hubName={active ? connectionName : ''}
      ownsProfile={!!browserId}
      connections={<>
        {!active && notice}
        <ConnectionControls state={state} dispatch={dispatch} onReset={setResetTrigger} />
      </>}
      suspended={!!invitation}
    />
    {active ? <App
      key={active.id}
      environment={{
        session: active.session,
        boards: active.boards,
        authority: active.owner
          ? { kind: 'owner' }
          : { kind: 'share', permissions: active.view?.permissions ?? { interact: false, editLayout: false } },
      }}
      connectionUi={controls}
      connectionNotice={notice}
      connectionName={connectionName}
      renderSharing={active.owner
        ? board => <ShareSettings boardId={board.id} repository={new OwnerSharingRepository(active.session)} />
        : undefined}
    /> : <main className="workspace connections-page">
      <div className="workspace-status"><LanguageSwitcher /></div>
      <h1 tabIndex={-1}>{t('connection.title')}</h1>
      {notice}
      {controls}
    </main>}
    {invitation && <OwnerInvitation
      invitation={invitation}
      busy={busy}
      ownsProfile={!!browserId}
      error={error}
      dispatch={dispatch}
    />}
    {resetTrigger && <ConfirmationDialog
      returnTo={resetTrigger}
      onClose={() => setResetTrigger(null)}
      confirmation={{
        title: t('connection.resetLabel'),
        body: t('connection.resetHelp'),
        action: t('connection.resetConfirm'),
        run: () => { setResetTrigger(null); void dispatch({ type: 'reset' }); },
      }}
    />}
    {forget && <ConfirmationDialog
      returnTo={forget.trigger}
      onClose={() => setForget(null)}
      confirmation={{
        title: t('connection.forgetTitle', { name: forget.record.name }),
        body: t('connection.forgetHelp'),
        action: t('connection.forgetConfirm'),
        run: () => {
          void dispatch({ type: 'forget', id: forget.record.id })
            .finally(() => restoreDialogFocus(heading.current));
        },
      }}
    />}
  </>;
}

export function ConnectionScreen(props: ConnectionScreenProps) {
  return <DashboardNavigationProvider><Connections {...props} /></DashboardNavigationProvider>;
}
