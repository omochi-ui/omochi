import { SessionFault } from '@omochi-ui/core/browser';
import type { BrowserTransport } from '@omochi-ui/hub-transport';
import type { BoardRepository } from '../board-repository.js';
import { SharedBoardRepository } from './boards.js';
import { acceptInvitation, recoverShare } from './enrollment.js';
import { parseShareLink, type SavedShare, type ShareLink } from './link.js';
import { OwnerBoardRepository } from './owner-boards.js';
import { openHubProfile, resetHubProfile, readHubProfile, forgetHubRecord, type HubProfile, type ShareRecord } from './profile.js';
import { RemoteHubSession, type SharedView } from './session.js';

import { readCatalog, changeCatalog, clearCatalog, announceConnections, checkedLink, connectionId, CATALOG_CHANNEL, type ConnectionPreference } from './catalog.js';

export interface ConnectionEntry extends ShareRecord { link?: ShareLink; persistent: boolean }

type AcceptedInvitation = Awaited<ReturnType<typeof acceptInvitation>>;
export type PendingInvitation =
  | { kind: 'invitation'; link: ShareLink }
  | { kind: 'savePending'; accepted: AcceptedInvitation; link: ShareLink | null };

export type ActiveConnection = {
  session: RemoteHubSession;
  boards: BoardRepository;
  id: string;
  status: string;
  view: SharedView | null;
} & (
  | { kind: 'public'; link: ShareLink; owner: false }
  | { kind: 'saved'; owner: boolean; name: string }
);

export interface ConnectionState {
  active: ActiveConnection | null;
  invitation: PendingInvitation | null;
  browserId: string | null;
  records: readonly ShareRecord[];
  entries: readonly ConnectionEntry[];
  ownsProfile: boolean;
  listError: SessionFault | null;
  feedback: 'nameSaved' | 'shareSaved' | null;
  busy: boolean;
  error: SessionFault | null;
}

export type ConnectionAction =
  | { type: 'select'; id: string }
  | { type: 'accept' }
  | { type: 'dismiss' }
  | { type: 'disconnect' }
  | { type: 'retry' }
  | { type: 'loadProfile' }
  | { type: 'forget'; id: string }
  | { type: 'rememberLocation'; id: string; location: string }
  | { type: 'refresh' }
  | { type: 'saveShare'; id: string }
  | { type: 'rename'; id: string; name: string }
  | { type: 'reset' };

interface Options {
  transport: BrowserTransport;
  initialLink: ShareLink | null;
  bootstrapError: SessionFault | null;
  requestLeave(): Promise<boolean>;
}

const PUBLIC_CONTEXT = 'omochi.shared.public';
const SELECTED = 'omochi.shared.selected';

function cache(key: string, value?: string): string | null {
  try {
    if (value === undefined) return sessionStorage.getItem(key);
    if (value) sessionStorage.setItem(key, value);
    else sessionStorage.removeItem(key);
  } catch { /* Storage denial does not grant network authority. */ }
  return null;
}

function sameLink(a: ShareLink | null, b: ShareLink) {
  return a?.hubId === b.hubId && a.grantId === b.grantId && a.mode === b.mode &&
    a.secret === b.secret && a.relayUrls.join('\n') === b.relayUrls.join('\n');
}

export function initialConnectionState(link: ShareLink | null, error: SessionFault | null): ConnectionState {
  return {
    active: null,
    invitation: link?.mode === 'owner' ? { kind: 'invitation', link } : null,
    browserId: null,
    records: [],
    entries: [], ownsProfile: false, listError: null, feedback: null,
    busy: true,
    error,
  };
}

/** Owns connection transitions, endpoint/profile lifetime and per-tab restoration.
 * Views submit complete actions; they never coordinate storage writes or state cleanup.
 */
