import { Button } from './Button.js';
import type { DashboardEditor } from './useDashboardEditor.js';

export function DashboardNotices({
  editor,
  busy,
  error,
  connectionError,
  narrow,
  save,
  reload,
  restorePanel,
}: {
  editor: DashboardEditor;
  busy: boolean;
  error: string | null;
  connectionError: string | null;
  narrow: boolean;
  save: () => void;
  reload: () => void;
  restorePanel: () => void;
}) {
  const { draft, undo, message } = editor;
  const editing = draft !== null;
  return (
    <>
      <div className="announcement" role="status">
        {message}
      </div>
      {error && (
        <div className="notice error" role="alert">
          <span>
            {draft
              ? 'Could not save changes. Your edits are kept. Retry saving, or discard changes to load the saved version.'
              : 'Could not load or update dashboards. Try again.'}{' '}
            <small>{error}</small>
          </span>
          {draft && (
            <Button disabled={busy} onClick={save}>
              Retry save
            </Button>
          )}
          <Button disabled={busy} onClick={reload}>
            {draft ? 'Discard and reload' : 'Retry'}
          </Button>
        </div>
      )}
      {connectionError && (
        <div className="notice" role="status">
          Hub connection interrupted. Reconnecting automatically. Check that the hub is running.{' '}
          <small>{connectionError}</small>
        </div>
      )}
      {undo && editing && (
        <div className="notice">
          <span>View removed.</span>
          <Button onClick={restorePanel}>Undo removal</Button>
        </div>
      )}
      {narrow && editing && (
        <div className="notice">
          Use Move up or Move down to reorder views. This also updates the desktop layout. Resize views on a
          wider screen.
        </div>
      )}
    </>
  );
}
