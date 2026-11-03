import { createContext, useContext, useRef, type ReactNode } from 'react';
import { createDashboardNavigation } from './dashboard-navigation.js';
type Navigation = ReturnType<typeof createDashboardNavigation>;
const Context = createContext<Navigation | null>(null);
export function DashboardNavigationProvider({ children }: { children: ReactNode }) {
  const navigation = useRef(createDashboardNavigation());
  return <Context.Provider value={navigation.current}>{children}</Context.Provider>;
}
export function useDashboardNavigation(): Navigation {
  const navigation = useContext(Context);
  if (!navigation) throw new Error('Missing dashboard navigation provider');
  return navigation;
}
