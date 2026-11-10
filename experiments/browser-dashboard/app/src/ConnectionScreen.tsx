import '../../../../packages/hub-ui/src/i18n.js';
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  useSyncExternalStore,
} from 'react';
import {
  openBrowserProfile,
  parseInvitation,
  type ProfileOwner,
  type ProfileRecovery,
} from '@omochi-ui/browser-hub';
import type { BrowserTransport } from '@omochi-ui/browser-transport';
import { SessionFault, type SessionError } from '@omochi-ui/core/browser';
import App from '../../../../packages/hub-ui/src/App.js';
import { DashboardNavigationProvider } from '../../../../packages/hub-ui/src/DashboardNavigation.js';
import { createDisconnectedSession } from './disconnected-session.js';
import type { BoardRepository } from '../../../../packages/hub-ui/src/board-repository.js';

function fixed(error: unknown): SessionError {
  return error instanceof SessionFault
    ? error
    : new SessionFault('StorageUnavailable');
}
export function ConnectionScreen({
  transport,
  boards,
  takeInvitation,
  bootstrapError,
}: {
  transport: BrowserTransport;
  boards: BoardRepository;
  takeInvitation(): string | null;
  bootstrapError: SessionError | null;
}) {
  const [profile, setProfile] = useState<ProfileOwner | ProfileRecovery | null>(
    null,
  );
  const [error, setError] = useState<SessionError | null>(bootstrapError);
  const [attempt, setAttempt] = useState(0);
  const owner = profile?.kind === 'owner' ? profile : null;
  const subscribe = useCallback(
    (changed: () => void) => owner?.subscribe(changed) ?? (() => {}),
    [owner],
  );
  const view = useCallback(() => owner?.view() ?? null, [owner]);
  const snapshot = useSyncExternalStore(subscribe, view, view);
  const selectedSource = snapshot ? owner!.session() : null;
  const source = useMemo(
    () => selectedSource ?? createDisconnectedSession(),
    [selectedSource],
  );
  useEffect(() => {
    let disposed = false;
    let handle: ProfileOwner | ProfileRecovery | null = null;
    void openBrowserProfile(transport).then(
      async (opened) => {
        handle = opened;
        if (disposed) await opened.close();
        else setProfile(opened);
      },
      (cause) => {
        if (!disposed) setError(fixed(cause));
      },
    );
    const close = () => {
      disposed = true;
      void handle?.close().catch(() => {});
    };
    window.addEventListener('pagehide', close, { once: true });
    return () => {
      window.removeEventListener('pagehide', close);
      close();
    };
  }, [transport, attempt]);
  const reopen = () => {
    setProfile(null);
    setError(null);
    setAttempt((value) => value + 1);
  };
  return (
    <DashboardNavigationProvider>
      <App
        environment={{ authority: { kind: 'owner' }, session: source, boards }}
        connectionUi={
          <div className="connections-page">
            <header>
              <p className="connection-eyebrow">Omochi · browser preview</p>
              <h2>Your application connections</h2>
              <p>
                Connect directly to your gateways. This preview uses TLS relays
                without an uptime guarantee.
              </p>
            </header>
            {error && (
              <p role="alert" data-error-code={error.code}>
                {error.message}
              </p>
            )}
            {!profile && (
              <>
                <p role="status">
                  {error
                    ? 'No browser connection is open.'
                    : 'Opening your browser profile…'}
                </p>
                {error && (
                  <button className="hub-button" onClick={reopen}>
                    Retry opening profile
                  </button>
                )}
              </>
            )}
            {profile?.kind === 'recovery' && (
              <RecoveryScreen profile={profile} reopen={reopen} />
            )}
            {profile?.kind === 'owner' && (
              <OwnerScreen
                owner={profile}
                takeInvitation={takeInvitation}
                reopen={reopen}
              />
            )}
          </div>
        }
      />
    </DashboardNavigationProvider>
  );
}