export class ConnectionController {
  private readonly options: Options;
  private state: ConnectionState;
  private profile: HubProfile | null = null;
  private openingProfile: Promise<HubProfile | null> | null = null;
  private readonly lifetime = new AbortController();
  private readonly listeners = new Set<(state: ConnectionState) => void>();
  private incoming: ShareLink | SessionFault | null = null;
  private openingShare: ShareLink | null;
  private started = false;
  private closing: Promise<void> | null = null;
  private preferences: ConnectionPreference[] = [];
  private tabShares = new Map<string, { link: ShareLink; name: string }>();
  private channel: BroadcastChannel | null = null;
  private refreshQueue: Promise<void> = Promise.resolve();

  private rememberShare(link: ShareLink, name = '') {
    const id = connectionId(link);
    this.tabShares.set(id, { link, name: name || this.tabShares.get(id)?.name || '' });
    while (this.tabShares.size > 100) this.tabShares.delete(this.tabShares.keys().next().value!);
    cache('omochi.connections.tab', JSON.stringify([...this.tabShares.values()]));
    this.publishEntries();
  }

  private publishEntries() {
    const entries: ConnectionEntry[] = this.state.records.map(record => ({ ...record, persistent: true }));
    for (const preference of this.preferences) if (preference.link) {
      const link = this.tabShares.get(preference.id)?.link ?? preference.link;
      entries.push({ ...link, id: preference.id, name: preference.name || this.tabShares.get(preference.id)?.name || '', state: 'accepted', persistent: true, link });
    }
    for (const [id, value] of this.tabShares) if (!entries.some(entry => entry.id === id)) {
      entries.push({ ...value.link, id, name: value.name, state: 'accepted', persistent: false, link: value.link });
    }
    const active = this.state.active;
    if (active?.kind === 'saved' && !entries.some(e => e.id === active.id)) entries.push({ id: active.id, hubId: active.id.split('/')[0]!, grantId: active.id.split('/')[1]!, name: active.name, owner: active.owner ? true : undefined, relayUrls: [], state: 'accepted', persistent: false });
    this.update({ entries: entries.map(entry => ({ ...entry, name: this.preferences.find(p => p.id === entry.id)?.name || (entry.name === 'Owner hub' || !entry.name && entry.owner ? `Hub ${entry.hubId.slice(0, 8)}` : entry.name) })) });
  }

  private refresh() {
    return this.refreshQueue = this.refreshQueue.then(() => this.readMetadata());
  }

  private async readMetadata() {
    if (this.lifetime.signal.aborted) return;
    try {
      const [metadata, preferences] = await Promise.all([readHubProfile(), readCatalog()]);
      if (this.lifetime.signal.aborted) return;
      this.preferences = preferences;
      this.update({ ...metadata, listError: null });
      this.publishEntries();
    } catch (error) {
      this.update({ listError: error instanceof SessionFault ? error : new SessionFault('StorageUnavailable') });
    }
  }

  private async releaseProfile() {
    await this.profile?.close(); this.profile = null;
    this.update({ ownsProfile: false });
  }

  private restoreLocation(id: string) {
    if (typeof location === 'undefined' || typeof history === 'undefined') return;
    const saved = this.preferences.find(p => p.id === id)?.location;
    const url = new URL(location.href);
    const params = new URLSearchParams(saved ?? '');
    for (const key of ['dashboard', 'preset', 'view']) {
      url.searchParams.delete(key);
      if (params.has(key)) url.searchParams.set(key, params.get(key)!);
    }
    history.replaceState(null, '', url);
  }


  constructor(options: Options) {
    this.options = options;
    this.state = initialConnectionState(options.initialLink, options.bootstrapError);
    this.openingShare = options.initialLink?.mode === 'share' ? options.initialLink : null;
  }

  getSnapshot = (): ConnectionState => this.state;

  subscribe(listener: (state: ConnectionState) => void) {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  }

