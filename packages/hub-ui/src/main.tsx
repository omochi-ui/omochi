import './i18n.js';
import { DashboardNavigationProvider } from './DashboardNavigation.js';
import { HttpSharingRepository } from './sharing-repository.js';
import { OwnerSettings } from './OwnerSettings.js';
import { createRoot } from 'react-dom/client';
import App from './App.js';
import { ShareSettings } from './ShareSettings.js';
import { createLocalHubSession } from '@omochi-ui/renderer';
import { HttpBoardRepository } from './http-boards.js';
import './styles.css';
import '@omochi-ui/renderer/styles.css';
const session = createLocalHubSession(
  `${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/ws`,
);
createRoot(document.getElementById('root')!).render(
  <DashboardNavigationProvider>
    <App
      connectionUi={<OwnerSettings />}
      renderSharing={(board) => (
        <ShareSettings
          key={board.id}
          boardId={board.id}
          repository={new HttpSharingRepository()}
        />
      )}
      environment={{
        authority: { kind: 'owner' },
        session,
        boards: new HttpBoardRepository(),
      }}
    />
  </DashboardNavigationProvider>,
);