function RecoveryScreen({
  profile,
  reopen,
}: {
  profile: ProfileRecovery;
  reopen(): void;
}) {
  const [confirm, setConfirm] = useState(false),
    [error, setError] = useState<SessionError | null>(null),
    [busy, setBusy] = useState(false);
  return (
    <section className="connection-card">
      <h2>Saved profile needs recovery</h2>
      <p role="alert">{profile.error.message}</p>
      <p>
        Previous browser ID:{' '}
        <code>{profile.storedEndpointId ?? 'unknown'}</code>
      </p>
      <p>
        Resetting removes this browser’s identity and saved connections. Your
        boards remain. Gateway access is not revoked remotely; remove the
        previous browser ID with <code>trust forget</code> on each gateway.
      </p>
      {error && (
        <p role="alert" data-error-code={error.code}>
          {error.message}
        </p>
      )}
      {!confirm ? (
        <button className="hub-button danger" onClick={() => setConfirm(true)}>
          Reset browser identity…
        </button>
      ) : (
        <div className="connection-actions">
          <button
            className="hub-button danger"
            disabled={busy}
            onClick={() => {
              setBusy(true);
              void profile.resetIdentity().then(reopen, (cause) => {
                setError(fixed(cause));
                setBusy(false);
              });
            }}
          >
            Confirm identity reset
          </button>
          <button
            className="hub-button"
            disabled={busy}
            onClick={() => setConfirm(false)}
          >
            Cancel
          </button>
        </div>
      )}
    </section>
  );
}