  private update(change: Partial<ConnectionState>) {
    if (this.lifetime.signal.aborted) return;
    this.state = { ...this.state, ...change };
    for (const listener of this.listeners) listener(this.state);
  }

  private async run(work: () => Promise<void>, clearError = true) {
    this.update({ busy: true, ...(clearError ? { error: null, feedback: null } : {}) });
    try {
      await work();
    } catch (error) {
      this.update({ error: error instanceof SessionFault ? error : new SessionFault('StorageUnavailable') });
    } finally {
      this.update({ busy: false });
      this.drainIncoming();
    }
  }

  async start() {
    if (this.started || this.lifetime.signal.aborted) return;
    this.started = true;
    try {
      const values = JSON.parse(cache('omochi.connections.tab') || '[]');
      if (Array.isArray(values) && values.length <= 100) for (const value of values) this.rememberShare(checkedLink(value.link), typeof value.name === 'string' ? value.name.slice(0, 200) : '');
    } catch { /* Invalid tab history does not grant authority. */ }
    if (typeof BroadcastChannel !== 'undefined') {
      try {
        this.channel = new BroadcastChannel(CATALOG_CHANNEL);
        this.channel.onmessage = () => { void this.refresh(); };
      } catch { /* The explicit refresh action remains available. */ }
    }
    await this.run(() => this.open(), false);
  }

  private async loadProfile() {
    if (this.profile) return this.profile;
    this.openingProfile ??= this.acquireProfile();
    try { return await this.openingProfile; }
    finally { this.openingProfile = null; }
  }

  private async acquireProfile() {
    const profile = await openHubProfile(this.options.transport);
    try {
      const records = await profile.shares();
      if (!this.lifetime.signal.aborted) {
        this.profile = profile;
        this.update({ browserId: profile.identity.endpointId, records, ownsProfile: true });
        this.publishEntries();
        return profile;
      }
    } catch (error) {
      await profile.close();
      throw error;
    }
    await profile.close();
    return null;
  }

  private async open() {
    if (this.openingShare) {
      await this.openShare(this.openingShare);
      await this.refresh();
      return;
    }
    cache(PUBLIC_CONTEXT, '');
    await this.refresh();
    if (this.lifetime.signal.aborted) return;
    if (this.state.invitation) { await this.loadProfile(); return; }
    const saved = this.state.entries.find(record => record.id === cache(SELECTED));
    if (saved?.link) await this.openShare(saved.link);
    else if (saved) {
      const profile = await this.loadProfile();
      if (profile) {
        if (saved.state === 'pending') {
          const { id: _id, state: _state, persistent: _persistent, link: _link, ...source } = saved;
          await this.saveAccepted(await recoverShare(this.options.transport, profile, source, this.lifetime.signal), null);
        } else this.activate(saved, profile.identity.secretHex);
      }
    }
  }

  private async openShare(link: ShareLink) {
    // Retain an accepted navigation for explicit retry if identity creation fails.
    this.openingShare = link;
    this.rememberShare(link);
    cache(PUBLIC_CONTEXT, JSON.stringify(link));
    const identity = await this.options.transport.generateIdentity();
    this.activate({ ...link, name: '' }, identity.secretHex, link);
  }

  private activate(source: SavedShare, secretHex: string, link: ShareLink | null = null) {
    if (this.lifetime.signal.aborted) return;
    const boards = new SharedBoardRepository();
    const session = new RemoteHubSession({
      source,
      secretHex,
      publicSecret: link?.secret,
      owner: source.owner,
      onView: view => {
        if (this.state.active?.session !== session || this.lifetime.signal.aborted) return;
        boards.publish(view);
        if (link) this.rememberShare(link, view.board.name);
        this.update({ active: { ...this.state.active, view } });
      },
    }, this.options.transport);
    session.subscribe(event => {
      if (this.state.active?.session === session && event.kind === 'status') {
        this.update({ active: { ...this.state.active, status: event.state.replace(/…$/, '') } });
      }
    });
    boards.bind(session);
    const id = `${source.hubId}/${source.grantId}`;
    const active: ActiveConnection = {
      session,
      boards: source.owner ? new OwnerBoardRepository(session) : boards,
      id,
      status: 'Connecting',
      view: null,
      ...(link ? { kind: 'public', link, owner: false } : { kind: 'saved', owner: !!source.owner, name: source.name }),
    };
    cache(PUBLIC_CONTEXT, link ? JSON.stringify(link) : '');
    cache(SELECTED, link ? '' : id);
    if (!link && source.owner) cache('omochi.connections.owner', id);
    this.openingShare = null;
    this.update({ active });
  }

