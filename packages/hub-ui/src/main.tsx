import { createRoot } from 'react-dom/client';
import App from './App.js';
import { ShareSettings } from './ShareSettings.js';
import { createLocalHubSession } from '@omochi-ui/renderer';
import { HttpBoardRepository } from './http-boards.js';
import './styles.css';
import '@omochi-ui/renderer/styles.css';
const session = createLocalHubSession(`${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/ws`);
createRoot(document.getElementById('root')!).render(<App renderSharing={board => <ShareSettings key={board.id} boardId={board.id} />} environment={{ session, boards: new HttpBoardRepository() }} />);
