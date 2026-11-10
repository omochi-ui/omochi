import { useId, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from './Button.js';
import { LanguageSwitcher } from './LanguageSwitcher.js';
import { Modal, type DialogProps } from './dialogs.js';
import { localizedError } from './localized-error.js';

export interface BrowserIdentitySettings {
  id: string | null;
  busy: boolean;
  error: Error | null;
  onLoad(): void;
  onReset(trigger: HTMLElement): void;
}

export function DashboardSettings({
  returnTo,
  onClose,
  browserIdentity,
}: DialogProps & { browserIdentity?: BrowserIdentitySettings }) {
  const { t } = useTranslation();
  const id = useId();
  const identityText = useRef<HTMLElement>(null);
  const [message, setMessage] = useState<
    'settings.copied' | 'settings.selected' | null
  >(null);
  const close = () => {
    setMessage(null);
    onClose();
  };
  const copy = async () => {
    if (!browserIdentity?.id) return;
    try {
      await navigator.clipboard.writeText(browserIdentity.id);
      setMessage('settings.copied');
    } catch {
      const element = identityText.current;
      if (!element) return;
      element.focus();
      const range = document.createRange();
      range.selectNodeContents(element);
      const selection = window.getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
      setMessage('settings.selected');
    }
  };
  return (
    <Modal
      labelledBy={`${id}-title`}
      title={t('settings.title')}
      returnTo={returnTo}
      onClose={close}
      open={!!returnTo}
      className="settings-dialog"
    >
      <section className="settings-section">
        <LanguageSwitcher />
      </section>
      {browserIdentity && (
        <section
          className="settings-section"
          aria-labelledby={`${id}-identity`}
        >
          <h3 id={`${id}-identity`}>{t('settings.browserIdentity')}</h3>
          <p>{t('settings.identityHelp')}</p>
          {browserIdentity.id ? (
            <>
              <code
                ref={identityText}
                tabIndex={-1}
                className="browser-identity"
                data-browser-id={browserIdentity.id}
              >
                {browserIdentity.id}
              </code>
              <Button
                onClick={() => {
                  void copy();
                }}
              >
                {t('settings.copy')}
              </Button>
            </>
          ) : (
            <>
              {browserIdentity.busy ? (
                <p role="status">{t('settings.loadingIdentity')}</p>
              ) : (
                <>
                  {browserIdentity.error && (
                    <p role="alert">
                      {localizedError(browserIdentity.error, t)}
                    </p>
                  )}
                  <Button onClick={browserIdentity.onLoad}>
                    {t('settings.loadIdentity')}
                  </Button>
                </>
              )}
            </>
          )}
          <p className="announcement" role="status">
            {message && t(message)}
          </p>
          <div className="settings-reset">
            <Button
              className="danger"
              disabled={browserIdentity.busy}
              onClick={(event) => {
                close();
                browserIdentity.onReset(returnTo ?? event.currentTarget);
              }}
            >
              {t('connection.reset')}
            </Button>
          </div>
        </section>
      )}
    </Modal>
  );
}
