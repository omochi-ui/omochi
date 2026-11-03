import { useRef, useState } from 'react';
import type { Dashboard, Panel } from './contracts.js';
export const emptyDashboard = (): Dashboard => ({ id: '', name: 'Untitled dashboard', revision: 0, panels: [] });
export function useDashboardEditor() {
  const [draft, setDraft] = useState<Dashboard | null>(null);
  const baseline = useRef('');
  const [undo, setUndo] = useState<Panel[] | null>(null);
  const [validated, setValidated] = useState(false);
  const [message, setMessage] = useState('');
  const dirty = !!draft && JSON.stringify(draft) !== baseline.current;
  const begin = (value: Dashboard) => {
    baseline.current = JSON.stringify(value); setDraft(value); setValidated(false); setUndo(null); setMessage('');
  };
  return { draft, setDraft, dirty, begin, undo, setUndo, validated, setValidated, message, setMessage };
}
