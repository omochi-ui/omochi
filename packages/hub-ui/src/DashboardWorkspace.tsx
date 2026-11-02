import { useEffect, useRef, useState } from 'react';
import type { A2uiSession } from '@omochi-ui/renderer';
import { Button } from './Button.js';
import { Icon } from './Icon.js';
import type { Panel } from './contracts.js';
import type { ServiceView } from './services.js';
import { useDashboards } from './useDashboards.js';
import { emptyDashboard, useDashboardEditor } from './useDashboardEditor.js';
import { Sidebar } from './Sidebar.js';
import { DashboardHeader } from './DashboardHeader.js';
import { DashboardGrid } from './DashboardGrid.js';
import { ViewPicker } from './ViewPicker.js';
import { DashboardNotices } from './DashboardNotices.js';
import { DashboardEmptyState } from './DashboardEmptyState.js';
import { ConfirmationDialog, DetailsDialog, type Confirmation } from './dialogs.js';

const focusMain = () =>
  requestAnimationFrame(() => document.querySelector<HTMLElement>('#workspace')?.focus());

export function DashboardWorkspace({ session, services }: { session: A2uiSession; services: ServiceView[] }) {
  const data = useDashboards();
  const editor = useDashboardEditor();
  const { dashboards, presets, selected, loading, busy, error, reload, current, preset } = data;
  const { draft, dirty, startEdit } = editor;
  const active = draft ?? current ?? preset;
  const editing = draft !== null;
  const connected = session.status === 'Connected';
  const available = services.filter((s) => s.status === 'connected').length;
  const panels = [...(active?.panels ?? [])].sort((a, b) => a.y - b.y || a.x - b.x);
  const [board, setBoard] = useState(() => new URLSearchParams(location.search).get('view') === 'board');
  const [navOpen, setNavOpen] = useState(false);
  const [narrow, setNarrow] = useState(() => matchMedia('(max-width: 760px)').matches);
  const [picker, setPicker] = useState(false);
  const [details, setDetails] = useState<Panel | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const returnFocus = useRef<HTMLElement | null>(null);
  const boardTrigger = useRef<HTMLButtonElement>(null);
  const rememberFocus = () => {
    returnFocus.current = document.activeElement as HTMLElement;
  };
  useEffect(() => {
    const media = matchMedia('(max-width: 760px)');
    const change = () => setNarrow(media.matches);
    media.addEventListener('change', change);
    return () => media.removeEventListener('change', change);
  }, []);
  useEffect(() => {
    const url = new URL(location.href);
    if (selected.startsWith('preset:')) {
      url.searchParams.set('preset', selected.slice('preset:'.length));
      url.searchParams.delete('dashboard');
    } else if (selected) {
      url.searchParams.set('dashboard', selected.slice('dashboard:'.length));
      url.searchParams.delete('preset');
    }
    if (board) url.searchParams.set('view', 'board');
    else url.searchParams.delete('view');
    history.replaceState(null, '', url);
  }, [selected, board]);
  useEffect(() => {
    document.title = `${active?.name ?? 'Dashboards'} · omochi`;
  }, [active?.name]);
  const clearDraft = () => {
    editor.clear();
    setPicker(false);
    data.clearError();
  };
  const discardThen = (operation: () => void) => {
    if (!dirty) {
      clearDraft();
      operation();
      return;
    }
    rememberFocus();
    setConfirmation({
      title: 'Discard unsaved changes?',
      body: 'Your changes to this dashboard will be lost.',
      action: 'Discard changes',
      run: () => {
        clearDraft();
        operation();
      },
    });
  };
  const switchDashboard = (id: string) =>
    discardThen(() => {
      data.select(id);
      setNavOpen(false);
      focusMain();
    });
  const openPicker = () => {
    returnFocus.current = document.getElementById('add-view') ?? (document.activeElement as HTMLElement);
    setPicker(true);
  };
  const enterBoard = () => {
    setBoard(true);
    requestAnimationFrame(() => document.querySelector<HTMLButtonElement>('.exit-board')?.focus());
  };
  const save = () => {
    if (!draft || !editor.validate()) return;
    void data.save(draft, () => {
      clearDraft();
      editor.announce('Dashboard saved.');
      focusMain();
    });
  };
  const remove = () => {
    if (!current) return;
    rememberFocus();
    setConfirmation({
      title: `Delete “${current.name}”?`,
      body: 'This dashboard cannot be restored. The apps and their views will not be deleted.',
      action: 'Delete dashboard',
      run: () => {
        void data.remove(current, () => {
          editor.announce('Dashboard deleted.');
          focusMain();
        });
      },
    });
  };
  return (
    <div className={`hub-layout ${board ? 'board-only' : ''} ${navOpen ? 'nav-open' : ''}`}>
      <a className="skip-link" href="#workspace">
        Skip to board
      </a>
      <Button
        className="mobile-nav"
        aria-expanded={navOpen}
        aria-controls="dashboard-navigation"
        onClick={() => setNavOpen(!navOpen)}
      >
        Dashboards
      </Button>
      <Sidebar
        dashboards={dashboards}
        presets={presets}
        selected={selected}
        busy={busy}
        loading={loading}
        dirty={dirty}
        connected={connected}
        available={available}
        switchDashboard={switchDashboard}
        discardThen={discardThen}
        onCreate={() => discardThen(() => startEdit(emptyDashboard()))}
      />
      <main id="workspace" tabIndex={-1} className="workspace">
        <div className="workspace-status">
          <div className="connection-status" role="status">
            {connected ? '✓ Hub connected' : `Hub: ${session.status}`}
            <span>
              {available} {available === 1 ? 'app connected' : 'apps connected'}
            </span>
          </div>
          {board && (
            <Button
              className="exit-board"
              onClick={() => {
                setBoard(false);
                requestAnimationFrame(() => {
                  if (boardTrigger.current) boardTrigger.current.focus();
                  else focusMain();
                });
              }}
            >
              Exit board view
            </Button>
          )}
        </div>

        <DashboardHeader
          editor={editor}
          active={active}
          current={current}
          preset={preset}
          busy={busy}
          boardTrigger={boardTrigger}
          onBoard={enterBoard}
          discardThen={discardThen}
          save={save}
          remove={remove}
          focusMain={focusMain}
        />
        <DashboardNotices
          editor={editor}
          busy={busy}
          error={error}
          connectionError={session.error}
          narrow={narrow}
          save={save}
          reload={() =>
            discardThen(() => {
              void reload();
            })
          }
          restorePanel={() => {
            editor.restorePanel();
            focusMain();
          }}
        />
        {active && (
          <div className="board-bar">
            <span>
              {panels.length} {panels.length === 1 ? 'view' : 'views'}
            </span>
            {editing && (
              <Button
                id="add-view"
                className="with-icon"
                disabled={busy || panels.length >= 200}
                onClick={openPicker}
              >
                <Icon name="plus" /> Add view
              </Button>
            )}
          </div>
        )}
        {loading ? (
          <div className="empty-state">Loading dashboards…</div>
        ) : !panels.length ? (
          <DashboardEmptyState
            hasDashboard={!!active}
            board={board}
            disabled={busy || !!error}
            onAdd={() => {
              if (!draft)
                startEdit(
                  current
                    ? structuredClone(current)
                    : preset
                      ? { ...structuredClone(preset), id: '', revision: 0 }
                      : emptyDashboard(),
                );
              openPicker();
            }}
          />
        ) : (
          <DashboardGrid
            panels={panels}
            editor={editor}
            session={session}
            services={services}
            busy={busy}
            narrow={narrow}
            onDetails={(panel) => {
              rememberFocus();
              setDetails(panel);
            }}
          />
        )}
        <p id="layout-help" className="sr-only">
          Use arrow keys on the move handle, or select Adjust layout for move and resize buttons.
        </p>
      </main>
      {picker && (
        <ViewPicker
          services={services}
          surfaceIds={session.surfaceIds}
          draft={draft}
          returnTo={returnFocus.current}
          onClose={() => setPicker(false)}
          addSurface={(service, id) => {
            editor.addSurface(service, id);
            setPicker(false);
          }}
        />
      )}
      {details && (
        <DetailsDialog
          panel={details}
          services={services}
          returnTo={returnFocus.current}
          onClose={() => setDetails(null)}
        />
      )}
      {confirmation && (
        <ConfirmationDialog
          confirmation={confirmation}
          returnTo={returnFocus.current}
          onClose={() => setConfirmation(null)}
        />
      )}
    </div>
  );
}
