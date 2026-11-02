import { Button } from './Button.js';
import { Icon } from './Icon.js';
import type { Dashboard, DashboardPreset } from './contracts.js';

export function Sidebar({
  dashboards,
  presets,
  selected,
  busy,
  loading,
  dirty,
  connected,
  available,
  switchDashboard,
  discardThen,
  onCreate,
}: {
  dashboards: Dashboard[];
  presets: DashboardPreset[];
  selected: string;
  busy: boolean;
  loading: boolean;
  dirty: boolean;
  connected: boolean;
  available: number;
  switchDashboard: (id: string) => void;
  discardThen: (operation: () => void) => void;
  onCreate: () => void;
}) {
  return (
    <aside id="dashboard-navigation" className="sidebar">
      <a
        className="brand"
        href="/"
        onClick={(event) => {
          if (dirty) {
            event.preventDefault();
            discardThen(() => {
              location.href = '/';
            });
          }
        }}
      >
        <svg className="brand-mark" viewBox="0 0 96 96" aria-hidden="true">
          <path
            fill="currentColor"
            fillRule="evenodd"
            d="M32 12h20q24 0 24 24v-2h1a13 13 0 0 1 13 13v2a13 13 0 0 1-13 13h-1q-1 22-24 22H32Q8 84 8 60V36Q8 12 32 12Zm27 36a17 17 0 1 0-34 0 17 17 0 0 0 34 0Z"
          />
        </svg>{' '}
        omochi <span className="brand-tag">hub</span>
      </a>
      <div className="nav-caption">WORKSPACE</div>
      {!!presets.length && (
        <>
          <div className="nav-section">
            <Icon name="preset" /> Presets <span className="count">{presets.length}</span>
          </div>
          <nav aria-label="Presets">
            {presets.map((p) => (
              <Button
                static
                key={p.id}
                className={`dashboard-link ${selected === `preset:${p.id}` ? 'selected' : ''}`}
                aria-current={selected === `preset:${p.id}` ? 'page' : undefined}
                disabled={busy}
                onClick={() => switchDashboard(`preset:${p.id}`)}
              >
                {p.name}
              </Button>
            ))}
          </nav>
        </>
      )}
      <div className="nav-section">
        <Icon name="dashboard" /> Dashboards <span className="count">{dashboards.length}</span>
      </div>
      <nav aria-label="Dashboards">
        {dashboards.map((d) => (
          <Button
            static
            key={d.id}
            className={`dashboard-link ${selected === `dashboard:${d.id}` ? 'selected' : ''}`}
            aria-current={selected === `dashboard:${d.id}` ? 'page' : undefined}
            disabled={busy}
            onClick={() => switchDashboard(`dashboard:${d.id}`)}
          >
            {d.name}
          </Button>
        ))}
        {!dashboards.length && <p className="sidebar-hint">Your dashboards will appear here.</p>}
      </nav>
      <Button className="new-dashboard with-icon" disabled={loading || busy} onClick={() => onCreate()}>
        <Icon name="plus" /> Create dashboard
      </Button>
      <div className="sidebar-footer">
        <span className={`signal ${connected ? 'online' : ''}`} />
        {connected ? 'Hub connected' : 'Reconnecting…'}
        <small>{available} apps available</small>
      </div>
    </aside>
  );
}
