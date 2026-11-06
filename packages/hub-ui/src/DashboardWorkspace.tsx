import { DashboardSettings, type BrowserIdentitySettings } from './DashboardSettings.js';
import { ConnectionMenu } from './ConnectionMenu.js';
import { localizedError } from './localized-error.js';
import { Trans, useTranslation } from 'react-i18next';
import { useEffect, useId, useLayoutEffect, useRef, useState, type PointerEvent, type ReactNode } from 'react';
import { SurfaceView, useA2uiSource } from '@omochi-ui/renderer';
import { Button } from './Button.js';
import { Icon } from './Icon.js';
import { DialogHeader, restoreDialogFocus } from './dialogs.js';
import { referenceId, type Dashboard, type Panel } from './contracts.js';
import { appendPanel, placePanel, resizePanel, reorderPanels, type ResizeEdge } from './layout.js';
import { desiredServices, groupByNode, splitSurfaceId, type ServiceView } from './services.js';
import { BOARD_FILE_LIMIT, exportBoard, parseBoard, type SourceLabel } from './board-codec.js';
import { SessionFault } from '@omochi-ui/core/browser';
import { BoardFault } from './board-repository.js';
import type { DashboardEnvironment } from './environment.js';
import { emptyDashboard, useDashboardEditor } from './useDashboardEditor.js';
import { useDashboards } from './useDashboards.js';
import { useDashboardNavigation } from './DashboardNavigation.js';

