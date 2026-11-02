import { useEffect, useState } from 'react';
import { useA2uiSession } from '@omochi-ui/renderer';
import { subscribeServices, type ServiceView } from './services.js';
import { DashboardWorkspace } from './DashboardWorkspace.js';

export default function App() {
  const session = useA2uiSession(`${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/ws`);
  const [services, setServices] = useState<ServiceView[]>([]);
  useEffect(() => {
    const subscription = subscribeServices('', setServices);
    return () => subscription.close();
  }, []);
  return <DashboardWorkspace session={session} services={services} />;
}
