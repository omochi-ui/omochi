import { useTranslation } from 'react-i18next';
import { useEffect, useRef, type ReactNode } from 'react';
import { Button } from './Button.js';
import { Icon } from './Icon.js';
import type { Panel } from './contracts.js';
import type { ServiceView } from './services.js';

export interface DialogProps {
  returnTo: HTMLElement | null;
  onClose: () => void;
}
export function DialogHeader({ id, title, onClose, disabled = false }: { id: string; title: string; onClose: () => void; disabled?: boolean }) {
  const { t } = useTranslation();
  return <header className="dialog-header"><h2 id={id}>{title}</h2><Button className="icon-button" aria-label={t('common.close')} disabled={disabled} onClick={onClose}><Icon name="close" /></Button></header>;
}
export function restoreDialogFocus(target: HTMLElement | null) {
  requestAnimationFrame(() => {
    if (document.querySelector('dialog:modal')) return;
    if (target?.isConnected && target !== document.body && target.getClientRects().length) {
      target.focus();
      if (document.activeElement === target) return;
    }
    document.querySelector<HTMLElement>('#workspace, .connections-page h1')?.focus();
  });
}
export function Modal({
  labelledBy,
  title,
  returnTo,
  onClose,
  children,
  className = '',
  open = true,
  dismissible = true,
}: DialogProps & { labelledBy: string; title: string; children: ReactNode; className?: string; open?: boolean; dismissible?: boolean }) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const element = dialog.current;
    if (!open) return;
    element?.showModal();
    return () => { element?.close(); restoreDialogFocus(returnTo); };
  }, [open, returnTo]);
  return (
    <dialog
      ref={dialog}
      className={`picker ${className}`}
      aria-labelledby={labelledBy}
      onCancel={(event) => {
        event.preventDefault();
        if (!dismissible) return;
        dialog.current?.close();
        onClose();
        restoreDialogFocus(returnTo);
      }}
    >
      <DialogHeader id={labelledBy} title={title} onClose={onClose} disabled={!dismissible} />
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
    <Modal labelledBy="confirm-title" title={confirmation.title} returnTo={returnTo} onClose={onClose}>
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
    <Modal labelledBy="details-title" title={t('view.detailsTitle')} returnTo={returnTo} onClose={onClose}>
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