function OwnerScreen({
  owner,
  takeInvitation,
  reopen,
}: {
  owner: ProfileOwner;
  takeInvitation(): string | null;
  reopen(): void;
}) {
  const subscribe = useCallback(
    (changed: () => void) => owner.subscribe(changed),
    [owner],
  );
  const getView = useCallback(() => owner.view(), [owner]);
  const view = useSyncExternalStore(subscribe, getView, getView);
  const [text, setText] = useState(() => takeInvitation() ?? ''),
    [error, setError] = useState<SessionError | null>(null);
  const [busy, setBusy] = useState(false),
    [confirmation, setConfirmation] = useState<string | null>(null);
  const source = owner.session();
  const run = (work: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    void work()
      .catch((cause) => setError(fixed(cause)))
      .finally(() => setBusy(false));
  };
  const accept = () => {
    let nodeId: string;
    try {
      nodeId = parseInvitation(text).nodeId;
    } catch (cause) {
      setError(fixed(cause));
      return;
    }
    const raw = text;
    setText('');
    const accepted = view.gateways.some(
      (gateway) =>
        gateway.record.nodeId === nodeId &&
        gateway.record.enrollment === 'accepted',
    );
    run(() => (accepted ? owner.refreshInvitation(raw) : owner.pair(raw)));
  };
  const unsaved = view.gateways.some(
    (gateway) => gateway.recovery === 'unsaved',
  );
  return (
    <>
      <p className="connection-identity">
        Browser ID:{' '}
        <code data-browser-id={view.endpointId}>{view.endpointId}</code>
      </p>
      {view.closingPending && (
        <p role="status">
          Closing pending. Waiting for the current connection to finish before
          another can open.
        </p>
      )}
      {error && (
        <p role="alert" data-error-code={error.code}>
          {error.message}
        </p>
      )}
      <section className="connection-card">
        <h2>Add or refresh a gateway</h2>
        <p>
          Accepting an invitation grants this browser access to all services,
          actions and resources exposed by that gateway. A board does not limit
          that access. This site’s code handles your browser key and application
          data.
        </p>
        <label htmlFor="gateway-invitation">Gateway invitation</label>
        <textarea
          id="gateway-invitation"
          value={text}
          onChange={(event) => setText(event.target.value)}
          autoComplete="off"
          spellCheck={false}
          maxLength={16384}
          rows={3}
        />
        <button
          className="hub-button primary"
          disabled={busy || unsaved || !text.trim()}
          onClick={accept}
        >
          Accept or refresh invitation
        </button>
        <p className="connection-note">
          For a saved gateway, this first checks your existing access and
          refreshes its address. It never pairs automatically.
        </p>
        {view.invitation && (
          <div role="status" data-invitation-state={view.invitation.state}>
            <p>
              {view.invitation.state === 'can-redeem'
                ? 'The gateway requires enrollment. Accept gateway-wide access to continue.'
                : view.invitation.state === 'expired'
                  ? 'This invitation has expired. Request a new one from the gateway owner.'
                  : view.invitation.state === 'refreshed'
                    ? 'Saved gateway address updated.'
                    : view.invitation.state === 'refresh-failed'
                      ? 'The saved gateway could not be checked. Retry or request a new invitation.'
                      : 'Checking saved gateway access…'}
            </p>
            {view.invitation.state === 'can-redeem' && (
              <button
                className="hub-button primary"
                disabled={busy || unsaved}
                onClick={() =>
                  run(() => owner.redeemInvitation(view.invitation!.nodeId))
                }
              >
                Accept gateway-wide access
              </button>
            )}
            {view.invitation.state === 'refresh-failed' && (
              <button
                className="hub-button"
                disabled={busy || unsaved}
                onClick={() =>
                  run(() =>
                    owner.retryInvitationRefresh(view.invitation!.nodeId),
                  )
                }
              >
                Retry address refresh
              </button>
            )}
          </div>
        )}
      </section>
      <section aria-labelledby="saved-gateways">
        <h2 id="saved-gateways">Saved gateways</h2>
        {!view.gateways.length && (
          <p>No saved gateways. Ask a gateway owner for an invitation.</p>
        )}
        <div className="connection-grid">
          {view.gateways.map((gateway) => (
            <article
              className="connection-card"
              key={gateway.record.nodeId}
              data-gateway={gateway.record.nodeId}
              data-recovery={gateway.recovery}
            >
              <h3>{gateway.record.name}</h3>
              <code>{gateway.record.nodeId}</code>
              {gateway.error && (
                <p role="alert" data-error-code={gateway.error.code}>
                  {gateway.error.message}
                </p>
              )}
              {(gateway.recovery === 'pending' ||
                gateway.recovery === 'unknown') && (
                <p>
                  Finish pairing with a new invitation using this same browser
                  identity.
                </p>
              )}
              {gateway.recovery === 'unsaved' && (
                <p>
                  Access was confirmed, but the local save failed. Retry save
                  without using another invitation.
                </p>
              )}
              <div className="connection-actions">
                {gateway.recovery === 'unsaved' ? (
                  <button
                    className="hub-button primary"
                    disabled={busy}
                    onClick={() =>
                      run(() =>
                        owner.retryEnrollmentSave(gateway.record.nodeId),
                      )
                    }
                  >
                    Retry save
                  </button>
                ) : (
                  gateway.record.enrollment === 'accepted' && (
                    <button
                      className="hub-button primary"
                      disabled={busy || unsaved}
                      onClick={() =>
                        run(() =>
                          owner.selectGateway(
                            gateway.record.nodeId,
                            !gateway.selected,
                          ),
                        )
                      }
                    >
                      {gateway.selected ? 'Disconnect' : 'Connect'}
                    </button>
                  )
                )}
                {gateway.selected && (
                  <button
                    className="hub-button"
                    disabled={busy || unsaved || !source}
                    onClick={() =>
                      run(() =>
                        owner.retryConnection({
                          gatewayId: gateway.record.nodeId,
                        }),
                      )
                    }
                  >
                    Retry connection
                  </button>
                )}
                <button
                  className="hub-button"
                  disabled={busy || (unsaved && gateway.recovery !== 'unsaved')}
                  onClick={() => setConfirmation(gateway.record.nodeId)}
                >
                  Forget local connection…
                </button>
              </div>
            </article>
          ))}
        </div>
        {view.connections === 'suspended' && !unsaved && (
          <button
            className="hub-button primary"
            disabled={busy}
            onClick={() => run(() => owner.connectSelected())}
          >
            Resume selected connections
          </button>
        )}
      </section>
      {confirmation && (
        <section className="connection-card" aria-label="Confirm local removal">
          <h2>
            {confirmation === 'reset'
              ? 'Reset this browser identity?'
              : 'Forget this local connection?'}
          </h2>
          <p>
            Browser ID: <code>{view.endpointId}</code>
          </p>
          <p>
            {confirmation === 'reset'
              ? 'This removes your browser key and all saved connections. Your boards remain.'
              : 'This removes the saved connection from this browser. Your boards remain.'}{' '}
            Gateway trust is not revoked. The gateway owner must run{' '}
            <code>trust forget</code> with this browser ID to revoke access.
          </p>
          <div className="connection-actions">
            <button
              className="hub-button danger"
              disabled={busy}
              onClick={() =>
                run(async () => {
                  if (confirmation === 'reset') {
                    await owner.resetIdentity();
                    reopen();
                  } else {
                    await owner.forgetLocalGateway(confirmation);
                    setConfirmation(null);
                  }
                })
              }
            >
              {confirmation === 'reset'
                ? 'Confirm identity reset'
                : 'Confirm forget local connection'}
            </button>
            <button
              className="hub-button"
              disabled={busy}
              onClick={() => setConfirmation(null)}
            >
              Cancel
            </button>
          </div>
        </section>
      )}
      <footer>
        <button
          className="hub-button danger"
          disabled={busy}
          onClick={() => setConfirmation('reset')}
        >
          Reset browser identity…
        </button>
        <p>
          SDK PNG images are supported. External images, audio and video are
          unavailable in this preview.
        </p>
      </footer>
    </>
  );
}
