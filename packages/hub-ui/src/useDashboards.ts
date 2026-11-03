import { useCallback, useEffect, useRef, useState } from 'react';
import type { Dashboard, DashboardPreset } from './contracts.js';
import { BoardFault, type BoardRepository } from './board-repository.js';
export function useDashboards(boards: BoardRepository) {
  const [dashboards, setDashboards] = useState<readonly Dashboard[]>([]);
  const [presets, setPresets] = useState<readonly DashboardPreset[]>([]);
  const [selected, setSelected] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const reloadSequence = useRef(0);
  const reload = useCallback(async () => {
    const sequence = ++reloadSequence.current;
    setLoading(true);
    try {
      const [nextDashboards, nextPresets] = await Promise.all([
        boards.list(), boards.presets(),
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
  }, [boards]);
  useEffect(() => {
    void reload(); const unsubscribe = boards.subscribe(() => { void reload(); });
    return () => { reloadSequence.current++; unsubscribe(); };
  }, [boards, reload]);
  return { dashboards, setDashboards, presets, selected, setSelected, loading, error, setError, reload };
}
