import type { Dashboard, DashboardPreset } from './contracts.js';

async function request<T>(
  path: string,
  method = 'GET',
  value?: unknown,
  failure = 'Dashboard request failed',
): Promise<T> {
  const response = await fetch(path, {
    method,
    headers: value === undefined ? undefined : { 'Content-Type': 'application/json' },
    ...(value === undefined ? {} : { body: JSON.stringify(value) }),
  });
  const data = await response.json();
  if (!response.ok) throw new Error(data.error ?? failure);
  return data as T;
}

export const hubClient = {
  list: () => request<Dashboard[]>('/api/dashboards'),
  presets: () => request<DashboardPreset[]>('/api/presets', 'GET', undefined, 'Preset request failed'),
  save: (draft: Dashboard) =>
    request<Dashboard>(`/api/dashboards${draft.id ? `/${draft.id}` : ''}`, draft.id ? 'PUT' : 'POST', draft),
  remove: (dashboard: Dashboard) =>
    request(`/api/dashboards/${dashboard.id}`, 'DELETE', { revision: dashboard.revision }),
};
