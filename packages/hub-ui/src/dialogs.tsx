import { useTranslation } from 'react-i18next';
import { useEffect, useRef, type ReactNode } from 'react';
import { Button } from './Button.js';
import type { Panel } from './contracts.js';
import type { ServiceView } from './services.js';

export interface DialogProps {
  returnTo: HTMLElement | null;
  onClose: () => void;
}
export function restoreDialogFocus(target: HTMLElement | null) {
  requestAnimationFrame(() => {
    if (target?.isConnected) target.focus();
    else document.querySelector<HTMLElement>('#workspace')?.focus();
  });
}
export function Modal({
  labelledBy,
  returnTo,
  onClose,
  children,
}: DialogProps & { labelledBy: string; children: ReactNode }) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  return (
    <dialog
      ref={dialog}
      className="picker"
      aria-labelledby={labelledBy}
      onCancel={(event) => {
        event.preventDefault();
        dialog.current?.close();
        onClose();
        restoreDialogFocus(returnTo);
      }}
    >
      {children}
    </dialog>
  );
}
export interface Confirmation {
  title: string;
  body: string;
  action: string;
  run: () => void;
}
export function ConfirmationDialog({
  confirmation,
  returnTo,
  onClose,
}: DialogProps & { confirmation: Confirmation }) {
  const { t } = useTranslation();
  return (
    <Modal labelledBy="confirm-title" returnTo={returnTo} onClose={onClose}>
      <h2 id="confirm-title">{confirmation.title}</h2>
      <p>{confirmation.body}</p>
      <div className="toolbar">
        <Button
          autoFocus
          onClick={() => {
            onClose();
            restoreDialogFocus(returnTo);
          }}
        >
          {t('common.cancel')}
        </Button>
        <Button
          className="danger"
          onClick={() => {
            onClose();
            confirmation.run();
          }}
        >
          {confirmation.action}
        </Button>
      </div>
    </Modal>
  );
}
export function DetailsDialog({
  panel,
  services,
  returnTo,
  onClose,
}: DialogProps & { panel: Panel; services: ServiceView[] }) {
  const { t } = useTranslation();
  return (
    <Modal labelledBy="details-title" returnTo={returnTo} onClose={onClose}>
      <h2 id="details-title">{t('view.detailsTitle')}</h2>
      <dl>
        <dt>{t('common.name')}</dt>
        <dd>{panel.title}</dd>
        <dt>{t('common.device')}</dt>
        <dd>
          {services.find((s) => s.node?.id === panel.reference.nodeId)?.node?.name ?? panel.reference.nodeId}
        </dd>
        <dt>{t('common.app')}</dt>
        <dd>
          {services.find((s) => s.id === `${panel.reference.nodeId}~${panel.reference.serviceId}`)?.name ??
            panel.reference.serviceId}
        </dd>
        <dt>{t('common.viewId')}</dt>
        <dd>{panel.reference.surfaceId}</dd>
      </dl>
      <Button
        onClick={() => {
          onClose();
          restoreDialogFocus(returnTo);
        }}
      >
        {t('view.closeDetails')}
      </Button>
    </Modal>
  );
}
