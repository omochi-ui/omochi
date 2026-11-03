import { useCallback, useEffect, useId, useRef, useState, type PointerEvent, type ReactNode } from 'react';
import { SurfaceView, useA2uiSource } from '@omochi-ui/renderer';
import { Button } from './Button.js';
import { Icon } from './Icon.js';
import { referenceId, type Dashboard, type DashboardPreset, type Panel } from './contracts.js';
import { placePanel, resizePanel, reorderPanels, type ResizeEdge } from './layout.js';
import { desiredServices, groupByNode, splitSurfaceId, type ServiceView } from './services.js';
import { BOARD_FILE_LIMIT, exportBoard, parseBoard, type SourceLabel } from './board-codec.js';
import { BoardFault } from './board-repository.js';
import type { DashboardEnvironment } from './environment.js';
const emptyDashboard = (): Dashboard => ({ id: '', name: 'Untitled dashboard', revision: 0, panels: [] });

export default function App({ environment, connectionUi }: { environment: DashboardEnvironment; connectionUi?: ReactNode }) {
  const session = useA2uiSource(environment.session);
  const services: readonly ServiceView[] = session.services;
  const direct = environment.session.imagePolicy === 'sdk-png';
  const [gateways, setGateways] = useState<ReadonlySet<string>>(new Set());
  const [dashboards, setDashboards] = useState<readonly Dashboard[]>([]);
  const [presets, setPresets] = useState<readonly DashboardPreset[]>([]);
  const [selected, setSelected] = useState('');
  const [draft, setDraft] = useState<Dashboard | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [picker, setPicker] = useState(false);
  const [search, setSearch] = useState('');
  const [pickerService, setPickerService] = useState<string | null>(null);
  const [sourceError, setSourceError] = useState<string | null>(null);
  const [importPreview, setImportPreview] = useState<ReturnType<typeof parseBoard> | null>(null);
  const importDialog = useRef<HTMLDialogElement>(null);
  const importFile = useRef<HTMLInputElement>(null);
  const reloadSequence = useRef(0);
  const fieldId = useId();
  const [narrow, setNarrow] = useState(() => matchMedia('(max-width: 760px)').matches);
  const grid = useRef<HTMLDivElement>(null);
  const pickerDialog = useRef<HTMLDialogElement>(null);
  const baseline = useRef('');
  const [board, setBoard] = useState(() => new URLSearchParams(location.search).get('view') === 'board');
  const [navOpen, setNavOpen] = useState(false);
  const [message, setMessage] = useState('');
  const [validated, setValidated] = useState(false);
  const [undo, setUndo] = useState<Panel[] | null>(null);
  const [adjust, setAdjust] = useState<string | null>(null);
  const [details, setDetails] = useState<Panel | null>(null);
  const [confirmation, setConfirmation] = useState<{ title: string; action: string; body: string; run: () => void } | null>(null);
  const confirmDialog = useRef<HTMLDialogElement>(null);
  const detailsDialog = useRef<HTMLDialogElement>(null);
  const returnFocus = useRef<HTMLElement | null>(null);
  const boardTrigger = useRef<HTMLButtonElement>(null);
  const [preview, setPreview] = useState<Panel[] | null>(null);
  const previewRef = useRef<Panel[] | null>(null);
  const gesture = useRef<{ id: string; mode: 'move' | ResizeEdge; x: number; y: number; scrollY: number; column: number; row: number; panel: Panel; panels: Panel[] } | null>(null);
  const current = selected.startsWith('dashboard:') ? dashboards.find(d => `dashboard:${d.id}` === selected) : undefined;
  const preset = selected.startsWith('preset:') ? presets.find(p => `preset:${p.id}` === selected) : undefined;
  const active = draft ?? current ?? preset;
  const editing = draft !== null;
  const dirty = !!draft && JSON.stringify(draft) !== baseline.current;
  const connected = session.status === 'Connected';

  const reload = useCallback(async () => {
    const sequence = ++reloadSequence.current;
    setLoading(true);
    try {
      const [nextDashboards, nextPresets] = await Promise.all([
        environment.boards.list(), environment.boards.presets(),
      ]);
      if (sequence !== reloadSequence.current) return;
      setDashboards(nextDashboards);
      setPresets(nextPresets);
      setSelected(id => {
        if (nextDashboards.some(d => `dashboard:${d.id}` === id) || nextPresets.some(p => `preset:${p.id}` === id)) return id;
        const search = new URLSearchParams(location.search);
        const requestedPreset = search.get('preset');
        if (requestedPreset && nextPresets.some(p => p.id === requestedPreset)) return `preset:${requestedPreset}`;
        const requestedDashboard = search.get('dashboard');
        if (requestedDashboard && nextDashboards.some(d => d.id === requestedDashboard)) return `dashboard:${requestedDashboard}`;
        return nextDashboards[0] ? `dashboard:${nextDashboards[0].id}` : nextPresets[0] ? `preset:${nextPresets[0].id}` : '';
      });
      setError(null);
    } catch (reason) { if (sequence === reloadSequence.current) setError(reason instanceof Error ? reason.message : new BoardFault('StorageUnavailable').message); }
    finally { if (sequence === reloadSequence.current) setLoading(false); }
  }, [environment.boards]);
  useEffect(() => {
    void reload(); const unsubscribe = environment.boards.subscribe(() => { void reload(); });
    return () => { reloadSequence.current++; unsubscribe(); };
  }, [environment.boards, reload]);
  useEffect(() => {
    const selected = new Set<string>(); setGateways(selected); setSourceError(null);
    return environment.session.subscribe(event => {
      if (event.kind === 'status' && /^[a-f0-9]{64}$/.test(event.sourceId) && !selected.has(event.sourceId)) {
        selected.add(event.sourceId); setGateways(new Set(selected));
      }
    });
  }, [environment.session]);
  const desired = JSON.stringify(desiredServices(active?.panels ?? [], picker ? pickerService : null, direct ? gateways : undefined));
  useEffect(() => {
    try { environment.session.setServices(JSON.parse(desired) as string[]); }
    catch (reason) {
      // A profile transition can retire the previous source before React commits its replacement.
      if (!(reason && typeof reason === 'object' && 'code' in reason && reason.code === 'InvalidState')) setSourceError(reason instanceof Error ? reason.message : 'This source is unavailable.');
    }
  }, [environment.session, desired]);
  useEffect(() => {
    const url = new URL(location.href);
    if (selected.startsWith('preset:')) {
      url.searchParams.set('preset', selected.slice('preset:'.length));
      url.searchParams.delete('dashboard');
    } else if (selected) {
      url.searchParams.set('dashboard', selected.slice('dashboard:'.length));
      url.searchParams.delete('preset');
    }
    if (board) url.searchParams.set('view', 'board'); else url.searchParams.delete('view');
    history.replaceState(null, '', url);
  }, [selected, board]);
  useEffect(() => { document.title = `${active?.name ?? 'Dashboards'} · omochi`; }, [active?.name]);
  useEffect(() => { if (confirmation) confirmDialog.current?.showModal(); }, [confirmation]);
  useEffect(() => { if (details) detailsDialog.current?.showModal(); }, [details]);
  useEffect(() => { if (importPreview) importDialog.current?.showModal(); }, [importPreview]);
  useEffect(() => {
    if (picker) pickerDialog.current?.showModal();
    else pickerDialog.current?.close();
  }, [picker]);
  useEffect(() => {
    const media = matchMedia('(max-width: 760px)');
    const change = () => { setNarrow(media.matches); gesture.current = null; setPreview(null); previewRef.current = null; };
    media.addEventListener('change', change);
    return () => media.removeEventListener('change', change);
  }, []);
  useEffect(() => {
    if (!dirty) return;
    const prevent = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = ''; };
    window.addEventListener('beforeunload', prevent);
    return () => window.removeEventListener('beforeunload', prevent);
  }, [dirty]);

  const focusMain = () => requestAnimationFrame(() => document.querySelector<HTMLElement>('#workspace')?.focus());
  const restoreFocus = () => requestAnimationFrame(() => { if (returnFocus.current?.isConnected) returnFocus.current.focus(); focusMain(); });
  const rememberFocus = () => { returnFocus.current = document.activeElement as HTMLElement; };
  const startEdit = (value: Dashboard) => {
    baseline.current = JSON.stringify(value); setDraft(value); setValidated(false); setUndo(null); setMessage(''); setError(null);
    requestAnimationFrame(() => { if (!document.querySelector('dialog[open]')) document.querySelector<HTMLInputElement>('.dashboard-name')?.focus(); });
  };
  const clearDraft = () => { setDraft(null); setPicker(false); setUndo(null); setAdjust(null); setValidated(false); setError(null); };
  const discardThen = (operation: () => void) => {
    if (!dirty) { clearDraft(); operation(); return; }
    rememberFocus();
    setConfirmation({ title: 'Discard unsaved changes?', body: 'Your changes to this dashboard will be lost.', action: 'Discard changes', run: () => { clearDraft(); operation(); } });
  };
  const switchDashboard = (id: string) => discardThen(() => { setSelected(id); setNavOpen(false); focusMain(); });
  const closePicker = () => { pickerDialog.current?.close(); setPicker(false); restoreFocus(); };
  const openPicker = () => { returnFocus.current = document.getElementById('add-view') ?? document.activeElement as HTMLElement; setSearch(''); setPickerService(null); setPicker(true); };
  const run = async (operation: () => Promise<void>) => {
    setBusy(true); setError(null);
    try { await operation(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setBusy(false); }
  };
  const save = () => {
    if (!draft) return;
    setValidated(true);
    if (!draft.name.trim() || draft.panels.some(p => !p.title.trim())) {
      requestAnimationFrame(() => document.querySelector<HTMLInputElement>('[aria-invalid="true"]')?.focus()); return;
    }
    setUndo(null);
    void run(async () => {
      const input = { name: draft.name, panels: draft.panels };
      const saved = await (draft.id ? environment.boards.update(draft.id, draft.revision, input) : environment.boards.create(input));
      setDashboards(all => [...all.filter(d => d.id !== saved.id), saved]);
      setSelected(`dashboard:${saved.id}`); clearDraft(); setMessage('Dashboard saved.'); focusMain();
    });
  };
  const remove = () => {
    if (!current) return;
    rememberFocus();
    setConfirmation({ title: `Delete “${current.name}”?`, body: 'This dashboard cannot be restored. The apps and their views will not be deleted.', action: 'Delete dashboard', run: () => {
      void run(async () => {
        await environment.boards.delete(current.id, current.revision);
        const next = dashboards.filter(d => d.id !== current.id);
        setDashboards(next); setSelected(next[0] ? `dashboard:${next[0].id}` : presets[0] ? `preset:${presets[0].id}` : '');
        setMessage('Dashboard deleted.'); focusMain();
      });
    } });
  };
  const readImport = async (file: File | undefined) => {
    if (!file) return;
    rememberFocus();
    await run(async () => {
      if (file.size > BOARD_FILE_LIMIT) throw new BoardFault('InvalidBoard');
      setImportPreview(parseBoard(new Uint8Array(await file.arrayBuffer())));
    });
    if (importFile.current) importFile.current.value = '';
  };
  const importCopy = () => {
    if (!importPreview) return;
    void run(async () => {
      const saved = await environment.boards.create(importPreview.input);
      setDashboards(all => [...all.filter(board => board.id !== saved.id), saved]);
      setSelected(`dashboard:${saved.id}`); importDialog.current?.close(); setImportPreview(null);
      setMessage('Board imported as a new dashboard. Connect its gateways separately.'); focusMain();
    });
  };
  const download = () => {
    if (!active) return;
    void run(async () => {
      const labels = new Map<string, SourceLabel>();
      for (const service of services) if (service.node) labels.set(service.node.id, { nodeId: service.node.id,
        ...(service.node.name.trim() && service.node.name.length <= 256 ? { name: service.node.name } : {}) });
      const bytes = exportBoard({ ...active, revision: draft?.revision ?? current?.revision ?? 0 }, [...labels.values()]);
      const url = URL.createObjectURL(new Blob([bytes], { type: 'application/json' }));
      try { const anchor = document.createElement('a'); anchor.href = url; anchor.download = 'omochi-board.json'; anchor.click(); }
      finally { setTimeout(() => URL.revokeObjectURL(url), 0); }
      setMessage('Board exported. The file is a copy and contains no gateway access.');
    });
  };
  const changePanel = (panel: Panel) => {
    setUndo(null);
    setDraft(d => d ? { ...d, panels: placePanel(d.panels, panel) } : d);
    setMessage(`Updated ${panel.title}.`);
  };
  const cancelGesture = () => { gesture.current = null; previewRef.current = null; setPreview(null); };
  const beginGesture = (event: PointerEvent<HTMLButtonElement>, panel: Panel, mode: 'move' | ResizeEdge) => {
    if (narrow || !draft || busy || !grid.current || event.button !== 0) return;
    event.preventDefault(); event.currentTarget.focus();
    event.currentTarget.setPointerCapture(event.pointerId);
    const style = getComputedStyle(grid.current);
    const gap = parseFloat(style.columnGap);
    gesture.current = { id: panel.id, mode, x: event.clientX, y: event.clientY, scrollY: window.scrollY,
      column: (grid.current.clientWidth + gap) / 12, row: parseFloat(style.gridAutoRows) + parseFloat(style.rowGap), panel, panels: draft.panels };
    setUndo(null);
  };
  const moveGesture = (event: PointerEvent<HTMLButtonElement>) => {
    const g = gesture.current;
    if (!g || narrow) return;
    const dx = Math.round((event.clientX - g.x) / g.column);
    const dy = Math.round((event.clientY - g.y + window.scrollY - g.scrollY) / g.row);
    const panel = g.mode === 'move' ? { ...g.panel, x: g.panel.x + dx, y: Math.min(100000, g.panel.y + dy) }
      : resizePanel(g.panel, g.mode, dx, dy);
    previewRef.current = placePanel(g.panels, panel); setPreview(previewRef.current);
  };
  const endGesture = () => {
    if (gesture.current && previewRef.current) {
      const panels = previewRef.current;
      setDraft(d => d ? { ...d, panels } : d); setMessage('Layout updated.');
    }
    cancelGesture();
  };
  const reorder = (id: string, direction: -1 | 1) => {
    setUndo(null); setDraft(d => d ? { ...d, panels: reorderPanels(d.panels, id, direction) } : d);
    setMessage(direction < 0 ? 'View moved up.' : 'View moved down.');
  };
  const removePanel = (panel: Panel) => {
    if (!draft) return;
    setUndo(draft.panels); setDraft({ ...draft, panels: draft.panels.filter(p => p.id !== panel.id) }); setAdjust(null);
    setMessage(`${panel.title} removed.`);
    requestAnimationFrame(() => (document.querySelector<HTMLButtonElement>('.panel .details-button') ?? document.querySelector<HTMLButtonElement>('#add-view'))?.focus());
  };
  const addSurface = (service: ServiceView, id: string) => {
    if (!service.node || !draft) return;
    const surface = splitSurfaceId(id);
    const reference = { nodeId: service.node.id, serviceId: service.id.slice(service.node.id.length + 1), surfaceId: surface.surfaceId };
    if (draft.panels.some(p => referenceId(p.reference) === id)) return;
    const panel: Panel = { id: crypto.randomUUID(), title: `${service.name} · ${surface.surfaceId}`, reference,
      x: 0, y: Math.max(0, ...draft.panels.map(p => p.y + p.h)), w: 6, h: 6 };
    setUndo(null); setDraft({ ...draft, panels: [...draft.panels, panel] });
    closePicker(); setMessage('View added.');
  };
  const panels = [...(active?.panels ?? [])].sort((a, b) => a.y - b.y || a.x - b.x);
  const groups = groupByNode(services);
  const matches = services.flatMap(service => service.node && service.status === 'connected' && (!direct || service.id === pickerService) ? session.surfaceIds.filter(id => splitSurfaceId(id).channelId === service.id && `${service.name} ${splitSurfaceId(id).surfaceId}`.toLowerCase().includes(search.toLowerCase())) : []);
  const available = services.filter(s => s.status === 'connected').length;

  return <div className={`hub-layout ${board ? 'board-only' : ''} ${navOpen ? 'nav-open' : ''}`} onKeyDown={event => { if (event.key === 'Escape' && gesture.current) { event.preventDefault(); cancelGesture(); } }}>
    <a className="skip-link" href="#workspace">Skip to board</a>
    <Button className="mobile-nav" aria-expanded={navOpen} aria-controls="dashboard-navigation" onClick={() => setNavOpen(!navOpen)}>Dashboards</Button>
    <aside id="dashboard-navigation" className="sidebar">
      <a className="brand" href="./" onClick={event => { if (dirty) { event.preventDefault(); discardThen(() => { location.href = './'; }); } }}><svg className="brand-mark" viewBox="0 0 96 96" aria-hidden="true"><path fill="currentColor" fillRule="evenodd" d="M32 12h20q24 0 24 24v-2h1a13 13 0 0 1 13 13v2a13 13 0 0 1-13 13h-1q-1 22-24 22H32Q8 84 8 60V36Q8 12 32 12Zm27 36a17 17 0 1 0-34 0 17 17 0 0 0 34 0Z" /></svg> omochi <span className="brand-tag">hub</span></a>
      <div className="nav-caption">WORKSPACE</div>
      {!!presets.length && <><div className="nav-section"><Icon name="preset" /> Presets <span className="count">{presets.length}</span></div>
      <nav aria-label="Presets">
        {presets.map(p => <Button static key={p.id} className={`dashboard-link ${selected === `preset:${p.id}` ? 'selected' : ''}`} aria-current={selected === `preset:${p.id}` ? 'page' : undefined} disabled={busy}
          onClick={() => switchDashboard(`preset:${p.id}`)}>{p.name}</Button>)}
      </nav></>}
      <div className="nav-section"><Icon name="dashboard" /> Dashboards <span className="count">{dashboards.length}</span></div>
      <nav aria-label="Dashboards">
        {dashboards.map(d => <Button static key={d.id} className={`dashboard-link ${selected === `dashboard:${d.id}` ? 'selected' : ''}`} aria-current={selected === `dashboard:${d.id}` ? 'page' : undefined} disabled={busy}
          onClick={() => switchDashboard(`dashboard:${d.id}`)}>{d.name}</Button>)}
        {!dashboards.length && <p className="sidebar-hint">Your dashboards will appear here.</p>}
      </nav>
      <Button className="new-dashboard with-icon" disabled={loading || busy} onClick={() => discardThen(() => startEdit(emptyDashboard()))}><Icon name="plus" /> Create dashboard</Button>
      <Button className="new-dashboard" disabled={loading || busy || editing} onClick={() => { rememberFocus(); importFile.current?.click(); }}>Import board file</Button>
      <input ref={importFile} id="import-board-file" type="file" accept="application/json,.json" hidden onChange={event => { void readImport(event.target.files?.[0]); }} />
      <div className="sidebar-footer"><span className={`signal ${connected ? 'online' : ''}`} />{connected ? 'Hub connected' : 'Reconnecting…'}<small>{available} apps available</small></div>
    </aside>
    <main id="workspace" tabIndex={-1} className="workspace">
      <div className="workspace-status"><div className="connection-status" role="status">{connected ? direct ? '✓ Gateway connected' : '✓ Hub connected' : `${direct ? 'Gateways' : 'Hub'}: ${session.status}`}<span>{available} {available === 1 ? 'app available' : 'apps available'}</span></div>
      {board && <Button className="exit-board" onClick={() => { setBoard(false); requestAnimationFrame(() => { if (boardTrigger.current) boardTrigger.current.focus(); else focusMain(); }); }}>Exit board view</Button>}</div>
      {connectionUi && <details className="connection-settings" open hidden={board}><summary>Connections</summary>{connectionUi}</details>}

      <header className="page-header">
        <div className="header-copy">
          {editing ? <><label htmlFor="dashboard-name">Dashboard name</label><input id="dashboard-name" aria-invalid={validated && !draft.name.trim()} aria-describedby={validated && !draft.name.trim() ? 'name-error' : undefined} className="dashboard-name" aria-label="Dashboard name" maxLength={200} value={draft.name} disabled={busy} onChange={e => setDraft({ ...draft, name: e.target.value })} />{validated && !draft.name.trim() && <p id="name-error" className="field-error">Enter a dashboard name.</p>}</>
            : <h1>{active?.name ?? 'Dashboards'}</h1>}
          <p className="subtitle">{editing ? dirty ? 'Unsaved changes' : 'Editing dashboard' : preset ? 'Preset · Save a copy to edit' : ''}</p>
        </div>
        <div className="toolbar">{editing ? <>
          <Button disabled={busy} onClick={() => discardThen(focusMain)}>Cancel</Button>
          <Button className="primary" disabled={busy} onClick={save}>{busy ? 'Saving…' : 'Save dashboard'}</Button>
        </> : current ? <>
          <Button disabled={busy} className="danger" onClick={remove}>Delete dashboard</Button>
          <Button className="primary" disabled={busy} onClick={() => startEdit(structuredClone(current))}>Edit dashboard</Button>
        </> : preset ? <Button className="primary" disabled={busy} onClick={() => startEdit({ ...structuredClone(preset), id: '', revision: 0 })}>Save as dashboard</Button> : null}{!editing && active && <><Button disabled={busy} onClick={download}>Export board file</Button><Button ref={boardTrigger} disabled={busy} onClick={() => { setBoard(true); requestAnimationFrame(() => document.querySelector<HTMLButtonElement>('.exit-board')?.focus()); }}>Board view</Button></>}</div>
      </header>
      <div className="announcement" role="status">{message}</div>
      {error && <div className="notice error" role="alert"><span>{draft ? 'Could not save changes. Your edits are kept. Retry saving, or discard changes to load the saved version.' : 'Could not load or update dashboards. Try again.'} <small>{error}</small></span>{draft && <Button disabled={busy} onClick={save}>Retry save</Button>}<Button disabled={busy} onClick={() => discardThen(() => { void reload(); })}>{draft ? 'Discard and reload' : 'Retry'}</Button></div>}
      {session.error && <div className="notice" role="status">{direct ? 'A gateway is unavailable. Check Connections or retry the affected view.' : 'Hub connection interrupted. Reconnecting automatically. Check that the hub is running.'} <small>{session.error}</small></div>}
      {sourceError && <div className="notice" role="alert">{sourceError}</div>}
      {undo && editing && <div className="notice"><span>View removed.</span><Button onClick={() => { setDraft(d => d ? { ...d, panels: undo } : d); setUndo(null); setMessage('View restored.'); focusMain(); }}>Undo removal</Button></div>}
      {narrow && editing && <div className="notice">Use Move up or Move down to reorder views. This also updates the desktop layout. Resize views on a wider screen.</div>}
      {active && <div className="board-bar"><span>{panels.length} {panels.length === 1 ? 'view' : 'views'}</span>
        {editing && <Button id="add-view" className="with-icon" disabled={busy || panels.length >= 200} onClick={openPicker}><Icon name="plus" /> Add view</Button>}</div>}
      {loading ? <div className="empty-state">Loading dashboards…</div> : !panels.length ? <div className="empty-state">
        <div className="empty-symbol"><Icon name="dashboard" /></div><h2>{active ? 'No views yet' : 'No dashboards yet'}</h2>
        <p>{board ? 'Exit board view to add views.' : 'Add views published by your apps.'}</p>
        {!board && <Button className="primary with-icon" disabled={busy || !!error} onClick={() => {
          if (!draft) startEdit(current ? structuredClone(current) : preset ? { ...structuredClone(preset), id: '', revision: 0 } : emptyDashboard());
          openPicker();
        }}><Icon name="plus" /> {active ? 'Add view' : 'Create dashboard'}</Button>}
      </div> : <div ref={grid} className={`panel-grid ${editing ? 'editing' : ''}`}>
        {panels.map((panel, panelIndex) => {
          const id = referenceId(panel.reference);
          const errorId = `${fieldId}-panel-${panelIndex}`;
          const service = services.find(s => s.id === `${panel.reference.nodeId}~${panel.reference.serviceId}`);
          const present = connected && service?.status === 'connected' && session.surfaceIds.includes(id);
          const rectangle = preview?.find(p => p.id === panel.id) ?? panel;
          return <article className="panel" key={panel.id} data-panel-id={panel.id} data-reference={id} data-surface={present ? id : undefined} style={{ gridColumn: `${rectangle.x + 1} / span ${rectangle.w}`, gridRow: `${rectangle.y + 1} / span ${rectangle.h}` }}>
            <header className="panel-header">
              {editing && !narrow && <Button static aria-label={`Move ${panel.title}`} className="drag-handle" disabled={busy}
                aria-describedby="layout-help" onKeyDown={event => { const delta = ({ ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] } as Record<string, number[]>)[event.key]; if (delta) { event.preventDefault(); changePanel({ ...panel, x: panel.x + delta[0]!, y: Math.min(100000, panel.y + delta[1]!) }); } }} onPointerDown={e => beginGesture(e, panel, 'move')} onPointerMove={moveGesture} onPointerUp={endGesture} onPointerCancel={cancelGesture} onLostPointerCapture={cancelGesture}><Icon name="grip" /></Button>}
              <div className="panel-heading">{editing ? <input aria-invalid={validated && !panel.title.trim()} aria-describedby={validated && !panel.title.trim() ? errorId : undefined} aria-label={`View name ${panel.title}`} maxLength={200} disabled={busy} value={panel.title} onChange={e => changePanel({ ...panel, title: e.target.value })} /> : <h2>{panel.title}</h2>}{editing && validated && !panel.title.trim() && <p className="field-error" id={errorId}>Enter a view name.</p>}
                <small title={`${service?.node?.name ?? panel.reference.nodeId} / ${service?.name ?? panel.reference.serviceId}`}>{service?.node?.name ?? panel.reference.nodeId} / {service?.name ?? panel.reference.serviceId}</small></div>
              <Button className="details-button" aria-label={`View details for ${panel.title}`} onClick={() => { rememberFocus(); setDetails(panel); }}>Details</Button>
              {editing ? <Button className="icon-button" aria-label={`Remove ${panel.title}`} disabled={busy} onClick={() => removePanel(panel)}><Icon name="close" /></Button>
                : <span className="view-status">{present ? '✓ Live' : '○ Unavailable'}</span>}
            </header>
            <div className="panel-content">{present ? <SurfaceView session={session} surfaceId={id} />
              : <div className="unavailable"><Icon name="unavailable" /><strong>View unavailable</strong><p>{direct ? <>Connect gateway <code>{panel.reference.nodeId}</code> in Connections and check that the source app is running.</> : 'Start the source app and check its connection to restore this view.'}{service?.detail && <small>{service.detail}</small>}</p>
                {direct && service && <Button disabled={busy || !gateways.has(panel.reference.nodeId)} onClick={() => {
                  setSourceError(null); void environment.session.retry({ service: `${panel.reference.nodeId}~${panel.reference.serviceId}` }).catch(reason => setSourceError(reason instanceof Error ? reason.message : 'This source is unavailable.'));
                }}>Retry view</Button>}</div>}</div>
            {editing && <footer className="panel-editor">
              {narrow ? <><Button disabled={busy || panels[0]?.id === panel.id} onClick={() => reorder(panel.id, -1)}>Move up</Button><Button disabled={busy || panels[panels.length - 1]?.id === panel.id} onClick={() => reorder(panel.id, 1)}>Move down</Button></> : <>
                <Button aria-expanded={adjust === panel.id} aria-controls={`adjust-${panel.id}`} disabled={busy} onClick={() => setAdjust(adjust === panel.id ? null : panel.id)}>Adjust layout</Button>
                {adjust === panel.id && <div id={`adjust-${panel.id}`} className="adjust-controls" role="group" aria-label={`Adjust ${panel.title}`}>
                  {([
                    ['Move left', { ...panel, x: panel.x - 1 }, panel.x === 0], ['Move right', { ...panel, x: panel.x + 1 }, panel.x + panel.w === 12],
                    ['Move up', { ...panel, y: panel.y - 1 }, panel.y === 0], ['Move down', { ...panel, y: panel.y + 1 }, panel.y >= 100000],
                    ['Narrower', resizePanel(panel, 'e', -1, 0), panel.w === 1], ['Wider', resizePanel(panel, 'e', 1, 0), panel.x + panel.w === 12],
                    ['Shorter', resizePanel(panel, 's', 0, -1), panel.h === 1], ['Taller', resizePanel(panel, 's', 0, 1), panel.h === 1000],
                  ] as [string, Panel, boolean][]).map(([label, next, boundary]) => <Button key={label} disabled={busy || boundary} onClick={() => changePanel(next)}>{label}</Button>)}
                </div>}
              </>}
            </footer>}
            {editing && !narrow && (['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'] as ResizeEdge[]).map(edge => <Button static key={edge} className={`resize-edge resize-${edge}`} aria-label={`Resize ${panel.title} ${edge}`} tabIndex={-1} disabled={busy}
              onPointerDown={e => beginGesture(e, panel, edge)} onPointerMove={moveGesture} onPointerUp={endGesture} onPointerCancel={cancelGesture} onLostPointerCapture={cancelGesture} />)}
          </article>;
        })}
      </div>}
      <p id="layout-help" className="sr-only">Use arrow keys on the move handle, or select Adjust layout for move and resize buttons.</p>
    </main>
    {picker && <dialog ref={pickerDialog} aria-labelledby="picker-title" className="picker" onCancel={closePicker}>
      <header><div><h2 id="picker-title">Add a view</h2></div><Button className="icon-button" aria-label="Close view picker" onClick={closePicker}><Icon name="close" /></Button></header>
      <p>{direct ? 'Choose an app to discover its views. Only the selected app is subscribed.' : 'Select a view from a connected app.'}</p><label htmlFor="search-views">Search views</label>
      <input id="search-views" autoFocus className="search" aria-label="Search views" placeholder="App or view name" value={search} onChange={e => setSearch(e.target.value)} />
      <div className="view-options">
        {groups.filter(group => direct || group.services.some(service => matches.some(id => splitSurfaceId(id).channelId === service.id))).map(group => <section key={group.node?.id ?? 'unknown'}><h3>{group.node?.name ?? 'Unknown machine'}</h3>
          {direct && <div className="service-options">{group.services.filter(service => service.node).map(service => <Button key={service.id} aria-pressed={pickerService === service.id} disabled={service.status === 'disconnected' && pickerService !== service.id}
            onClick={() => setPickerService(service.id)}>{service.name}{pickerService === service.id ? ' · Selected' : ' · Show views'}</Button>)}</div>}
          {group.services.filter(s => s.node && s.status === 'connected' && (!direct || s.id === pickerService)).map(service => {
            const ids = session.surfaceIds.filter(id => splitSurfaceId(id).channelId === service.id && `${service.name} ${splitSurfaceId(id).surfaceId}`.toLowerCase().includes(search.toLowerCase()));
            return ids.length ? <div className="app-options" key={service.id}><h4>{service.name}</h4>{ids.map(id => {
              const added = draft?.panels.some(p => referenceId(p.reference) === id);
              return <Button static key={id} disabled={added} onClick={() => addSurface(service, id)}><span><Icon name="view" /><strong>{splitSurfaceId(id).surfaceId}</strong></span><span>{added ? 'Added' : <><Icon name="plus" /> Add</>}</span></Button>;
            })}</div> : null;
          })}</section>)}
        {!matches.length && <div className="picker-empty" role="status">{direct && !pickerService ? <p>Select an app above to discover its views.</p> : search ? <><p>No views match “{search}”.</p><Button onClick={() => setSearch('')}>Clear search</Button></> : <p>{direct ? services.find(service => service.id === pickerService)?.detail ?? 'Waiting for this app’s views…' : 'No views available. Start an app and check its connection to the hub.'}</p>}</div>}
      </div>

    </dialog>}
    {importPreview && <dialog ref={importDialog} className="picker" aria-labelledby="import-title" onCancel={() => { importDialog.current?.close(); setImportPreview(null); restoreFocus(); }}>
      <h2 id="import-title">Import board as a copy</h2><p>{importPreview.input.name} · {importPreview.input.panels.length} views</p>
      <p>This creates a new local dashboard. It does not enroll or connect gateways. Sender edits or deletion do not change your copy. Deleting this board does not revoke gateway access.</p>
      <ul>{importPreview.sources.map(source => <li key={source.nodeId}>{source.name && <span>{source.name} — </span>}<code>{source.nodeId}</code>{!gateways.has(source.nodeId) && <span> · Gateway connection required</span>}</li>)}</ul>
      <div className="toolbar"><Button disabled={busy} onClick={() => { importDialog.current?.close(); setImportPreview(null); restoreFocus(); }}>Cancel import</Button><Button className="primary" disabled={busy} onClick={importCopy}>Import as new dashboard</Button></div>
    </dialog>}
    {details && <dialog ref={detailsDialog} className="picker" aria-labelledby="details-title" onCancel={() => { detailsDialog.current?.close(); setDetails(null); restoreFocus(); }}><h2 id="details-title">View details</h2><dl><dt>Name</dt><dd>{details.title}</dd><dt>Device</dt><dd>{services.find(s => s.node?.id === details.reference.nodeId)?.node?.name ?? details.reference.nodeId}</dd><dt>App</dt><dd>{services.find(s => s.id === `${details.reference.nodeId}~${details.reference.serviceId}`)?.name ?? details.reference.serviceId}</dd><dt>View ID</dt><dd>{details.reference.surfaceId}</dd></dl><Button onClick={() => { detailsDialog.current?.close(); setDetails(null); restoreFocus(); }}>Close details</Button></dialog>}
    {confirmation && <dialog ref={confirmDialog} className="picker" aria-labelledby="confirm-title" onCancel={() => { confirmDialog.current?.close(); setConfirmation(null); restoreFocus(); }}><h2 id="confirm-title">{confirmation.title}</h2><p>{confirmation.body}</p><div className="toolbar"><Button autoFocus onClick={() => { confirmDialog.current?.close(); setConfirmation(null); restoreFocus(); }}>Cancel</Button><Button className="danger" onClick={() => { const operation = confirmation.run; confirmDialog.current?.close(); setConfirmation(null); operation(); }}>{confirmation.action}</Button></div></dialog>}
  </div>;
}
