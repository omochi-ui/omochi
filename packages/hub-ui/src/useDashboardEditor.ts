import { useEffect, useRef, useState } from 'react';
import { referenceId, type Dashboard, type Panel } from './contracts.js';
import { placePanel, reorderPanels } from './layout.js';
import { splitSurfaceId, type ServiceView } from './services.js';

export const emptyDashboard = (): Dashboard => ({
  id: '',
  name: 'Untitled dashboard',
  revision: 0,
  panels: [],
});

export function useDashboardEditor() {
  const [draft, setDraft] = useState<Dashboard | null>(null);
  const baseline = useRef('');
  const [validated, setValidated] = useState(false);
  const [undo, setUndo] = useState<Panel[] | null>(null);
  const [message, announce] = useState('');
  const dirty = !!draft && JSON.stringify(draft) !== baseline.current;
  useEffect(() => {
    if (!dirty) return;
    const prevent = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = '';
    };
    window.addEventListener('beforeunload', prevent);
    return () => window.removeEventListener('beforeunload', prevent);
  }, [dirty]);
  const startEdit = (value: Dashboard) => {
    baseline.current = JSON.stringify(value);
    setDraft(value);
    setValidated(false);
    setUndo(null);
    announce('');
    requestAnimationFrame(() => {
      if (!document.querySelector('dialog[open]'))
        document.querySelector<HTMLInputElement>('.dashboard-name')?.focus();
    });
  };
  const clear = () => {
    setDraft(null);
    setUndo(null);
    setValidated(false);
  };
  const validate = () => {
    setValidated(true);
    const valid = !!draft && !!draft.name.trim() && draft.panels.every((p) => p.title.trim());
    if (!valid)
      requestAnimationFrame(() => document.querySelector<HTMLInputElement>('[aria-invalid="true"]')?.focus());
    if (valid) setUndo(null);
    return valid;
  };
  const changePanel = (panel: Panel) => {
    setUndo(null);
    setDraft((d) => (d ? { ...d, panels: placePanel(d.panels, panel) } : d));
    announce(`Updated ${panel.title}.`);
  };
  const reorder = (id: string, direction: -1 | 1) => {
    setUndo(null);
    setDraft((d) => (d ? { ...d, panels: reorderPanels(d.panels, id, direction) } : d));
    announce(direction < 0 ? 'View moved up.' : 'View moved down.');
  };
  const removePanel = (panel: Panel) => {
    if (!draft) return;
    setUndo(draft.panels);
    setDraft({ ...draft, panels: draft.panels.filter((p) => p.id !== panel.id) });
    announce(`${panel.title} removed.`);
  };
  const addSurface = (service: ServiceView, id: string) => {
    if (!service.node || !draft) return;
    const surface = splitSurfaceId(id);
    const reference = {
      nodeId: service.node.id,
      serviceId: service.id.slice(service.node.id.length + 1),
      surfaceId: surface.surfaceId,
    };
    if (draft.panels.some((p) => referenceId(p.reference) === id)) return;
    const panel: Panel = {
      id: crypto.randomUUID(),
      title: `${service.name} · ${surface.surfaceId}`,
      reference,
      x: 0,
      y: Math.max(0, ...draft.panels.map((p) => p.y + p.h)),
      w: 6,
      h: 6,
    };
    setUndo(null);
    setDraft({ ...draft, panels: [...draft.panels, panel] });
    announce('View added.');
  };
  return {
    draft,
    dirty,
    validated,
    undo,
    message,
    announce,
    startEdit,
    clear,
    validate,
    rename: (name: string) => setDraft((d) => (d ? { ...d, name } : d)),
    changePanel,
    reorder,
    removePanel,
    addSurface,
    beginLayout: () => setUndo(null),
    commitLayout: (panels: Panel[]) => {
      setDraft((d) => (d ? { ...d, panels } : d));
      announce('Layout updated.');
    },
    restorePanel: () => {
      setDraft((d) => (d && undo ? { ...d, panels: undo } : d));
      setUndo(null);
      announce('View restored.');
    },
  };
}
export type DashboardEditor = ReturnType<typeof useDashboardEditor>;
