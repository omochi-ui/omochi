// Exercise the real React parent/child, profile storage and navigation. Only the
// remote session's network boundary is controlled, including adversarial late replies.
import { createRoot } from 'react-dom/client';
import { ConnectionScreen } from '../../src/remote/ConnectionScreen.tsx';
import { RemoteHubSession } from '../../src/remote/session.ts';
import { openHubProfile } from '../../src/remote/profile.ts';
import { installNativeInvitationButton } from '../../src/remote/native-link.ts';
import { createHostedFragmentInbox } from '../../src/remote/fragment.ts';
import i18n from '../../src/i18n.ts';
import '../../src/styles.css';
import '@omochi-ui/renderer/styles.css';
await i18n.changeLanguage('en');
const publicStartup = new URLSearchParams(location.search).has('public');
const publicLink = { version: 2, mode: 'share', hubId: 'a'.repeat(64), grantId: '44444444-4444-4444-8444-444444444444', relayUrls: ['https://relay.example/'], secret: '2'.repeat(64) };
const identity = { endpointId: 'c'.repeat(64), secretHex: 'd'.repeat(64) };
let restoreRecords = false;
const transport = {
  async generateIdentity() {
    if (restoreRecords) {
      // Model the same saved owner grants under a newly generated browser ID.
      const request = indexedDB.open('omochi.hub-client.v1', 1);
      const db = await new Promise(resolve => { request.onsuccess = () => resolve(request.result); });
      await new Promise(resolve => {
        const tx = db.transaction('shares', 'readwrite');
        for (const source of sources) tx.objectStore('shares').put({ ...source, id: `${source.hubId}/${source.grantId}`, state: 'accepted' });
        tx.oncomplete = resolve;
      });
      db.close();
    }
    return identity;
  },
  identityFromSecret: async () => identity,
};
const sources = [
  { name: 'Original Hub', hubId: 'a'.repeat(64), grantId: '11111111-1111-4111-8111-111111111111' },
  { name: 'Other Hub', hubId: 'b'.repeat(64), grantId: '22222222-2222-4222-8222-222222222222' },
  { name: 'Other owner grant', hubId: 'a'.repeat(64), grantId: '33333333-3333-4333-8333-333333333333' },
].map(source => ({ ...source, owner: true, relayUrls: ['https://relay.example/'] }));
if (!publicStartup) {
  const seed = await openHubProfile(transport);
  for (const source of sources) await seed.save(source, 'accepted');
  await seed.close();
}
const calls = [], sessions = [], replies = [];
const timers = new Map();
let timerId = -1;
const nativeTimeout = window.setTimeout.bind(window), nativeClearTimeout = window.clearTimeout.bind(window);
window.setTimeout = (callback, delay, ...args) => {
  if (delay !== 1000) return nativeTimeout(callback, delay, ...args);
  const id = timerId--; timers.set(id, () => callback(...args)); return id;
};
window.clearTimeout = id => { if (id < 0) timers.delete(id); else nativeClearTimeout(id); };
RemoteHubSession.prototype.start = async function () {
  sessions.push(this);
  this.options.onView({ board: { id: 'shared-board', revision: 1, name: 'Public dashboard', panels: [] }, permissions: { interact: true, editLayout: true } });
  this.status('Connected');
};
RemoteHubSession.prototype.stop = async function () {};
RemoteHubSession.prototype.control = async function (command) {
  if (!command.method.startsWith('gatewayPair')) return [];
  const index = calls.length;
  calls.push({ ...command, source: this.options.source, identity: this.options.secretHex, session: sessions.indexOf(this) });
  return new Promise((resolve, reject) => { replies[index] = { resolve, reject }; });
};
const payload = () => '#g:' + btoa(JSON.stringify({ version: 4, address: { id: 'e'.repeat(64), addrs: [] }, secret: 'f'.repeat(64), expires: Math.floor(Date.now() / 1000) + 180 })).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '');
if (!publicStartup) history.replaceState(null, '', payload());
const fragments = createHostedFragmentInbox(window, fragment => installNativeInvitationButton(document, fragment));
window.gatewayTest = {
  calls, sources, sessions,
  tick() { const ready = [...timers.values()]; timers.clear(); for (const callback of ready) callback(); },
  invite() { location.hash = payload(); },
  reply(index, status, id = 'job1') { replies[index].resolve({ id, status, gatewayId: 'e'.repeat(64) }); },
  fail(index) { replies[index].reject(Error('Disconnected')); },
  changeIdentity({ restore = true, endpointId = '9'.repeat(64), secretHex = '8'.repeat(64) } = {}) {
    restoreRecords = restore; identity.endpointId = endpointId; identity.secretHex = secretHex;
  },
};
createRoot(document.getElementById('root')).render(<ConnectionScreen transport={transport} takeLink={() => publicStartup ? publicLink : null} bootstrapError={null} subscribeFragment={fragments.subscribe} />);