  private async leave(release = true): Promise<boolean> {
    if (!(await this.options.requestLeave()) || this.lifetime.signal.aborted) return false;
    const active = this.state.active;
    if (active && typeof location !== 'undefined') {
      const params = new URLSearchParams(location.search);
      const saved = new URLSearchParams();
      for (const key of ['dashboard', 'preset', 'view']) if (params.has(key)) saved.set(key, params.get(key)!);
      try {
        await changeCatalog(active.id, { location: saved.toString() });
        this.preferences = await readCatalog();
      } catch { this.update({ listError: new SessionFault('StorageUnavailable') }); }
    }
    await active?.session.stop();
    if (release) await this.releaseProfile();
    if (this.lifetime.signal.aborted) return false;
    this.openingShare = null;
    cache(PUBLIC_CONTEXT, '');
    cache(SELECTED, '');
    this.update({ active: null });
    return true;
  }

  receive(fragment: string) {
    if (this.lifetime.signal.aborted) return;
    try {
      const link = parseShareLink(fragment);
      const current = this.state.invitation?.link ??
        (this.state.active?.kind === 'public' ? this.state.active.link : this.openingShare);
      if (sameLink(current, link)) return;
      this.incoming = link;
    } catch {
      this.incoming = new SessionFault('InvalidInvitation');
    }
    this.drainIncoming();
  }

  private drainIncoming() {
    if (!this.started || this.lifetime.signal.aborted || this.state.busy || this.state.invitation?.kind === 'savePending' || !this.incoming) return;
    const incoming = this.incoming;
    this.incoming = null;
    void this.run(async () => {
      if (incoming instanceof SessionFault) throw incoming;
      if (incoming.mode === 'owner') {
        this.update({ invitation: { kind: 'invitation', link: incoming } });
        await this.loadProfile();
      } else if (await this.leave()) {
        this.update({ invitation: null });
        this.restoreLocation(connectionId(incoming));
        await this.openShare(incoming);
      }
    });
  }

  private async saveAccepted(accepted: AcceptedInvitation, link: ShareLink | null) {
    if (this.lifetime.signal.aborted) return;
    if (!this.profile) throw new SessionFault('InvalidState');
    this.update({ invitation: { kind: 'savePending', accepted, link } });
    await this.profile.save(accepted.source, 'accepted');
    this.update({ records: await this.profile.shares() });
    announceConnections(); this.publishEntries();
    this.restoreLocation(connectionId(accepted.source));
    this.activate(accepted.source, this.profile.identity.secretHex);
    this.update({ invitation: null });
  }

