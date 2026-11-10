import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import App from '../App.js';
import { Button } from '../Button.js';
import { DashboardNavigationProvider } from '../DashboardNavigation.js';
import { LanguageSwitcher } from '../LanguageSwitcher.js';
import { ShareSettings } from '../ShareSettings.js';
import { ConfirmationDialog, restoreDialogFocus } from '../dialogs.js';
import { ConnectionControls } from './ConnectionControls.js';
import { CurrentConnectionActions } from './CurrentConnectionActions.js';
import { GatewayInvitation } from './GatewayInvitation.js';
import { ConnectionError, OwnerInvitation } from './OwnerInvitation.js';
import { OwnerSharingRepository } from './owner-boards.js';
import type { ConnectionEntry } from './connection-controller.js';
import {
  useConnections,
  type ConnectionScreenProps,
} from './useConnections.js';

function Connections(props: ConnectionScreenProps) {
  const { t } = useTranslation();
  const { state, dispatch, subscribeGateway } = useConnections(props);
  const { active, invitation, browserId, busy, error } = state;
  const fallbackName =
    active?.kind === 'saved' ? active.name : t('connection.defaultName');
  const connectionName =
    state.entries.find((e) => e.id === active?.id)?.name ||
    (active?.owner ? fallbackName : active?.view?.board.name || fallbackName);
  const [resetTrigger, setResetTrigger] = useState<HTMLElement | null>(null);
  const [forget, setForget] = useState<{
    record: ConnectionEntry;
    trigger: HTMLElement;
  } | null>(null);
  const rememberLocation = useCallback(
    (location: string) => {
      if (active)
        void dispatch({ type: 'rememberLocation', id: active.id, location });
    },
    [active?.id, dispatch],
  );
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      if (!document.querySelector('dialog:modal'))
        document
          .querySelector<HTMLElement>('#workspace, .connections-page h1')
          ?.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [active?.id]);
  useEffect(() => {
    if (!active) document.title = `${t('connection.title')} · omochi`;
  }, [active, t]);

  const controls = (
    <ConnectionControls
      state={state}
      dispatch={dispatch}
      heading={heading}
      showHeading={!!active}
      onForget={(record, trigger) => setForget({ record, trigger })}
    />
  );
  const notice = !invitation && <ConnectionError error={error} />;
  const currentActions = (
    <CurrentConnectionActions state={state} dispatch={dispatch} />
  );
  const identityDetails = (
    <details className="connection-details">
      <summary>{t('connection.details')}</summary>
      {browserId && (
        <dl>
          <dt>{t('connection.browserId')}</dt>
          <dd>
            <code data-browser-id={browserId}>{browserId}</code>
          </dd>
        </dl>
      )}
      <Button
        disabled={busy}
        className="danger"
        onClick={(event) => setResetTrigger(event.currentTarget)}
      >
        {t('connection.reset')}
      </Button>
    </details>
  );

  return (
    <>
      <GatewayInvitation
        subscribeInvitation={subscribeGateway}
        session={active?.owner ? active.session : null}
        ownerScope={
          active?.owner && browserId ? `${active.id}/${browserId}` : null
        }
        hubName={active ? connectionName : ''}
        ownsProfile={state.ownsProfile}
        connections={
          <>
            {!active && notice}
            {active && (
              <section className="invitation-current-connection">
                <p>
                  {t('connection.current')} · {connectionName}
                </p>
                {currentActions}
              </section>
            )}
            <ConnectionControls state={state} dispatch={dispatch} />
            {!active && identityDetails}
          </>
        }
        suspended={!!invitation}
      />
      {active ? (
        <App
          key={active.id}
          environment={{
            session: active.session,
            boards: active.boards,
            authority: active.owner
              ? { kind: 'owner' }
              : {
                  kind: 'share',
                  permissions: active.view?.permissions ?? {
                    interact: false,
                    editLayout: false,
                  },
                },
          }}
          connectionSidebar={controls}
          connectionActions={currentActions}
          onLocationChange={rememberLocation}
          connectionNotice={notice}
          connectionName={connectionName}
          browserIdentity={{
            id: browserId,
            busy,
            error: error ?? state.listError,
            onLoad: () => {
              void dispatch({ type: 'loadProfile' });
            },
            onReset: setResetTrigger,
          }}
          renderSharing={
            active.owner
              ? (board) => (
                  <ShareSettings
                    boardId={board.id}
                    repository={new OwnerSharingRepository(active.session)}
                  />
                )
              : undefined
          }
        />
      ) : (
        <main
          id="workspace"
          tabIndex={-1}
          className="workspace connections-page"
        >
          <div className="workspace-status">
            <LanguageSwitcher />
          </div>
          <h1 tabIndex={-1}>{t('connection.title')}</h1>
          {notice}
          {!!state.entries.length && <p>{t('connection.chooseConnection')}</p>}
          {controls}
          {identityDetails}
        </main>
      )}
      {invitation && (
        <OwnerInvitation
          invitation={invitation}
          busy={busy}
          ownsProfile={state.ownsProfile}
          error={error}
          dispatch={dispatch}
        />
      )}
      {resetTrigger && (
        <ConfirmationDialog
          returnTo={resetTrigger}
          onClose={() => setResetTrigger(null)}
          confirmation={{
            title: t('connection.resetLabel'),
            body: t('connection.resetHelp'),
            action: t('connection.resetConfirm'),
            run: () => {
              setResetTrigger(null);
              void dispatch({ type: 'reset' });
            },
          }}
        />
      )}
      {forget && (
        <ConfirmationDialog
          returnTo={forget.trigger}
          onClose={() => setForget(null)}
          confirmation={{
            title: t('connection.forgetTitle', { name: forget.record.name }),
            body: t('connection.forgetHelp'),
            action: t('connection.forgetConfirm'),
            run: () => {
              void dispatch({ type: 'forget', id: forget.record.id }).finally(
                () => restoreDialogFocus(heading.current),
              );
            },
          }}
        />
      )}
    </>
  );
}

export function ConnectionScreen(props: ConnectionScreenProps) {
  return (
    <DashboardNavigationProvider>
      <Connections {...props} />
    </DashboardNavigationProvider>
  );
}
