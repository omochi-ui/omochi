import { useId, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import type { SessionFault } from '@omochi-ui/core/browser';
import { Button } from '../Button.js';
import { Modal } from '../dialogs.js';
import type {
  ConnectionAction,
  PendingInvitation,
} from './connection-controller.js';
import { nativeConnectionLink } from './native-link.js';

export function ConnectionError({ error }: { error: SessionFault | null }) {
  const { t } = useTranslation();
  return (
    error && (
      <p className="notice error" role="alert" data-error-code={error.code}>
        {t(`sessionError.${error.code}`)}
      </p>
    )
  );
}

export function OwnerInvitation({
  invitation,
  busy,
  ownsProfile,
  error,
  dispatch,
}: {
  invitation: PendingInvitation;
  busy: boolean;
  ownsProfile: boolean;
  error: SessionFault | null;
  dispatch(action: ConnectionAction): Promise<void>;
}) {
  const { t } = useTranslation();
  const title = useId();
  const trigger = useRef(document.activeElement as HTMLElement | null);
  const link = invitation.kind === 'invitation' ? invitation.link : null;
  const owner =
    link?.mode === 'owner' ||
    (invitation.kind === 'savePending' && invitation.accepted.source.owner);
  const canDismiss = !busy && invitation.kind !== 'savePending';
  const dismiss = () => {
    void dispatch({ type: 'dismiss' });
  };
  const appLink = link ? nativeConnectionLink(link) : null;

  return (
    <Modal
      labelledBy={title}
      title={t(owner ? 'owner.register' : 'connection.acceptTitle')}
      returnTo={trigger.current}
      onClose={dismiss}
      dismissible={canDismiss}
      className="owner-invitation"
    >
      <p>{t(owner ? 'connection.ownerHelp' : 'connection.inviteHelp')}</p>
      {link && (
        <details className="connection-details">
          <summary>{t('gateway.details')}</summary>
          <dl>
            <dt>{t('connection.hubId')}</dt>
            <dd>
              <code>{link.hubId}</code>
            </dd>
          </dl>
        </details>
      )}
      <ConnectionError error={error} />
      {invitation.kind === 'savePending' ? (
        <>
          <p role="status">{t('connection.saveFailed')}</p>
          <Button
            className="primary"
            disabled={busy}
            onClick={() => {
              void dispatch({ type: 'accept' });
            }}
          >
            {t('connection.retrySave')}
          </Button>
        </>
      ) : (
        <div className="dialog-actions">
          <Button
            className="primary"
            disabled={busy || !ownsProfile}
            onClick={() => {
              void dispatch({ type: 'accept' });
            }}
          >
            {t('connection.accept')}
          </Button>
          {error && !ownsProfile && (
            <Button
              disabled={busy}
              onClick={() => {
                void dispatch({ type: 'retry' });
              }}
            >
              {t('connection.retryOpen')}
            </Button>
          )}
          {appLink && !busy && (
            <a className="hub-button" href={appLink}>
              {t('native.open')}
            </a>
          )}
          <Button disabled={!canDismiss} onClick={dismiss}>
            {t('connection.dismiss')}
          </Button>
        </div>
      )}
    </Modal>
  );
}
