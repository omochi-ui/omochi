import {
  isValidNodeId,
  parseControlFrame,
  SessionFault,
  type LiveSession,
  type SessionError,
  type ServiceKey,
  type Unsubscribe,
} from '@omochi-ui/core/browser';
import type {
  BrowserTransport,
  Connection,
  Endpoint,
  Stream,
} from '@omochi-ui/browser-transport';
import {
  invitationAddress,
  parseInvitation,
  type BrowserInvitation,
} from './bootstrap.js';
import {
  createBrowserSession,
  type AcceptedGateway,
  type GatewayRecord,
} from './session.js';
import {
  acquireProfileLock,
  defaultPreferences,
  openProfileDatabase,
  readProfile,
  resetProfile,
  transaction,
  validGateway,
  validIdentity,
  validPreferences,
  type IdentityRecord,
  type Preferences,
} from './storage.js';

export interface ProfileView {
  endpointId: string;
  gateways: readonly {
    record: Readonly<GatewayRecord>;
    selected: boolean;
    recovery: 'none' | 'pending' | 'unknown' | 'unsaved';
    error: SessionError | null;
  }[];
  operation: 'idle' | 'pairing' | 'refreshing' | 'saving';
  connections: 'available' | 'suspended' | 'closed';
  closingPending: boolean;
  invitation: null | {
    nodeId: string;
    expires: number;
    state:
      'refreshing' | 'refreshed' | 'refresh-failed' | 'can-redeem' | 'expired';
    error: SessionError | null;
  };
}
export interface ProfileOwner {
  readonly kind: 'owner';
  view(): Readonly<ProfileView>;
  session(): LiveSession | null;
  pair(invitation: string): Promise<void>;
  refreshInvitation(invitation: string): Promise<void>;
  retryInvitationRefresh(nodeId: string): Promise<void>;
  redeemInvitation(nodeId: string): Promise<void>;
  retryEnrollmentSave(nodeId: string): Promise<void>;
  connectSelected(): Promise<void>;
  retryConnection(
    target: { gatewayId: string } | { service: ServiceKey },
  ): Promise<void>;
  selectGateway(nodeId: string, selected: boolean): Promise<void>;
  forgetLocalGateway(nodeId: string): Promise<void>;
  resetIdentity(): Promise<void>;
  subscribe(listener: () => void): Unsubscribe;
  close(): Promise<void>;
}
export interface ProfileRecovery {
  readonly kind: 'recovery';
  readonly error: SessionError;
  readonly storedEndpointId: string | null;
  resetIdentity(): Promise<void>;
  close(): Promise<void>;
}
type Summary = NonNullable<ProfileView['invitation']>;
interface Proof {
  record: GatewayRecord;
  refresh: boolean;
}
interface NetworkPhase {
  controller: AbortController;
  proved: boolean;
}
function fault(error: unknown): SessionFault {
  return error instanceof SessionFault
    ? error
    : new SessionFault('NetworkUnavailable');
}
function check(signal: AbortSignal): void {
  if (signal.aborted) throw new SessionFault('Cancelled');
}
async function deadline<T>(
  ms: number,
  parent: AbortSignal,
  work: (signal: AbortSignal) => Promise<T>,
): Promise<T> {
  check(parent);
  const controller = new AbortController(),
    expires = Date.now() + ms;
  const signal = AbortSignal.any([parent, controller.signal]);
  const timer = setTimeout(() => controller.abort(), ms);
  try {
    const result = await work(signal);
    check(parent);
    if (controller.signal.aborted || Date.now() >= expires)
      throw new SessionFault('Timeout');
    return result;
  } catch (error) {
    if (parent.aborted) throw new SessionFault('Cancelled');
    if (controller.signal.aborted || Date.now() >= expires)
      throw new SessionFault('Timeout');
    throw fault(error);
  } finally {
    clearTimeout(timer);
  }
}
async function json(stream: Stream, signal: AbortSignal): Promise<unknown> {
  const frame = await stream.receive(signal);
  if (!frame) throw new SessionFault('NetworkUnavailable');
  if (
    !Number.isSafeInteger(frame.wireBytes) ||
    frame.wireBytes < 1 ||
    frame.wireBytes > 4 * 1024 * 1024
  )
    throw new SessionFault('ProtocolError');
  try {
    return JSON.parse(frame.json);
  } catch {
    throw new SessionFault('ProtocolError');
  }
}
function publicError(error: SessionError | null): SessionError | null {
  return error
    ? Object.freeze({ code: error.code, message: error.message })
    : null;
}

