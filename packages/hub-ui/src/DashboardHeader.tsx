import type { Ref } from 'react';
import { Button } from './Button.js';
import type { Dashboard, DashboardPreset } from './contracts.js';
import type { DashboardEditor } from './useDashboardEditor.js';

export function DashboardHeader({
  editor,
  active,
  current,
  preset,
  busy,
  boardTrigger,
  onBoard,
  discardThen,
  save,
  remove,
  focusMain,
}: {
  editor: DashboardEditor;
  active: Dashboard | DashboardPreset | undefined;
  current: Dashboard | undefined;
  preset: DashboardPreset | undefined;
  busy: boolean;
  boardTrigger: Ref<HTMLButtonElement>;
  onBoard: () => void;
  discardThen: (operation: () => void) => void;
  save: () => void;
  remove: () => void;
  focusMain: () => void;
}) {
  const { draft, dirty, validated, startEdit } = editor;
  const editing = draft !== null;
  return (
    <header className="page-header">
      <div className="header-copy">
        {editing ? (
          <>
            <label htmlFor="dashboard-name">Dashboard name</label>
            <input
              id="dashboard-name"
              aria-invalid={validated && !draft.name.trim()}
              aria-describedby={validated && !draft.name.trim() ? 'name-error' : undefined}
              className="dashboard-name"
              aria-label="Dashboard name"
              maxLength={200}
              value={draft.name}
              disabled={busy}
              onChange={(e) => editor.rename(e.target.value)}
            />
            {validated && !draft.name.trim() && (
              <p id="name-error" className="field-error">
                Enter a dashboard name.
              </p>
            )}
          </>
        ) : (
          <h1>{active?.name ?? 'Dashboards'}</h1>
        )}
        <p className="subtitle">
          {editing
            ? dirty
              ? 'Unsaved changes'
              : 'Editing dashboard'
            : preset
              ? 'Preset · Save a copy to edit'
              : ''}
        </p>
      </div>
      <div className="toolbar">
        {editing ? (
          <>
            <Button disabled={busy} onClick={() => discardThen(focusMain)}>
              Cancel
            </Button>
            <Button className="primary" disabled={busy} onClick={save}>
              {busy ? 'Saving…' : 'Save dashboard'}
            </Button>
          </>
        ) : current ? (
          <>
            <Button disabled={busy} className="danger" onClick={remove}>
              Delete dashboard
            </Button>
            <Button className="primary" disabled={busy} onClick={() => startEdit(structuredClone(current))}>
              Edit dashboard
            </Button>
          </>
        ) : preset ? (
          <Button
            className="primary"
            disabled={busy}
            onClick={() => startEdit({ ...structuredClone(preset), id: '', revision: 0 })}
          >
            Save as dashboard
          </Button>
        ) : null}
        {!editing && active && (
          <Button
            ref={boardTrigger}
            disabled={busy}
            onClick={() => {
              onBoard();
            }}
          >
            Board view
          </Button>
        )}
      </div>
    </header>
  );
}
