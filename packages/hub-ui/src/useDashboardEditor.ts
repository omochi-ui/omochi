import { useRef, useState } from 'react';
import type { Dashboard, Panel } from './contracts.js';
import { compactPanels } from './layout.js';
import type { TranslationMessage } from './i18n.js';
export const emptyDashboard = (name = 'Untitled dashboard'): Dashboard => ({ id: '', name, revision: 0, panels: [] });
export function useDashboardEditor() {
  const [draft, setDraft] = useState<Dashboard | null>(null);
  const baseline = useRef('');
  const [undo, setUndo] = useState<Panel[] | null>(null);
  const [validated, setValidated] = useState(false);
  const [message, setMessage] = useState<TranslationMessage | null>(null);
  const dirty = !!draft && JSON.stringify(draft) !== baseline.current;
  const begin = (value: Dashboard) => {
    baseline.current = JSON.stringify(value); setDraft({ ...value, panels: compactPanels(value.panels) }); setValidated(false); setUndo(null); setMessage(null);
  };
  return { draft, setDraft, dirty, begin, undo, setUndo, validated, setValidated, message, setMessage };
}