class Owner implements ProfileOwner {
  readonly kind = 'owner' as const;
  private readonly records = new Map<string, GatewayRecord>();
  private readonly recovery = new Map<string, 'unknown' | 'unsaved'>();
  private readonly errors = new Map<string, SessionFault>();
  private readonly proofs = new Map<string, Proof>();
  private readonly listeners = new Set<() => void>();
  private source: LiveSession | null = null;
  private sourceKey: string | null = null;
  private chain: Promise<unknown> = Promise.resolve();
  private closing: Promise<void> | null = null;
  private dead = false;
  private readonly pendingCloses = new Set<symbol>();
  private closeFailed = false;
  private operation: ProfileView['operation'] = 'idle';
  private connections: ProfileView['connections'] = 'available';
  private capture: Readonly<BrowserInvitation> | null = null;
  private summary: Summary | null = null;
  private expiry: ReturnType<typeof setTimeout> | null = null;
  private network: NetworkPhase | null = null;
  private cached!: Readonly<ProfileView>;
  constructor(
    private readonly db: IDBDatabase,
    private readonly release: () => Promise<void>,
    private readonly transport: BrowserTransport,
    private readonly identity: IdentityRecord,
    records: GatewayRecord[],
    private preferences: Preferences,
  ) {
    for (const record of records) this.records.set(record.nodeId, record);
    this.constructSession();
    this.publish();
  }
  view(): Readonly<ProfileView> {
    return this.cached;
  }
  session(): LiveSession | null {
    return this.source;
  }
  subscribe(listener: () => void): Unsubscribe {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  pair(text: string): Promise<void> {
    return this.withInvitation(text, false);
  }
  refreshInvitation(text: string): Promise<void> {
    return this.withInvitation(text, true);
  }
  private withInvitation(text: string, refresh: boolean): Promise<void> {
    try {
      this.available();
      const invite = parseInvitation(text);
      if (invite.nodeId === this.identity.endpointId)
        throw new SessionFault('InvalidInvitation');
      if (refresh && this.records.get(invite.nodeId)?.enrollment !== 'accepted')
        throw new SessionFault('InvalidState');
      return this.enqueue(async () => {
        this.available();
        this.assertUnexpired(invite);
        this.captureInvitation(invite);
        await this.exchange(invite, refresh);
      });
    } catch (error) {
      return Promise.reject(error);
    }
  }
  retryInvitationRefresh(nodeId: string): Promise<void> {
    return this.whenAvailable(async () => {
      const invite = this.requireCapture(nodeId);
      if (this.records.get(nodeId)?.enrollment !== 'accepted')
        throw new SessionFault('InvalidState');
      await this.exchange(invite, true);
    });
  }
  redeemInvitation(nodeId: string): Promise<void> {
    return this.whenAvailable(async () => {
      const invite = this.requireCapture(nodeId);
      if (this.summary?.state !== 'can-redeem')
        throw new SessionFault('InvalidState');
      await this.exchange(invite, false);
    });
  }
  retryEnrollmentSave(nodeId: string): Promise<void> {
    return this.enqueue(async () => {
      const proof = this.proofs.get(nodeId);
      if (!proof) throw new SessionFault('InvalidState');
      this.operation = 'saving';
      this.publish();
      try {
        await this.saveProof(nodeId, proof);
      } finally {
        this.operation = 'idle';
        this.publish();
      }
      // Deliberately remain suspended. The public Connect action restarts ordinary traffic.
    });
  }
  connectSelected(): Promise<void> {
    return this.whenAvailable(async () => {
      await this.reconstruct();
    });
  }
  retryConnection(
    target: { gatewayId: string } | { service: ServiceKey },
  ): Promise<void> {
    return this.whenAvailable(async () => {
      if (!this.source) throw new SessionFault('InvalidState');
      await this.source.retry(target);
    });
  }
  selectGateway(nodeId: string, selected: boolean): Promise<void> {
    return this.whenAvailable(async () => {
      if (
        !isValidNodeId(nodeId) ||
        typeof selected !== 'boolean' ||
        this.records.get(nodeId)?.enrollment !== 'accepted'
      )
        throw new SessionFault('InvalidState');
      const ids = this.preferences.selectedGateways.filter(
        (id) => id !== nodeId,
      );
      if (selected) {
        if (this.preferences.selectedGateways.includes(nodeId)) return;
        if (ids.length >= 2) throw new SessionFault('CapacityExceeded');
        ids.push(nodeId);
      } else if (ids.length === this.preferences.selectedGateways.length)
        return;
      const next = { ...this.preferences, selectedGateways: ids };
      await transaction<void>(this.db, ['preferences'], 'readwrite', (tx) => {
        tx.objectStore('preferences').put(next);
      });
      this.preferences = next;
      await this.reconstruct();
      this.publish();
    });
  }
  forgetLocalGateway(nodeId: string): Promise<void> {
    if (this.proofs.size && !this.proofs.has(nodeId))
      return Promise.reject(new SessionFault('EnrollmentUnsaved'));
    const discardingProof = this.proofs.has(nodeId);
    return this.enqueue(async () => {
      if (this.proofs.size && !this.proofs.has(nodeId))
        throw new SessionFault('EnrollmentUnsaved');
      if (!isValidNodeId(nodeId) || !this.records.has(nodeId))
        throw new SessionFault('InvalidState');
      const suspended = discardingProof || this.proofs.has(nodeId);
      const next = {
        ...this.preferences,
        selectedGateways: this.preferences.selectedGateways.filter(
          (id) => id !== nodeId,
        ),
      };
      await transaction<void>(
        this.db,
        ['gateways', 'preferences'],
        'readwrite',
        (tx) => {
          tx.objectStore('gateways').delete(nodeId);
          tx.objectStore('preferences').put(next);
        },
      );
      this.preferences = next;
      this.records.delete(nodeId);
      this.errors.delete(nodeId);
      this.recovery.delete(nodeId);
      this.proofs.delete(nodeId);
      if (this.summary?.nodeId === nodeId) this.clearCapture();
      if (suspended) {
        await this.stopSession();
        this.connections = 'suspended';
      } else await this.reconstruct();
      this.publish();
    });
  }
  resetIdentity(): Promise<void> {
    return this.enqueue(async () => {
      if (this.closeFailed) throw new SessionFault('NetworkUnavailable');
      await this.stopSession();
      this.connections = 'suspended';
      this.publish();
      await resetProfile(this.db);
      this.clearCapture();
      this.proofs.clear();
      this.dead = true;
      this.connections = 'closed';
      this.publish();
      this.db.close();
      await this.release();
    });
  }
  close(): Promise<void> {
    if (this.closing) return this.closing;
    this.dead = true;
    this.network?.controller.abort();
    this.clearCapture();
    this.closing = this.chain.then(async () => {
      await this.stopSession();
      this.proofs.clear();
      this.connections = 'closed';
      this.operation = 'idle';
      this.publish();
      if (this.closeFailed) throw new SessionFault('NetworkUnavailable');
      this.db.close();
      await this.release();
    });
    return this.closing;
  }
  private available(): void {
    if (this.dead) throw new SessionFault('InvalidState');
    if (this.closeFailed) throw new SessionFault('NetworkUnavailable');
    if (this.proofs.size) throw new SessionFault('EnrollmentUnsaved');
  }
  private enqueue(work: () => Promise<void>): Promise<void> {
    if (this.dead) return Promise.reject(new SessionFault('InvalidState'));
    const job = this.chain.then(() => {
      if (this.dead) throw new SessionFault('InvalidState');
      return work();
    });
    this.chain = job.catch(() => {});
    return job;
  }
  private whenAvailable(work: () => Promise<void>): Promise<void> {
    try {
      this.available();
    } catch (error) {
      return Promise.reject(error);
    }
    return this.enqueue(() => {
      this.available();
      return work();
    });
  }
  private constructSession(): void {
    const gateways = this.preferences.selectedGateways
      .map((id) => this.records.get(id)!)
      .filter(
        (record): record is AcceptedGateway =>
          record?.enrollment === 'accepted',
      );
    this.sourceKey = JSON.stringify(gateways);
    this.source = createBrowserSession(
      {
        identity: {
          secretHex: this.identity.secretHex,
          endpointId: this.identity.endpointId,
        },
        gateways,
      },
      this.transport,
    );
    this.connections = 'available';
  }
  private async stopSession(): Promise<void> {
    const previous = this.source;
    this.source = null;
    this.sourceKey = null;
    if (previous) {
      this.publish();
      await this.joinClose(() => previous.stop());
    }
  }
  private async joinClose(work: () => Promise<void>): Promise<void> {
    const token = Symbol();
    const timer = setTimeout(() => {
      this.pendingCloses.add(token);
      this.publish();
    }, 2000);
    try {
      await work();
    } catch (error) {
      this.closeFailed = true;
      throw error;
    } finally {
      clearTimeout(timer);
      if (this.pendingCloses.delete(token)) this.publish();
    }
  }
  private async reconstruct(): Promise<void> {
    if (this.dead || this.closeFailed) return;
    const key = JSON.stringify(
      this.preferences.selectedGateways.map((id) => this.records.get(id)!),
    );
    if (this.source && this.sourceKey === key) return;
    await this.stopSession();
    if (!this.dead) {
      this.constructSession();
      this.publish();
    }
  }
  private assertUnexpired(invite: BrowserInvitation): void {
    if (invite.expires <= Math.floor(Date.now() / 1000))
      throw new SessionFault('InvalidInvitation');
  }
  private requireCapture(nodeId: string): Readonly<BrowserInvitation> {
    if (!this.capture || this.capture.nodeId !== nodeId)
      throw new SessionFault('InvalidInvitation');
    this.assertUnexpired(this.capture);
    return this.capture;
  }
  private clearCapture(): void {
    if (this.expiry !== null) clearTimeout(this.expiry);
    this.expiry = null;
    this.capture = null;
    this.summary = null;
  }
  private captureInvitation(invite: Readonly<BrowserInvitation>): void {
    this.clearCapture();
    this.capture = invite;
    this.summary = {
      nodeId: invite.nodeId,
      expires: invite.expires,
      state: 'can-redeem',
      error: null,
    };
    const expire = () => {
      if (this.capture !== invite) return;
      const remaining = invite.expires * 1000 - Date.now();
      if (remaining > 0) {
        this.expiry = setTimeout(expire, Math.min(remaining, 2147483647));
        return;
      }
      this.expiry = null;
      this.capture = null;
      this.summary = {
        nodeId: invite.nodeId,
        expires: invite.expires,
        state: 'expired',
        error: null,
      };
      if (this.network && !this.network.proved) this.network.controller.abort();
      this.publish();
    };
    expire();
    this.publish();
  }
  private async exchange(
    invite: Readonly<BrowserInvitation>,
    refresh: boolean,
  ): Promise<void> {
    this.assertUnexpired(invite);
    const previous = this.records.get(invite.nodeId);
    if (refresh && previous?.enrollment !== 'accepted')
      throw new SessionFault('InvalidState');
    this.operation = refresh ? 'refreshing' : 'pairing';
    this.summary = {
      nodeId: invite.nodeId,
      expires: invite.expires,
      state: refresh ? 'refreshing' : 'can-redeem',
      error: null,
    };
    this.publish();
    const phase: NetworkPhase = {
      controller: new AbortController(),
      proved: false,
    };
    this.network = phase;
    let endpoint: Endpoint | null = null;
    let sent = false;
    let failure: SessionFault | null = null;
    try {
      await this.stopSession();
      check(phase.controller.signal);
      if (previous?.enrollment !== 'accepted' && !refresh) {
        const record: GatewayRecord = {
          nodeId: invite.nodeId,
          name: previous?.name ?? invite.nodeId.slice(0, 12),
          relayUrls: [...invite.relayUrls],
          enrollment: 'pending',
          createdAt: previous?.createdAt ?? Date.now(),
        };
        await transaction<void>(this.db, ['gateways'], 'readwrite', (tx) => {
          tx.objectStore('gateways').put(record);
        });
        this.records.set(record.nodeId, record);
        this.publish();
      }
      check(phase.controller.signal);
      this.assertUnexpired(invite);
      endpoint = await this.transport.openEndpoint(
        this.identity.secretHex,
        invite.relayUrls,
      );
      check(phase.controller.signal);
      this.assertUnexpired(invite);
      const connection = await deadline(
        refresh ? 10000 : 30000,
        phase.controller.signal,
        (signal) =>
          endpoint!.connect(
            invitationAddress(invite),
            refresh ? 'omochi/gateway/4' : 'omochi/pair/1',
            signal,
          ),
      );
      if (connection.remoteId !== invite.nodeId)
        throw new SessionFault('ProtocolError');
      if (refresh)
        await this.probe(connection, invite.nodeId, phase.controller.signal);
      else
        await deadline(10000, phase.controller.signal, async (signal) => {
          const stream = await connection.open({ kind: 'pair' }, signal);
          sent = true;
          await stream.send(
            JSON.stringify({
              version: 4,
              secret: invite.secret,
              name: 'Omochi browser',
            }),
            signal,
          );
          await stream.finish();
          const reply = await json(stream, signal);
          if (
            !reply ||
            typeof reply !== 'object' ||
            Object.keys(reply).length !== 1 ||
            !Object.hasOwn(reply, 'accepted') ||
            typeof (reply as { accepted?: unknown }).accepted !== 'boolean'
          )
            throw new SessionFault('EnrollmentUnknown');
          if (!(reply as { accepted: boolean }).accepted) {
            sent = false;
            throw new SessionFault('EnrollmentRejected');
          }
        });
      phase.proved = true;
      // The proof remains usable after invitation expiry. Never retain its bearer secret here.
      const base = this.records.get(invite.nodeId)!;
      const proof: Proof = {
        refresh,
        record: {
          ...base,
          relayUrls: [...invite.relayUrls],
          enrollment: 'accepted',
        },
      };
      this.proofs.set(invite.nodeId, proof);
      const completed = endpoint;
      await this.joinClose(() => completed.close());
      endpoint = null;
      this.operation = 'saving';
      this.publish();
      await this.saveProof(invite.nodeId, proof);
    } catch (error) {
      failure = fault(error);
      if (
        !phase.proved &&
        !this.dead &&
        invite.expires <= Math.floor(Date.now() / 1000)
      )
        failure = new SessionFault('InvalidInvitation');
      if (phase.proved) {
        failure = new SessionFault('EnrollmentUnsaved');
        this.recovery.set(invite.nodeId, 'unsaved');
        this.connections = 'suspended';
      } else if (!refresh && sent) {
        failure = new SessionFault('EnrollmentUnknown');
        this.recovery.set(invite.nodeId, 'unknown');
      }
      this.errors.set(invite.nodeId, failure);
      if (refresh && this.summary?.state !== 'expired')
        this.summary = {
          nodeId: invite.nodeId,
          expires: invite.expires,
          state:
            failure.code === 'NotAuthorized' ? 'can-redeem' : 'refresh-failed',
          error: failure,
        };
    } finally {
      try {
        if (endpoint) {
          const remaining = endpoint;
          await this.joinClose(() => remaining.close());
        }
      } finally {
        this.network = null;
        this.operation = 'idle';
        if (!this.proofs.size) await this.reconstruct();
        this.publish();
      }
    }
    if (failure) throw failure;
  }
  private async probe(
    connection: Connection,
    nodeId: string,
    parent: AbortSignal,
  ): Promise<void> {
    let closeCode: string | null = null;
    const controller = new AbortController();
    const ended = connection.closed().then(
      (result) => {
        closeCode = result.code;
        controller.abort();
      },
      () => {
        controller.abort();
      },
    );
    try {
      await deadline(
        10000,
        AbortSignal.any([parent, controller.signal]),
        async (signal) => {
          const stream = await connection.open(
            { kind: 'control', protocol: 4 },
            signal,
          );
          const hello = parseControlFrame(await json(stream, signal));
          if (hello.kind !== 'hello' || hello.nodeId !== nodeId)
            throw new SessionFault('ProtocolError');
          const roster = parseControlFrame(await json(stream, signal));
          if (roster.kind !== 'services')
            throw new SessionFault('ProtocolError');
        },
      );
    } catch (error) {
      if (closeCode === '1') throw new SessionFault('NotAuthorized');
      throw error;
    } finally {
      void ended;
    }
  }
  private async saveProof(nodeId: string, proof: Proof): Promise<void> {
    try {
      await transaction<void>(this.db, ['gateways'], 'readwrite', (tx) => {
        tx.objectStore('gateways').put(proof.record);
      });
    } catch {
      this.recovery.set(nodeId, 'unsaved');
      this.errors.set(nodeId, new SessionFault('EnrollmentUnsaved'));
      this.connections = 'suspended';
      throw new SessionFault('EnrollmentUnsaved');
    }
    this.records.set(nodeId, proof.record);
    this.proofs.delete(nodeId);
    this.recovery.delete(nodeId);
    this.errors.delete(nodeId);
    if (proof.refresh && this.summary?.nodeId === nodeId)
      this.summary = { ...this.summary, state: 'refreshed', error: null };
    else if (!proof.refresh && this.summary?.nodeId === nodeId)
      this.clearCapture();
  }
  private publish(): void {
    const gateways = [...this.records.values()]
      .sort(
        (a, b) => a.createdAt - b.createdAt || a.nodeId.localeCompare(b.nodeId),
      )
      .map((record) =>
        Object.freeze({
          record: Object.freeze({
            ...record,
            relayUrls: Object.freeze([...record.relayUrls]),
          }),
          selected: this.preferences.selectedGateways.includes(record.nodeId),
          recovery:
            this.recovery.get(record.nodeId) ??
            (record.enrollment === 'pending' ? 'pending' : 'none'),
          error: publicError(this.errors.get(record.nodeId) ?? null),
        }),
      );
    this.cached = Object.freeze({
      endpointId: this.identity.endpointId,
      gateways: Object.freeze(gateways),
      operation: this.operation,
      connections: this.connections,
      closingPending: this.pendingCloses.size > 0,
      invitation: this.summary
        ? Object.freeze({
            ...this.summary,
            error: publicError(this.summary.error),
          })
        : null,
    });
    for (const listener of this.listeners) {
      try {
        const result: unknown = listener();
        if (result instanceof Promise) void result.catch(() => {});
      } catch {
        /* Observers cannot interrupt a commit. */
      }
    }
  }
}

class Recovery implements ProfileRecovery {
  readonly kind = 'recovery' as const;
  readonly error = Object.freeze({
    code: 'StorageCorrupt' as const,
    message: new SessionFault('StorageCorrupt').message,
  });
  private chain: Promise<unknown> = Promise.resolve();
  private dead = false;
  private closing: Promise<void> | null = null;
  constructor(
    private readonly db: IDBDatabase,
    private readonly release: () => Promise<void>,
    readonly storedEndpointId: string | null,
  ) {}
  resetIdentity(): Promise<void> {
    if (this.dead) return Promise.reject(new SessionFault('InvalidState'));
    const job = this.chain.then(async () => {
      if (this.dead) throw new SessionFault('InvalidState');
      await resetProfile(this.db);
      this.dead = true;
      this.db.close();
      await this.release();
    });
    this.chain = job.catch(() => {});
    return job;
  }
  close(): Promise<void> {
    if (this.closing) return this.closing;
    this.dead = true;
    return (this.closing = this.chain.then(async () => {
      this.db.close();
      await this.release();
    }));
  }
}

export async function openBrowserProfile(
  transport: BrowserTransport,
): Promise<ProfileOwner | ProfileRecovery> {
  const release = await acquireProfileLock();
  let db: IDBDatabase | null = null;
  let storedEndpointId: string | null = null;
  try {
    db = await openProfileDatabase();
    const raw = await readProfile(db);
    const stored = raw.identities.find(
      (value) =>
        value &&
        typeof value === 'object' &&
        (value as { id?: unknown }).id === 'default',
    ) as { endpointId?: unknown } | undefined;
    if (
      typeof stored?.endpointId === 'string' &&
      isValidNodeId(stored.endpointId)
    )
      storedEndpointId = stored.endpointId;
    if (
      raw.identities.length > 1 ||
      raw.preferences.length > 1 ||
      !raw.identities.every(validIdentity) ||
      !raw.gateways.every(validGateway) ||
      !raw.preferences.every(validPreferences)
    )
      throw new SessionFault('StorageCorrupt');
    let identity = raw.identities[0] as IdentityRecord | undefined;
    const gateways = raw.gateways as GatewayRecord[];
    const preferences =
      (raw.preferences[0] as Preferences | undefined) ?? defaultPreferences();
    if (
      preferences.selectedGateways.some(
        (id) =>
          !gateways.some(
            (record) =>
              record.nodeId === id && record.enrollment === 'accepted',
          ),
      ) ||
      (!identity && gateways.length) ||
      (identity && !raw.preferences.length)
    )
      throw new SessionFault('StorageCorrupt');
    if (identity) {
      const verified = await transport.identityFromSecret(identity.secretHex);
      if (
        verified.secretHex !== identity.secretHex ||
        verified.endpointId !== identity.endpointId
      )
        throw new SessionFault('StorageCorrupt');
    } else {
      const generated = await transport.generateIdentity();
      identity = { id: 'default', ...generated, createdAt: Date.now() };
      if (!validIdentity(identity)) throw new SessionFault('StorageCorrupt');
      const verified = await transport.identityFromSecret(identity.secretHex);
      if (
        verified.secretHex !== identity.secretHex ||
        verified.endpointId !== identity.endpointId
      )
        throw new SessionFault('StorageCorrupt');
      await transaction<void>(
        db,
        ['identity', 'preferences'],
        'readwrite',
        (tx) => {
          tx.objectStore('identity').put(identity!);
          tx.objectStore('preferences').put(preferences);
        },
      );
    }
    return new Owner(db, release, transport, identity, gateways, preferences);
  } catch (error) {
    if (error instanceof SessionFault && error.code === 'StorageCorrupt' && db)
      return new Recovery(db, release, storedEndpointId);
    db?.close();
    await release();
    throw error instanceof SessionFault
      ? error
      : new SessionFault('StorageUnavailable');
  }
}
