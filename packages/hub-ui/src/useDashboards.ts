import { useEffect, useState } from 'react';
import type { Dashboard, DashboardPreset } from './contracts.js';
import { hubClient } from './hubClient.js';

export function useDashboards() {
  const [dashboards, setDashboards] = useState<Dashboard[]>([]);
  const [presets, setPresets] = useState<DashboardPreset[]>([]);
  const [selected, select] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const reload = async () => {
    setLoading(true);
    try {
      const [nextDashboards, nextPresets] = await Promise.all([hubClient.list(), hubClient.presets()]);
      setDashboards(nextDashboards);
      setPresets(nextPresets);
      select((id) => {
        if (
          nextDashboards.some((d) => `dashboard:${d.id}` === id) ||
          nextPresets.some((p) => `preset:${p.id}` === id)
        )
          return id;
        const search = new URLSearchParams(location.search);
        const requestedPreset = search.get('preset');
        if (requestedPreset && nextPresets.some((p) => p.id === requestedPreset))
          return `preset:${requestedPreset}`;
        const requestedDashboard = search.get('dashboard');
        if (requestedDashboard && nextDashboards.some((d) => d.id === requestedDashboard))
          return `dashboard:${requestedDashboard}`;
        return nextDashboards[0]
          ? `dashboard:${nextDashboards[0].id}`
          : nextPresets[0]
            ? `preset:${nextPresets[0].id}`
            : '';
      });
      setError(null);
    } catch (reason) {
      setError(String(reason instanceof Error ? reason.message : reason));
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    void reload();
  }, []);
  const run = async (operation: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await operation();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };
  const save = (draft: Dashboard, onSaved: () => void) =>
    run(async () => {
      const saved = await hubClient.save(draft);
      setDashboards((all) => [...all.filter((d) => d.id !== saved.id), saved]);
      select(`dashboard:${saved.id}`);
      onSaved();
    });
  const remove = (dashboard: Dashboard, onRemoved: () => void) =>
    run(async () => {
      await hubClient.remove(dashboard);
      const next = dashboards.filter((d) => d.id !== dashboard.id);
      setDashboards(next);
      select(next[0] ? `dashboard:${next[0].id}` : presets[0] ? `preset:${presets[0].id}` : '');
      onRemoved();
    });
  return {
    dashboards,
    presets,
    selected,
    select,
    loading,
    busy,
    error,
    reload,
    save,
    remove,
    clearError: () => setError(null),
    current: selected.startsWith('dashboard:')
      ? dashboards.find((d) => `dashboard:${d.id}` === selected)
      : undefined,
    preset: selected.startsWith('preset:') ? presets.find((p) => `preset:${p.id}` === selected) : undefined,
  };
}
