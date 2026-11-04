import { createRoot } from 'react-dom/client';
import { createA2uiSession, type RendererSession } from '@omochi-ui/renderer';
import { browserSessionDiagnostics, createBrowserSession, parseInvitation, type AcceptedGateway } from '@omochi-ui/browser-hub';
import { loadBrowserTransport, type BrowserTransport } from '@omochi-ui/browser-transport';
import type { LiveSession, SessionErrorCode } from '@omochi-ui/core/browser';
import { SessionGallery } from './SessionGallery.js';
import '@omochi-ui/renderer/styles.css';

interface Configuration { gateways: { nodeId: string; name: string; relayUrls: string[]; invitation: string }[] }
declare global {
  interface Window {
    __BROWSER_FIXTURE__: Configuration;
    __browserTest?: { endpointId: string; source: LiveSession; controller: RendererSession; errors: SessionErrorCode[]; diagnostics(): unknown; close(): Promise<void> };
    __browserTestError?: string;
  }
}
async function pair(transport: BrowserTransport, secret: string, config: Configuration): Promise<AcceptedGateway[]> {
  const records: AcceptedGateway[] = [];
  // The temporary endpoint is closed before the ordinary manager can bind this identity.
  for (const gateway of config.gateways) {
    const endpoint = await transport.openEndpoint(secret, gateway.relayUrls);
    const cancel = new AbortController(); const timer = setTimeout(() => cancel.abort(), 10000);
    try {
      const invite = parseInvitation(gateway.invitation);
      const connection = await endpoint.connect(JSON.stringify({ id: gateway.nodeId, addrs: gateway.relayUrls.map(Relay => ({ Relay })) }), 'omochi/pair/1', cancel.signal);
      if (connection.remoteId !== gateway.nodeId) throw new Error('Fixture identity mismatch');
      const stream = await connection.open({ kind: 'pair' }, cancel.signal);
      await stream.send(JSON.stringify({ version: 4, secret: invite.secret, name: 'Omochi browser' }), cancel.signal);
      const response = await stream.receive(cancel.signal);
      if (!response || JSON.parse(response.json).accepted !== true) throw new Error('Fixture enrollment rejected');
      records.push({ nodeId: gateway.nodeId, name: gateway.name, relayUrls: gateway.relayUrls, enrollment: 'accepted', createdAt: Date.now() });
    } finally { clearTimeout(timer); await endpoint.close(); }
  }
  return records;
}
async function main() {
  const root = createRoot(document.getElementById('root')!);
  await navigator.locks.request('omochi.browser.v1/default/endpoint', { mode: 'exclusive', ifAvailable: true }, async lock => {
    if (!lock) throw new Error('OwnerBusy');
    const transport = await loadBrowserTransport(new URL('./iroh/', document.baseURI));
    const identity = await transport.generateIdentity();
    const gateways = await pair(transport, identity.secretHex, window.__BROWSER_FIXTURE__);
    // Never copy test invitations/secret material into the publicly inspectable state.
    delete (window as Partial<Window>).__BROWSER_FIXTURE__;
    const source = createBrowserSession({ identity, gateways }, transport);
    source.setServices(gateways.flatMap(gateway => ['s', 's'.repeat(16), 's'.repeat(17), 's'.repeat(64)].map(id => `${gateway.nodeId}~${id}`)));
    const errors: SessionErrorCode[] = [];
    source.subscribe(event => { if (event.kind === 'status' && event.error) errors.push(event.error.code); });
    const load = { updates: 0, invalid: 0, last: new Map<string, number>() };
    const attach = source.attachRenderer.bind(source);
    source.attachRenderer = apply => attach(event => {
      const result = apply(event);
      if (result.ok && event.kind === 'update' && 'updateComponents' in event.message) {
        for (const component of event.message.updateComponents.components) {
          const text = component.text;
          if (typeof text !== 'string' || !text.startsWith('LOAD ')) continue;
          const sequence = Number(text.slice(5, 10));
          if (text.length !== 1024 || !Number.isSafeInteger(sequence) || sequence <= (load.last.get(event.service) ?? 0)) load.invalid++;
          load.last.set(event.service, sequence); load.updates++;
        }
      }
      return result;
    });
    const controller = createA2uiSession(source);
    let release!: () => void;
    const done = new Promise<void>(resolve => { release = resolve; });
    let closing: Promise<void> | null = null;
    const images = controller.getSnapshot().images;
    window.__browserTest = { endpointId: identity.endpointId, source, controller, errors,
      diagnostics: () => ({ session: browserSessionDiagnostics(source), images: images?.diagnostics(), load: { updates: load.updates, invalid: load.invalid, last: [...load.last] } }), close: () => {
      closing ??= controller.close().then(() => { root.unmount(); release(); });
      return closing;
    } };
    root.render(<SessionGallery controller={controller} />);
    window.addEventListener('pagehide', () => { void window.__browserTest?.close(); }, { once: true });
    await done;
  });
}
void main().catch(() => { window.__browserTestError = 'Browser fixture failed'; document.getElementById('root')!.textContent = 'Browser fixture failed'; });
