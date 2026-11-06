import { SessionFault } from '@omochi-ui/core/browser';
import type { BrowserTransport } from '@omochi-ui/hub-transport';
import type { BoardRepository } from '../board-repository.js';
import { SharedBoardRepository } from './boards.js';
import { acceptInvitation, recoverShare } from './enrollment.js';
import { parseShareLink, type SavedShare, type ShareLink } from './link.js';
import { OwnerBoardRepository } from './owner-boards.js';
import { openHubProfile, resetHubProfile, type HubProfile, type ShareRecord } from './profile.js';
import { RemoteHubSession, type SharedView } from './session.js';

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
    this.update({ busy: true, ...(clearError ? { error: null } : {}) });
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
        this.update({ browserId: profile.identity.endpointId, records });
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
      return;
    }
    cache(PUBLIC_CONTEXT, '');
    const profile = await this.loadProfile();
    if (!profile || this.lifetime.signal.aborted) return;
    const selected = cache(SELECTED);
    const saved = this.state.records.find(record => record.id === selected && record.state === 'accepted');
    if (saved) this.activate(saved, profile.identity.secretHex);
  }

  private async openShare(link: ShareLink) {
    // Retain an accepted navigation for explicit retry if identity creation fails.
    this.openingShare = link;
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
    this.openingShare = null;
    this.update({ active });
  }

  private async leave(): Promise<boolean> {
    if (!(await this.options.requestLeave()) || this.lifetime.signal.aborted) return false;
    await this.state.active?.session.stop();
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
    this.activate(accepted.source, this.profile.identity.secretHex);
    this.update({ invitation: null });
  }

  async dispatch(action: ConnectionAction) {
    if (this.state.busy || this.lifetime.signal.aborted) return;
    if (this.state.invitation?.kind === 'savePending' && action.type !== 'accept') return;
    await this.run(async () => {
      switch (action.type) {
        case 'dismiss':
          // Enrollment may have persisted a pending record before its probe failed.
          if (this.profile) this.update({ records: await this.profile.shares() });
          this.update({ invitation: null });
          break;
        case 'select': {
          const record = this.state.records.find(record => record.id === action.id);
          if (!record || !this.profile || this.state.active?.id === record.id) return;
          if (!(await this.leave())) return;
          if (record.state === 'pending') {
            const { id: _id, state: _state, ...source } = record;
            await this.saveAccepted(await recoverShare(this.options.transport, this.profile, source, this.lifetime.signal), null);
          } else {
            this.activate(record, this.profile.identity.secretHex);
            this.update({ invitation: null });
          }
          break;
        }
        case 'accept': {
          const invitation = this.state.invitation;
          if (!invitation || !this.profile) return;
          if (invitation.kind === 'savePending') await this.saveAccepted(invitation.accepted, invitation.link);
          else if (await this.leave()) {
            await this.saveAccepted(await acceptInvitation(this.options.transport, this.profile, invitation.link, this.lifetime.signal), invitation.link);
          }
          break;
        }
        case 'disconnect':
          if (await this.leave()) {
            this.update({ invitation: null });
            const profile = await this.loadProfile();
            if (profile) this.update({ records: await profile.shares() });
          }
          break;
        case 'retry':
          if (this.state.invitation && !this.profile) await this.loadProfile();
          else if (this.state.active) await this.state.active.session.retry();
          else if (this.state.invitation) await this.loadProfile();
          else await this.open();
          break;
        case 'forget':
          if (!this.profile) throw new SessionFault('InvalidState');
          await this.profile.forget(action.id);
          if (cache(SELECTED) === action.id) cache(SELECTED, '');
          this.update({ records: await this.profile.shares() });
          break;
        case 'loadProfile':
          // Settings may acquire a registered profile without replacing a public-share session.
          await this.loadProfile();
          break;
        case 'reset':
          if (!(await this.leave())) return;
          await this.profile?.close();
          this.profile = null;
          this.update({ browserId: null, records: [], invitation: null });
          await resetHubProfile();
          if (!this.lifetime.signal.aborted) await this.open();
          break;
      }
    });
  }

  dispose(): Promise<void> {
    return this.closing ??= (async () => {
      this.lifetime.abort();
      this.incoming = null;
      await this.state.active?.session.stop();
      await this.openingProfile?.catch(() => null);
      await this.profile?.close();
    })();
  }
}