export function DashboardWorkspace({ environment, connectionUi, connectionNotice, connectionName, browserIdentity, renderSharing }: { environment: DashboardEnvironment; connectionUi?: ReactNode; connectionNotice?: ReactNode; connectionName?: string; browserIdentity?: BrowserIdentitySettings; renderSharing?: (board: Dashboard) => ReactNode }) {
  const { t } = useTranslation();
  const session = useA2uiSource(environment.session);
  const services: readonly ServiceView[] = session.services;
  const direct = environment.session.imagePolicy === 'sdk-png';
  const shared = environment.authority.kind === 'share' ? environment.authority.permissions : undefined;
  const canEdit = !shared || shared.editLayout;
  const canInteract = !shared || shared.interact;
  const [gateways, setGateways] = useState<ReadonlySet<string>>(new Set());
  const { dashboards, setDashboards, presets, selected, setSelected, loading, error, setError, reload } = useDashboards(environment.boards);
  const { draft, setDraft, dirty, begin, undo, setUndo, validated, setValidated, message, setMessage } = useDashboardEditor();
  const navigation = useDashboardNavigation();
  const [busy, setBusy] = useState(false);
  const [picker, setPicker] = useState(false);
  const [search, setSearch] = useState('');
  const [pickerService, setPickerService] = useState<string | null>(null);
  const [sourceError, setSourceError] = useState<Error | null>(null);
  const [importError, setImportError] = useState<'invalidFile' | 'readError' | null>(null);
  const [importPreview, setImportPreview] = useState<ReturnType<typeof parseBoard> | null>(null);
  const importDialog = useRef<HTMLDialogElement>(null);
  const importFile = useRef<HTMLInputElement>(null);
  const fieldId = useId();
  const [narrow, setNarrow] = useState(() => matchMedia('(max-width: 760px)').matches);
  const grid = useRef<HTMLDivElement>(null);
  const pickerDialog = useRef<HTMLDialogElement>(null);
  const [board, setBoard] = useState(() => new URLSearchParams(location.search).get('view') === 'board');
  const [navOpen, setNavOpen] = useState(false);
  const [settingsTrigger, setSettingsTrigger] = useState<HTMLElement | null>(null);
  const [adjust, setAdjust] = useState<string | null>(null);
  const [details, setDetails] = useState<Panel | null>(null);
  const [confirmation, setConfirmation] = useState<{ title: 'confirmation.discardTitle' | 'confirmation.deleteTitle'; name?: string; action: 'confirmation.discard' | 'workspace.delete'; body: 'confirmation.discardBody' | 'confirmation.deleteBody'; run: () => void } | null>(null);
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
  const connected = session.status === 'Connected';

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
      if (!(reason && typeof reason === 'object' && 'code' in reason && reason.code === 'InvalidState')) setSourceError(reason instanceof Error ? reason : new SessionFault('SourceUnavailable'));
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
  useEffect(() => { document.title = `${active?.name ?? t('workspace.title')} · omochi`; }, [active?.name, t]);
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
    if (!dirty && !busy) return;
    const prevent = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = ''; };
    window.addEventListener('beforeunload', prevent);
    return () => window.removeEventListener('beforeunload', prevent);
  }, [dirty, busy]);

  const focusMain = () => requestAnimationFrame(() => document.querySelector<HTMLElement>('#workspace')?.focus());
  const restoreFocus = () => restoreDialogFocus(returnFocus.current);
  const rememberFocus = () => { returnFocus.current = document.activeElement as HTMLElement; };
  const startEdit = (value: Dashboard) => {
    if (!canEdit) return;
    begin(value); setError(null); setImportError(null);
    requestAnimationFrame(() => { if (!document.querySelector('dialog[open]')) document.querySelector<HTMLInputElement>('.dashboard-name')?.focus(); });
  };
  const clearDraft = () => { setDraft(null); setPicker(false); setUndo(null); setAdjust(null); setValidated(false); setError(null); setImportError(null); };
  const pendingLeave = useRef<((allowed: boolean) => void) | null>(null);
  const requestLeave = async (): Promise<boolean> => {
    if (busy || pendingLeave.current) return false;
    if (!dirty) { clearDraft(); return true; }
    rememberFocus();
    return new Promise(resolve => {
      pendingLeave.current = resolve;
      setConfirmation({ title: 'confirmation.discardTitle', body: 'confirmation.discardBody', action: 'confirmation.discard', run: () => {
        pendingLeave.current = null; clearDraft(); resolve(true);
      } });
    });
  };
  useLayoutEffect(() => navigation.register(requestLeave));
  useEffect(() => () => { pendingLeave.current?.(false); }, []);
  const discardThen = (operation: () => void) => { void requestLeave().then(allowed => { if (allowed) operation(); }); };
  const switchDashboard = (id: string) => discardThen(() => { setSelected(id); setNavOpen(false); focusMain(); });
  const closePicker = () => { pickerDialog.current?.close(); setPicker(false); restoreFocus(); };
  const openPicker = () => { returnFocus.current = document.getElementById('add-view') ?? document.activeElement as HTMLElement; setSearch(''); setPickerService(null); setPicker(true); };
  const run = async (operation: () => Promise<void>) => {
    setBusy(true); setError(null); setImportError(null);
    try { await operation(); }
    catch (reason) { setError(reason instanceof Error ? reason : new BoardFault('StorageUnavailable')); }
    finally { setBusy(false); }
  };
  const save = () => {
    if (!draft || !canEdit) return;
    setValidated(true);
    if (!draft.name.trim() || draft.panels.some(p => !p.title.trim())) {
      requestAnimationFrame(() => document.querySelector<HTMLInputElement>('[aria-invalid="true"]')?.focus()); return;
    }
    setUndo(null);
    void run(async () => {
      const input = { name: draft.name, panels: draft.panels };
      const saved = await (draft.id ? environment.boards.update(draft.id, draft.revision, input) : environment.boards.create(input));
      setDashboards(all => [...all.filter(d => d.id !== saved.id), saved]);
      setSelected(`dashboard:${saved.id}`); clearDraft(); setMessage({ key: 'announcement.saved' }); focusMain();
    });
  };
  const remove = () => {
    if (!current) return;
    rememberFocus();
    setConfirmation({ title: 'confirmation.deleteTitle', name: current.name, body: 'confirmation.deleteBody', action: 'workspace.delete', run: () => {
      void run(async () => {
        await environment.boards.delete(current.id, current.revision);
        const next = dashboards.filter(d => d.id !== current.id);
        setDashboards(next); setSelected(next[0] ? `dashboard:${next[0].id}` : presets[0] ? `preset:${presets[0].id}` : '');
        setMessage({ key: 'announcement.deleted' }); focusMain();
      });
    } });
  };
  const readImport = async (file: File | undefined) => {
    if (!file) return;
    rememberFocus();
    setBusy(true); setImportError(null);
    try {
      if (file.size > BOARD_FILE_LIMIT) throw new BoardFault('InvalidBoard');
      setImportPreview(parseBoard(new Uint8Array(await file.arrayBuffer())));
    } catch (reason) {
      setImportError(reason instanceof BoardFault && reason.code === 'InvalidBoard' ? 'invalidFile' : 'readError');
    } finally {
      setBusy(false);
      if (importFile.current) importFile.current.value = '';
    }
  };
  const importCopy = () => {
    if (!importPreview) return;
    void run(async () => {
      const saved = await environment.boards.create(importPreview.input);
      setDashboards(all => [...all.filter(board => board.id !== saved.id), saved]);
      setSelected(`dashboard:${saved.id}`); importDialog.current?.close(); setImportPreview(null);
      setMessage({ key: 'announcement.imported' }); focusMain();
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
      setMessage({ key: 'announcement.exported' });
    });
  };
  const changePanel = (panel: Panel) => {
    setUndo(null);
    setDraft(d => d ? { ...d, panels: placePanel(d.panels, panel) } : d);
    setMessage({ key: 'announcement.updated', values: { name: panel.title } });
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
      setDraft(d => d ? { ...d, panels } : d); setMessage({ key: 'announcement.layout' });
    }
    cancelGesture();
  };
  const reorder = (id: string, direction: -1 | 1) => {
    if (!draft) return;
    const panels = reorderPanels(draft.panels, id, direction);
    if (panels === draft.panels) return;
    setUndo(null); setDraft({ ...draft, panels });
    setMessage({ key: direction < 0 ? 'announcement.up' : 'announcement.down' });
  };
  const removePanel = (panel: Panel) => {
    if (!draft) return;
    setUndo(draft.panels); setDraft({ ...draft, panels: draft.panels.filter(p => p.id !== panel.id) }); setAdjust(null);
    setMessage({ key: 'announcement.removed', values: { name: panel.title } });
    requestAnimationFrame(() => (document.querySelector<HTMLButtonElement>('.panel .details-button') ?? document.querySelector<HTMLButtonElement>('#add-view'))?.focus());
  };
  const addSurface = (service: ServiceView, id: string) => {
    if (!service.node || !draft) return;
    const surface = splitSurfaceId(id);
    const reference = { nodeId: service.node.id, serviceId: service.id.slice(service.node.id.length + 1), surfaceId: surface.surfaceId };
    if (draft.panels.some(p => referenceId(p.reference) === id)) return;
    const panel: Panel = { id: crypto.randomUUID(), title: `${service.name} · ${surface.surfaceId}`.slice(0, 200), reference,
      x: 0, y: 0, w: 6, h: 6 };
    const panels = appendPanel(draft.panels, panel);
    if (panels === draft.panels) return;
    setUndo(null); setDraft({ ...draft, panels });
    closePicker(); setMessage({ key: 'announcement.added' });
  };
  const panels = [...(active?.panels ?? [])].sort((a, b) => a.y - b.y || a.x - b.x);
  const groups = groupByNode(services);
  const matches = services.flatMap(service => service.node && service.status === 'connected' && (!direct || service.id === pickerService) ? session.surfaceIds.filter(id => splitSurfaceId(id).channelId === service.id && `${service.name} ${splitSurfaceId(id).surfaceId}`.toLowerCase().includes(search.toLowerCase())) : []);
  const available = services.filter(s => s.status === 'connected').length;

  const statuses = new Map([
    ['Connected', t('status.Connected')], ['Connecting', t('status.Connecting')],
    ['Disconnected', t('status.Disconnected')], ['Reconnecting', t('status.Reconnecting')],
    ['Idle', t('status.Idle')], ['Stopped', t('status.Stopped')], ['Error', t('status.Error')],
  ]);
  const connectionStatus = statuses.get(session.status.replace(/…$/, '')) ?? session.status;

  return <div className={`hub-layout ${board ? 'board-only' : ''} ${navOpen ? 'nav-open' : ''}`} onKeyDown={event => { if (event.key === 'Escape' && gesture.current) { event.preventDefault(); cancelGesture(); } }}>
    <a className="skip-link" href="#workspace">{t('workspace.skip')}</a>
    <Button className="mobile-nav" aria-expanded={navOpen} aria-controls="dashboard-navigation" onClick={() => setNavOpen(!navOpen)}>{t('workspace.title')}</Button>
    <aside id="dashboard-navigation" className="sidebar">
      <a className="brand" href="./" onClick={event => { if (dirty || busy) { event.preventDefault(); discardThen(() => { location.href = './'; }); } }}><svg className="brand-mark" viewBox="0 0 96 96" aria-hidden="true"><path fill="currentColor" fillRule="evenodd" d="M32 12h20q24 0 24 24v-2h1a13 13 0 0 1 13 13v2a13 13 0 0 1-13 13h-1q-1 22-24 22H32Q8 84 8 60V36Q8 12 32 12Zm27 36a17 17 0 1 0-34 0 17 17 0 0 0 34 0Z" /></svg> omochi <span className="brand-tag">hub</span></a>
      <div className="nav-caption">{t('workspace.caption')}</div>
      {!!presets.length && <><div className="nav-section"><Icon name="preset" /> {t('workspace.presets')} <span className="count">{presets.length}</span></div>
      <nav aria-label={t('workspace.presets')}>
        {presets.map(p => <Button static key={p.id} className={`dashboard-link ${selected === `preset:${p.id}` ? 'selected' : ''}`} aria-current={selected === `preset:${p.id}` ? 'page' : undefined} disabled={busy}
          onClick={() => switchDashboard(`preset:${p.id}`)}>{p.name}</Button>)}
      </nav></>}
      <div className="nav-section"><Icon name="dashboard" /> {t('workspace.title')} <span className="count">{dashboards.length}</span></div>
      <nav aria-label={t('workspace.title')}>
        {dashboards.map(d => <Button static key={d.id} className={`dashboard-link ${selected === `dashboard:${d.id}` ? 'selected' : ''}`} aria-current={selected === `dashboard:${d.id}` ? 'page' : undefined} disabled={busy}
          onClick={() => switchDashboard(`dashboard:${d.id}`)}>{d.name}</Button>)}
        {!dashboards.length && <p className="sidebar-hint">{t('workspace.emptyNav')}</p>}
      </nav>
      <Button hidden={!!shared} className="new-dashboard with-icon" disabled={loading || busy} onClick={() => discardThen(() => startEdit(emptyDashboard(t('workspace.untitled'))))}><Icon name="plus" /> {t('workspace.create')}</Button>
      <Button hidden={!!shared} className="new-dashboard" disabled={loading || busy || editing} onClick={() => { rememberFocus(); importFile.current?.click(); }}>{t('workspace.import')}</Button>
      <input ref={importFile} id="import-board-file" type="file" accept="application/json,.json" hidden onChange={event => { void readImport(event.target.files?.[0]); }} />
      <div className="sidebar-footer"><Button className="settings-button dashboard-link" aria-haspopup="dialog" onClick={event => {
        setSettingsTrigger(event.currentTarget);
        if (browserIdentity && !browserIdentity.id) browserIdentity.onLoad();
      }}>{t('settings.title')}</Button></div>
    </aside>
    <main id="workspace" tabIndex={-1} className="workspace">
      <div className="workspace-status"><div className="workspace-connection"><ConnectionMenu name={connectionName || t(direct ? 'workspace.gateways' : 'workspace.localHub')} status={session.status.replace(/…$/, '')} statusLabel={connectionStatus} available={available} readOnly={board}>{connectionUi}</ConnectionMenu>
      {board && <Button className="exit-board" onClick={() => { setBoard(false); requestAnimationFrame(() => { if (boardTrigger.current) boardTrigger.current.focus(); else focusMain(); }); }}>{t('workspace.exitBoard')}</Button>}</div></div>

      <header className="page-header">
        <div className="header-copy">
          {editing ? <><label htmlFor="dashboard-name">{t('workspace.name')}</label><input id="dashboard-name" aria-invalid={validated && !draft.name.trim()} aria-describedby={validated && !draft.name.trim() ? 'name-error' : undefined} className="dashboard-name" aria-label={t('workspace.name')} maxLength={200} value={draft.name} disabled={busy} onChange={e => setDraft({ ...draft, name: e.target.value })} />{validated && !draft.name.trim() && <p id="name-error" className="field-error">{t('workspace.nameRequired')}</p>}</>
            : <h1>{active?.name ?? t('workspace.title')}</h1>}
          <p className="subtitle">{editing ? dirty ? t('workspace.unsaved') : t('workspace.editing') : preset ? t('workspace.presetHint') : ''}</p>
        </div>
        <div className="toolbar">{editing ? <>
          <Button disabled={busy} onClick={() => discardThen(focusMain)}>{t('common.cancel')}</Button>
          <Button className="primary" disabled={busy} onClick={save}>{busy ? t('workspace.saving') : t('workspace.save')}</Button>
        </> : current ? <>
          <Button hidden={!!shared} disabled={busy} className="danger" onClick={remove}>{t('workspace.delete')}</Button>
          <Button hidden={!canEdit} className="primary" disabled={busy} onClick={() => startEdit(structuredClone(current))}>{t('workspace.edit')}</Button>{!shared && renderSharing?.(current)}
        </> : preset ? <Button className="primary" disabled={busy} onClick={() => startEdit({ ...structuredClone(preset), id: '', revision: 0 })}>{t('workspace.saveCopy')}</Button> : null}{!editing && active && <><Button disabled={busy} onClick={download}>{t('workspace.export')}</Button><Button ref={boardTrigger} disabled={busy} onClick={() => { setBoard(true); requestAnimationFrame(() => document.querySelector<HTMLButtonElement>('.exit-board')?.focus()); }}>{t('workspace.boardView')}</Button></>}</div>
      </header>
      {shared && <p hidden={board} className="notice" role="status">{canInteract ? t('workspace.interact') : t('workspace.readonly')} {shared.editLayout ? t('workspace.sharedEdit') : ''}</p>}
      <div hidden={board} className="announcement" role="status">{message && t(message.key, { ...message.values })}</div>
      {importError && <div className="notice error import-error" role="alert"><span>{t(`import.${importError}`)}</span><Button disabled={busy || editing} onClick={() => { rememberFocus(); importFile.current?.click(); }}>{t('import.chooseFile')}</Button></div>}
      {error && <div className="notice error" role="alert"><span>{draft ? t('workspace.saveError') : t('workspace.loadError')} <small>{draft && error instanceof BoardFault && error.code === 'InvalidBoard' ? t('workspace.invalidChanges') : localizedError(error, t)}</small></span>{draft && <Button disabled={busy} onClick={save}>{t('workspace.retrySave')}</Button>}<Button disabled={busy} onClick={() => discardThen(() => { void reload(); })}>{draft ? t('workspace.reload') : t('common.retry')}</Button></div>}
      {connectionNotice}
      {session.error && <div className="notice" role="status" data-error-code={session.errorCode ?? undefined}>{session.errorCode ? t(`sessionError.${session.errorCode}`) : <>{shared ? t('workspace.shareInterrupted') : direct ? t('workspace.gatewayUnavailable') : t('workspace.hubInterrupted')} <small>{session.error}</small></>}</div>}
      {sourceError && <div className="notice" role="alert">{localizedError(sourceError, t)}</div>}
      {undo && editing && <div className="notice"><span>{t('workspace.removed')}</span><Button onClick={() => { setDraft(d => d ? { ...d, panels: undo } : d); setUndo(null); setMessage({ key: 'workspace.restored' }); focusMain(); }}>{t('workspace.undo')}</Button></div>}
      {narrow && editing && <div className="notice">{t('workspace.mobileHelp')}</div>}
      {active && <div className="board-bar"><span>{t('workspace.views', { count: panels.length })}</span>
        {editing && !shared && <Button id="add-view" className="with-icon" disabled={busy || panels.length >= 200} onClick={openPicker}><Icon name="plus" /> {t('view.add')}</Button>}</div>}
      {loading ? <div className="empty-state">{t('workspace.loading')}</div> : !panels.length ? <div className="empty-state">
        <div className="empty-symbol"><Icon name="dashboard" /></div><h2>{active ? t('workspace.noViews') : t('workspace.noDashboards')}</h2>
        <p>{shared ? t('workspace.emptyShared') : board ? t('workspace.exitToAdd') : t('workspace.addHint')}</p>
        {!board && !shared && <Button className="primary with-icon" disabled={busy || !!error} onClick={() => {
          if (!draft) startEdit(current ? structuredClone(current) : preset ? { ...structuredClone(preset), id: '', revision: 0 } : emptyDashboard(t('workspace.untitled')));
          openPicker();
        }}><Icon name="plus" /> {active ? t('view.add') : t('workspace.create')}</Button>}
      </div> : <div ref={grid} className={`panel-grid ${editing ? 'editing' : ''}`}>
        {panels.map((panel, panelIndex) => {
          const id = referenceId(panel.reference);
          const errorId = `${fieldId}-panel-${panelIndex}`;
          const service = services.find(s => s.id === `${panel.reference.nodeId}~${panel.reference.serviceId}`);
          const present = connected && service?.status === 'connected' && session.surfaceIds.includes(id);
          const sourceName = [service?.node?.name !== panel.reference.nodeId ? service?.node?.name : undefined,
            service?.name !== panel.reference.serviceId ? service?.name : undefined].filter(Boolean).join(' / ') || t('view.sourceUnknown');
          const rectangle = preview?.find(p => p.id === panel.id) ?? panel;
          return <article className="panel" key={panel.id} data-panel-id={panel.id} data-reference={id} data-surface={present ? id : undefined} style={{ gridColumn: `${rectangle.x + 1} / span ${rectangle.w}`, gridRow: `${rectangle.y + 1} / span ${rectangle.h}` }}>
            <header className="panel-header">
              {editing && !narrow && <Button static aria-label={t('view.moveLabel', { name: panel.title })} className="drag-handle" disabled={busy}
                aria-describedby="layout-help" onKeyDown={event => { const delta = ({ ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] } as Record<string, number[]>)[event.key]; if (delta) { event.preventDefault(); changePanel({ ...panel, x: panel.x + delta[0]!, y: Math.min(100000, panel.y + delta[1]!) }); } }} onPointerDown={e => beginGesture(e, panel, 'move')} onPointerMove={moveGesture} onPointerUp={endGesture} onPointerCancel={cancelGesture} onLostPointerCapture={cancelGesture}><Icon name="grip" /></Button>}
              <div className="panel-heading">{editing ? <input aria-invalid={validated && !panel.title.trim()} aria-describedby={validated && !panel.title.trim() ? errorId : undefined} aria-label={t('view.nameLabel', { name: panel.title })} maxLength={200} disabled={busy} value={panel.title} onChange={e => changePanel({ ...panel, title: e.target.value })} /> : <h2>{panel.title}</h2>}{editing && validated && !panel.title.trim() && <p className="field-error" id={errorId}>{t('view.nameRequired')}</p>}
                <small>{sourceName}</small></div>
              <Button className="details-button" aria-label={t('view.detailsLabel', { name: panel.title })} onClick={() => { rememberFocus(); setDetails(panel); }}>{t('view.details')}</Button>
              {editing && !shared ? <Button className="icon-button" aria-label={t('view.removeLabel', { name: panel.title })} disabled={busy} onClick={() => removePanel(panel)}><Icon name="close" /></Button>
                : <span className="view-status">{present ? t('view.live') : t('view.offline')}</span>}
            </header>
            <div className="panel-content">{present ? <fieldset className="shared-surface" disabled={!canInteract} onClickCapture={event => { if (!canInteract) { event.preventDefault(); event.stopPropagation(); } }}><SurfaceView session={session} surfaceId={id} /></fieldset>
              : <div className="unavailable"><Icon name="unavailable" /><strong>{t('view.unavailable')}</strong><p>{direct ? <Trans i18nKey="view.connectGateway" values={{ nodeId: panel.reference.nodeId }} components={{ gateway: <code /> }} /> : t('view.restore')}{service?.detail && <small>{service.detail}</small>}</p>
                {direct && service && <Button disabled={busy || !gateways.has(panel.reference.nodeId)} onClick={() => {
                  setSourceError(null); void environment.session.retry({ service: `${panel.reference.nodeId}~${panel.reference.serviceId}` }).catch(reason => setSourceError(reason instanceof Error ? reason : new SessionFault('SourceUnavailable')));
                }}>{t('view.retry')}</Button>}</div>}</div>
            {editing && <footer className="panel-editor">
              {narrow ? <><Button disabled={busy || panels[0]?.id === panel.id} onClick={() => reorder(panel.id, -1)}>{t('view.up')}</Button><Button disabled={busy || panels[panels.length - 1]?.id === panel.id} onClick={() => reorder(panel.id, 1)}>{t('view.down')}</Button></> : <>
                <Button aria-expanded={adjust === panel.id} aria-controls={`adjust-${panel.id}`} disabled={busy} onClick={() => setAdjust(adjust === panel.id ? null : panel.id)}>{t('view.adjust')}</Button>
                {adjust === panel.id && <div id={`adjust-${panel.id}`} className="adjust-controls" role="group" aria-label={t('view.adjustLabel', { name: panel.title })}>
                  {([
                    [t('view.left'), { ...panel, x: panel.x - 1 }, panel.x === 0], [t('view.right'), { ...panel, x: panel.x + 1 }, panel.x + panel.w === 12],
                    [t('view.up'), { ...panel, y: panel.y - 1 }, panel.y === 0], [t('view.down'), { ...panel, y: panel.y + 1 }, panel.y >= 100000],
                    [t('view.narrower'), resizePanel(panel, 'e', -1, 0), panel.w === 1], [t('view.wider'), resizePanel(panel, 'e', 1, 0), panel.x + panel.w === 12],
                    [t('view.shorter'), resizePanel(panel, 's', 0, -1), panel.h === 1], [t('view.taller'), resizePanel(panel, 's', 0, 1), panel.h === 1000],
                  ] as [string, Panel, boolean][]).map(([label, next, boundary]) => <Button key={label} disabled={busy || boundary} onClick={() => changePanel(next)}>{label}</Button>)}
                </div>}
              </>}
            </footer>}
            {editing && !narrow && (['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'] as ResizeEdge[]).map(edge => <Button static key={edge} className={`resize-edge resize-${edge}`} aria-label={t('view.resizeLabel', { name: panel.title, edge: t(`edge.${edge}`) })} tabIndex={-1} disabled={busy}
              onPointerDown={e => beginGesture(e, panel, edge)} onPointerMove={moveGesture} onPointerUp={endGesture} onPointerCancel={cancelGesture} onLostPointerCapture={cancelGesture} />)}
          </article>;
        })}
      </div>}
      <p id="layout-help" className="sr-only">{t('view.keyboardHelp')}</p>
    </main>
    <DashboardSettings returnTo={settingsTrigger} onClose={() => setSettingsTrigger(null)} browserIdentity={browserIdentity} />
    {picker && <dialog ref={pickerDialog} aria-labelledby="picker-title" className="picker" onCancel={closePicker}>
      <DialogHeader id="picker-title" title={t('picker.title')} onClose={closePicker} />
      <p>{direct ? t('picker.chooseApp') : t('picker.chooseView')}</p><label htmlFor="search-views">{t('picker.search')}</label>
      <input id="search-views" autoFocus className="search" aria-label={t('picker.search')} placeholder={t('picker.placeholder')} value={search} onChange={e => setSearch(e.target.value)} />
      <div className="view-options">
        {groups.filter(group => direct || group.services.some(service => matches.some(id => splitSurfaceId(id).channelId === service.id))).map(group => <section key={group.node?.id ?? 'unknown'}><h3>{group.node?.name ?? t('picker.unknown')}</h3>
          {direct && <div className="service-options">{group.services.filter(service => service.node).map(service => <Button key={service.id} aria-pressed={pickerService === service.id} disabled={service.status === 'disconnected' && pickerService !== service.id}
            onClick={() => setPickerService(service.id)}>{service.name}{pickerService === service.id ? t('picker.selected') : t('picker.show')}</Button>)}</div>}
          {group.services.filter(s => s.node && s.status === 'connected' && (!direct || s.id === pickerService)).map(service => {
            const ids = session.surfaceIds.filter(id => splitSurfaceId(id).channelId === service.id && `${service.name} ${splitSurfaceId(id).surfaceId}`.toLowerCase().includes(search.toLowerCase()));
            return ids.length ? <div className="app-options" key={service.id}><h4>{service.name}</h4>{ids.map(id => {
              const added = draft?.panels.some(p => referenceId(p.reference) === id);
              return <Button static key={id} disabled={added} onClick={() => addSurface(service, id)}><span><Icon name="view" /><strong>{splitSurfaceId(id).surfaceId}</strong></span><span>{added ? t('picker.added') : <><Icon name="plus" /> {t('picker.add')}</>}</span></Button>;
            })}</div> : null;
          })}</section>)}
        {!matches.length && <div className="picker-empty" role="status">{direct && !pickerService ? <p>{t('picker.selectApp')}</p> : search ? <><p>{t('picker.noMatches', { search })}</p><Button onClick={() => setSearch('')}>{t('picker.clear')}</Button></> : <p>{direct ? services.find(service => service.id === pickerService)?.detail ?? t('picker.waiting') : t('picker.empty')}</p>}</div>}
      </div>

    </dialog>}
    {importPreview && <dialog ref={importDialog} className="picker" aria-labelledby="import-title" onCancel={() => { importDialog.current?.close(); setImportPreview(null); restoreFocus(); }}>
      <DialogHeader id="import-title" title={t('import.title')} disabled={busy} onClose={() => { importDialog.current?.close(); setImportPreview(null); restoreFocus(); }} /><p>{importPreview.input.name} · {t('workspace.views', { count: importPreview.input.panels.length })}</p>
      <p>{t('import.description')}</p>
      <ul>{importPreview.sources.map(source => <li key={source.nodeId}>{source.name && <span>{source.name} — </span>}<code>{source.nodeId}</code>{!gateways.has(source.nodeId) && <span>{t('import.gatewayRequired')}</span>}</li>)}</ul>
      <div className="toolbar"><Button disabled={busy} onClick={() => { importDialog.current?.close(); setImportPreview(null); restoreFocus(); }}>{t('import.cancel')}</Button><Button className="primary" disabled={busy} onClick={importCopy}>{t('import.confirm')}</Button></div>
    </dialog>}
    {details && <dialog ref={detailsDialog} className="picker" aria-labelledby="details-title" onCancel={() => { detailsDialog.current?.close(); setDetails(null); restoreFocus(); }}><DialogHeader id="details-title" title={t('view.detailsTitle')} onClose={() => { detailsDialog.current?.close(); setDetails(null); restoreFocus(); }} /><dl><dt>{t('common.name')}</dt><dd>{details.title}</dd><dt>{t('common.device')}</dt><dd>{services.find(s => s.node?.id === details.reference.nodeId)?.node?.name ?? details.reference.nodeId}</dd><dt>{t('common.app')}</dt><dd>{services.find(s => s.id === `${details.reference.nodeId}~${details.reference.serviceId}`)?.name ?? details.reference.serviceId}</dd><dt>{t('common.viewId')}</dt><dd>{details.reference.surfaceId}</dd></dl></dialog>}
    {confirmation && <dialog ref={confirmDialog} className="picker" aria-labelledby="confirm-title" onCancel={() => { confirmDialog.current?.close(); setConfirmation(null); pendingLeave.current?.(false); pendingLeave.current = null; restoreFocus(); }}><DialogHeader id="confirm-title" title={t(confirmation.title, { name: confirmation.name ?? '' })} onClose={() => { confirmDialog.current?.close(); setConfirmation(null); pendingLeave.current?.(false); pendingLeave.current = null; restoreFocus(); }} /><p>{t(confirmation.body)}</p><div className="toolbar"><Button autoFocus onClick={() => { confirmDialog.current?.close(); setConfirmation(null); pendingLeave.current?.(false); pendingLeave.current = null; restoreFocus(); }}>{t('common.cancel')}</Button><Button className="danger" onClick={() => { const operation = confirmation.run; confirmDialog.current?.close(); setConfirmation(null); operation(); }}>{t(confirmation.action)}</Button></div></dialog>}
  </div>;
}