  async dispatch(action: ConnectionAction) {
    if (this.lifetime.signal.aborted) return;
    if (action.type === 'rememberLocation') {
      try { await changeCatalog(action.id, { location: action.location }); }
      catch { this.update({ listError: new SessionFault('StorageUnavailable') }); }
      return;
    }
    if (this.state.busy) return;
    if (this.state.invitation?.kind === 'savePending' && action.type !== 'accept') return;
    await this.run(async () => {
      switch (action.type) {
        case 'dismiss':
          // Enrollment may have persisted a pending record before its probe failed.
          if (this.profile) this.update({ records: await this.profile.shares() });
          this.update({ invitation: null });
          if (this.state.active?.kind !== 'saved') await this.releaseProfile();
          this.publishEntries();
          break;
        case 'select': {
          const record = this.state.entries.find(record => record.id === action.id);
          if (!record || this.state.active?.id === record.id) return;
          if (!(await this.leave())) return;
          this.restoreLocation(record.id);
          if (record.link) { await this.openShare(record.link); break; }
          cache(SELECTED, record.id);
          const profile = await this.loadProfile();
          if (!profile) return;
          if (record.state === 'pending') {
            const { id: _id, state: _state, persistent: _persistent, link: _link, ...source } = record;
            await this.saveAccepted(await recoverShare(this.options.transport, profile, source, this.lifetime.signal), null);
          } else {
            this.activate(record, profile.identity.secretHex);
            this.update({ invitation: null });
          }
          break;
        }
        case 'accept': {
          const invitation = this.state.invitation;
          if (!invitation || !this.profile) return;
          if (invitation.kind === 'savePending') await this.saveAccepted(invitation.accepted, invitation.link);
          else if (await this.leave(false)) {
            await this.saveAccepted(await acceptInvitation(this.options.transport, this.profile, invitation.link, this.lifetime.signal), invitation.link);
          }
          break;
        }
        case 'disconnect':
          if (this.state.invitation) throw new SessionFault('InvalidState');
          if (await this.leave()) {
            this.update({ invitation: null });
            await this.refresh();
          }
          break;
        case 'retry':
          if (this.state.invitation && !this.profile) await this.loadProfile();
          else if (this.state.active) await this.state.active.session.retry();
          else if (this.state.invitation) await this.loadProfile();
          else await this.open();
          break;
        case 'forget': {
          const active = this.state.active;
          if (active?.kind === 'public' && active.id === action.id) this.rememberShare(active.link, active.view?.board.name);
          else this.tabShares.delete(action.id);
          cache('omochi.connections.tab', JSON.stringify([...this.tabShares.values()]));
          await forgetHubRecord(action.id);
          await changeCatalog(action.id, null);
          if (cache(SELECTED) === action.id) cache(SELECTED, '');
          await this.refresh();
          break;
        }
        case 'refresh': await this.refresh(); break;
        case 'rename':
          if (!action.name.trim() || action.name.length > 200) throw new SessionFault('InvalidState');
          await changeCatalog(action.id, { name: action.name.trim() });
          await this.refresh(); this.update({ feedback: 'nameSaved' });
          break;
        case 'saveShare': {
          const entry = this.state.entries.find(e => e.id === action.id);
          if (!entry?.link) throw new SessionFault('InvalidState');
          await changeCatalog(entry.id, { link: entry.link, name: entry.name });
          await this.refresh(); this.update({ feedback: 'shareSaved' });
          break;
        }
        case 'loadProfile': {
          await this.refresh();
          if (!this.state.browserId) {
            await this.loadProfile();
            if (this.state.active?.kind !== 'saved' && !this.state.invitation) await this.releaseProfile();
          }
          break;
        }
        case 'reset':
          if (!(await this.leave())) return;
          await this.profile?.close();
          this.profile = null;
          this.update({ browserId: null, records: [], invitation: null });
          await resetHubProfile();
          await clearCatalog();
          this.tabShares.clear(); cache('omochi.connections.tab', ''); cache('omochi.connections.owner', '');
          this.preferences = []; this.publishEntries();
          await this.loadProfile(); await this.releaseProfile();
          if (!this.lifetime.signal.aborted) await this.open();
          break;
      }
    });
  }

  dispose(): Promise<void> {
    return this.closing ??= (async () => {
      this.lifetime.abort();
      this.channel?.close();
      this.incoming = null;
      await this.state.active?.session.stop();
      await this.openingProfile?.catch(() => null);
      await this.profile?.close();
    })();
  }
}
