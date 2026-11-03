import { SessionFault } from '@omochi-ui/core/browser';
import type { BrowserIdentity, BrowserTransport } from '@omochi-ui/hub-transport';
import { address, type SavedShare, type ShareLink } from './link.js';
import { parseWelcome, type SharedView } from './session.js';
import type { HubProfile } from './profile.js';
async function probe(transport: BrowserTransport, identity: BrowserIdentity, source: SavedShare, signal: AbortSignal): Promise<SharedView> {
  const endpoint = await transport.openEndpoint(identity.secretHex, source.relayUrls);
  try {
    const connection = await endpoint.connect(address(source), 'omochi/dashboard/1', signal);
    if (connection.remoteId !== source.hubId) throw new SessionFault('NotAuthorized');
    try {
      const stream = await connection.open(signal); await stream.send(JSON.stringify({ version: 1, grantId: source.grantId }), signal);
      const frame = await stream.receive(signal); if (!frame) throw new SessionFault('NetworkUnavailable');
      return parseWelcome(JSON.parse(frame.json));
    } catch (error) {
      if ((await Promise.race([connection.closed(), new Promise<{code: string}>(resolve => setTimeout(() => resolve({code: '0'}), 50))])).code === '1') throw new SessionFault('NotAuthorized'); throw error;
    }
  } finally { await endpoint.close(); }
}
/** Invoked only after explicit user acceptance. A confirmed proof can be saved locally again without network. */
export async function acceptInvitation(transport: BrowserTransport, profile: HubProfile, link: ShareLink, signal: AbortSignal): Promise<{ source: SavedShare; view: SharedView }> {
  if (link.mode !== 'invite') throw new SessionFault('InvalidInvitation');
  const source: SavedShare = { hubId: link.hubId, grantId: link.grantId, relayUrls: link.relayUrls, name: 'Shared dashboard' };
  await profile.save(source, 'pending');
  const attempt = () => probe(transport, profile.identity, source, AbortSignal.any([signal, AbortSignal.timeout(10000)]));
  let view: SharedView;
  try { view = await attempt(); }
  catch (error) {
    if (!(error instanceof SessionFault) || error.code !== 'NotAuthorized') throw error;
    const endpoint = await transport.openEndpoint(profile.identity.secretHex, source.relayUrls);
    try {
      const deadline = AbortSignal.any([signal, AbortSignal.timeout(10000)]);
      const connection = await endpoint.connect(address(source), 'omochi/dashboard-enroll/1', deadline);
      if (connection.remoteId !== source.hubId) throw new SessionFault('NotAuthorized');
      const stream = await connection.open(deadline); await stream.send(JSON.stringify({ version: 1, grantId: source.grantId, secret: link.secret }), deadline);
      const frame = await stream.receive(deadline);
      if (!frame || JSON.parse(frame.json).accepted !== true) throw new SessionFault('EnrollmentRejected');
    } finally { await endpoint.close(); }
    view = await attempt();
  }
  return { source: { ...source, name: view.board.name }, view };
}
/** Recover a lost enrollment reply, including after invitation expiry, using only the durable identity. */
export async function recoverShare(transport: BrowserTransport, profile: HubProfile, source: SavedShare, signal: AbortSignal): Promise<{ source: SavedShare; view: SharedView }> {
  const view = await probe(transport, profile.identity, source, AbortSignal.any([signal, AbortSignal.timeout(10000)]));
  return { source: { ...source, name: view.board.name }, view };
}
