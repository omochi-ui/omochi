import assert from 'node:assert/strict';
import { IDBFactory, IDBObjectStore } from 'fake-indexeddb';
import { act, createElement, useSyncExternalStore } from 'react';
import { createRoot } from 'react-dom/client';
import { I18nextProvider } from 'react-i18next';
import { SessionFault } from '@omochi-ui/core/browser';
import { test, type TestContext } from '@omochi-ui/core/testing';
import type { BrowserTransport } from '@omochi-ui/hub-transport';
import { ConnectionController, type ConnectionState } from '../src/remote/connection-controller.js';
import type { ShareLink } from '../src/remote/link.js';
import { openHubProfile } from '../src/remote/profile.js';
import { OwnerInvitation } from '../src/remote/OwnerInvitation.js';
import { createDashboardI18n } from '../src/i18n.js';

const publicLink: ShareLink = {
  version: 2, mode: 'share', hubId: 'a'.repeat(64),
  grantId: '11111111-1111-4111-8111-111111111111',
  relayUrls: ['https://relay.example/'], secret: 'b'.repeat(64),
};
const ownerLink: ShareLink = {
  ...publicLink, version: 1, mode: 'owner', grantId: '22222222-2222-4222-8222-222222222222',
};
const saved = {
  hubId: ownerLink.hubId, grantId: ownerLink.grantId,
  relayUrls: ownerLink.relayUrls, owner: true as const, name: 'Owner hub',
};
const savedId = `${saved.hubId}/${saved.grantId}`;
const fragment = (link: ShareLink) => `#${link.mode}:${Buffer.from(JSON.stringify(link)).toString('base64url')}`;

function fixture(t: TestContext) {
  const previous = ['indexedDB', 'navigator', 'sessionStorage'].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)] as const);
  let locked = false, probes = 0;
  const storage = new Map<string, string>();
  Object.defineProperty(globalThis, 'indexedDB', { configurable: true, value: new IDBFactory() });
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { locks: {
    async request(_name: string, _options: unknown, callback: (lock: object | null) => Promise<unknown>) {
      if (locked) return callback(null);
      locked = true;
      try { return await callback({}); } finally { locked = false; }
    },
  } } });
  Object.defineProperty(globalThis, 'sessionStorage', { configurable: true, value: {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => { storage.set(key, value); },
    removeItem: (key: string) => { storage.delete(key); },
  } });
  const controllers: ConnectionController[] = [];
  t.after(async () => {
    for (const controller of controllers) await controller.dispose();
    for (const [key, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else Reflect.deleteProperty(globalThis, key);
    }
  });
  const identity = { endpointId: 'c'.repeat(64), secretHex: 'd'.repeat(64) };
  const transport: BrowserTransport = {
    async generateIdentity() { return identity; },
    async identityFromSecret() { return identity; },
    createDownstreamState() { throw new Error('The view starts render sessions'); },
    isRetryable() { return false; },
    async openEndpoint() {
      probes++;
      return {
        async connect() {
          return {
            remoteId: ownerLink.hubId,
            async open() {
              return {
                async send() {},
                async receive() { return { json: '{"kind":"ownerWelcome"}', wireBytes: 23 }; },
                async finish() {}, async cancel() {},
              };
            },
            async closed() { return { code: '0', reason: '' }; },
            async close() {},
          };
        },
        async close() {},
      };
    },
  };
  return {
    transport, identity, storage,
    get probes() { return probes; },
    get locked() { return locked; },
    create(initialLink: ShareLink | null, requestLeave = async () => true) {
      const controller = new ConnectionController({ transport, initialLink, bootstrapError: null, requestLeave });
      controllers.push(controller);
      return controller;
    },
    async seed() {
      const profile = await openHubProfile(transport);
      try { await profile.save(saved, 'accepted'); } finally { await profile.close(); }
    },
  };
}

function until(controller: ConnectionController, check: (state: ConnectionState) => boolean): Promise<void> {
  if (check(controller.getSnapshot())) return Promise.resolve();
  return new Promise(resolve => {
    const unsubscribe = controller.subscribe(state => {
      if (check(state)) { unsubscribe(); resolve(); }
    });
  });
}

test('a saved selection owns the complete switch, keeps cancelled shares intact and restores the owner on reload', async t => {
  const f = fixture(t);
  await f.seed();
  let leave = false;
  const controller = f.create(null, async () => leave);
  await controller.start();
  leave = true;
  controller.receive(fragment(publicLink));
  await until(controller, state => !state.busy && state.active?.kind === 'public');
  const active = controller.getSnapshot().active;
  const publicContext = f.storage.get('omochi.shared.public');
  leave = false;
  await controller.dispatch({ type: 'select', id: savedId });
  assert.equal(controller.getSnapshot().active, active);
  assert.equal(f.storage.get('omochi.shared.public'), publicContext);
  assert.equal(f.storage.has('omochi.shared.selected'), false);

  leave = true;
  await controller.dispatch({ type: 'select', id: savedId });
  assert.equal(controller.getSnapshot().active?.kind, 'saved');
  assert.equal(f.storage.has('omochi.shared.public'), false);
  assert.equal(f.storage.get('omochi.shared.selected'), savedId);
  controller.receive('#owner:invalid');
  await until(controller, state => !state.busy);
  assert.equal(controller.getSnapshot().error?.code, 'InvalidInvitation');
  assert.equal(controller.getSnapshot().active?.id, savedId);

  await controller.dispose();
  const restored = f.create(null);
  await restored.start();
  assert.equal(restored.getSnapshot().active?.id, savedId);
  assert.equal(restored.getSnapshot().active?.kind, 'saved');
  assert.equal(f.probes, 0, 'selecting a saved owner never enrolls again');
});

test('pending owner invitations and their dismissal never replace the active public session or handoff payload', async t => {
  const f = fixture(t);
  const controller = f.create(publicLink);
  await controller.start();
  assert.equal(f.locked, false, 'public viewing does not claim a profile');
  const active = controller.getSnapshot().active;
  controller.receive(fragment(ownerLink));
  await until(controller, state => !state.busy && !!state.browserId);
  assert.equal(controller.getSnapshot().invitation?.kind, 'invitation');
  assert.equal(controller.getSnapshot().active, active);
  await controller.dispatch({ type: 'dismiss' });
  assert.equal(controller.getSnapshot().invitation, null);
  assert.equal(controller.getSnapshot().active, active);
  assert.deepEqual(JSON.parse(f.storage.get('omochi.shared.public')!), publicLink);
  controller.receive(fragment(publicLink));
  assert.equal(controller.getSnapshot().active, active, 'the active share is still recognized as a duplicate');
  assert.equal(f.probes, 0);
});

test('owner invitation retries acquire the profile after another tab releases it while preserving the public share', async t => {
  const f = fixture(t);
  const otherTab = f.create(null);
  await otherTab.start();
  const controller = f.create(publicLink);
  await controller.start();
  const active = controller.getSnapshot().active;
  controller.receive(fragment(ownerLink));
  await until(controller, state => !state.busy);
  assert.equal(controller.getSnapshot().error?.code, 'OwnerBusy');
  assert.equal(controller.getSnapshot().browserId, null);

  await controller.dispatch({ type: 'retry' });
  assert.equal(controller.getSnapshot().error?.code, 'OwnerBusy', 'failed profile retries remain recoverable');
  assert.equal(controller.getSnapshot().browserId, null);
  assert.equal(controller.getSnapshot().active, active);

  await otherTab.dispose();
  await controller.dispatch({ type: 'retry' });
  assert.equal(controller.getSnapshot().error, null);
  assert.equal(controller.getSnapshot().browserId, f.identity.endpointId);
  assert.deepEqual(controller.getSnapshot().invitation, { kind: 'invitation', link: ownerLink });
  assert.equal(controller.getSnapshot().active, active);
  assert.deepEqual(JSON.parse(f.storage.get('omochi.shared.public')!), publicLink);
  assert.equal(f.probes, 0, 'profile recovery does not enroll before acceptance');

  await controller.dispatch({ type: 'accept' });
  assert.equal(controller.getSnapshot().error, null);
  assert.equal(controller.getSnapshot().invitation, null);
  assert.equal(controller.getSnapshot().active?.id, savedId);
  assert.equal(controller.getSnapshot().active?.kind, 'saved');
  assert.equal(controller.getSnapshot().records[0]?.state, 'accepted');
  assert.equal(f.probes, 1);
});

test('failed enrollment retains recovery until dismissal refreshes pending access, which can be retried without reloading', async t => {
  const f = fixture(t);
  const controller = f.create(ownerLink);
  await controller.start();
  const openEndpoint = f.transport.openEndpoint;
  f.transport.openEndpoint = async () => { throw new SessionFault('NetworkUnavailable'); };
  await controller.dispatch({ type: 'accept' });
  assert.equal(controller.getSnapshot().error?.code, 'NetworkUnavailable');
  assert.equal(controller.getSnapshot().invitation?.kind, 'invitation');
  assert.equal(controller.getSnapshot().active, null);

  const getAll = IDBObjectStore.prototype.getAll;
  IDBObjectStore.prototype.getAll = function(...args) {
    if (this.name === 'shares') throw new DOMException('unavailable', 'InvalidStateError');
    return getAll.apply(this, args);
  };
  t.after(() => { IDBObjectStore.prototype.getAll = getAll; });
  await controller.dispatch({ type: 'dismiss' });
  assert.equal(controller.getSnapshot().error?.code, 'StorageUnavailable');
  assert.equal(controller.getSnapshot().invitation?.kind, 'invitation', 'failed refresh must keep recovery visible');

  IDBObjectStore.prototype.getAll = getAll;
  await controller.dispatch({ type: 'dismiss' });
  assert.equal(controller.getSnapshot().invitation, null);
  assert.equal(controller.getSnapshot().error, null);
  assert.deepEqual(controller.getSnapshot().records, [{ ...saved, id: savedId, state: 'pending' }]);

  f.transport.openEndpoint = openEndpoint;
  await controller.dispatch({ type: 'select', id: savedId });
  assert.equal(controller.getSnapshot().error, null);
  assert.equal(controller.getSnapshot().invitation, null);
  assert.equal(controller.getSnapshot().active?.id, savedId);
  assert.equal(controller.getSnapshot().records[0]?.state, 'accepted');
  assert.equal(f.probes, 1);
});

test('Escape keeps the owner invitation and recovery controls visible when dismissal cannot read storage', async t => {
  const f = fixture(t);
  const controller = f.create(ownerLink);
  await controller.start();
  const actEnvironment = Object.getOwnPropertyDescriptor(globalThis, 'IS_REACT_ACT_ENVIRONMENT');
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { configurable: true, value: true });
  t.after(() => {
    if (actEnvironment) Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', actEnvironment);
    else Reflect.deleteProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT');
  });
  const container = document.createElement('div');
  document.body.append(container);
  t.after(() => container.remove());
  const root = createRoot(container);
  t.after(() => act(() => root.unmount()));
  function Invitation() {
    const state = useSyncExternalStore(changed => controller.subscribe(changed), () => controller.getSnapshot());
    return state.invitation && createElement(OwnerInvitation, {
      invitation: state.invitation, busy: state.busy, ownsProfile: !!state.browserId,
      error: state.error, dispatch: action => controller.dispatch(action),
    });
  }
  await act(async () => {
    root.render(createElement(I18nextProvider, {
      i18n: createDashboardI18n({ lng: 'en', detection: { caches: [] } }),
    }, createElement(Invitation)));
  });
  const dialog = container.querySelector('dialog');
  assert.ok(dialog?.open);
  const getAll = IDBObjectStore.prototype.getAll;
  IDBObjectStore.prototype.getAll = function(...args) {
    if (this.name === 'shares') throw new DOMException('unavailable', 'InvalidStateError');
    return getAll.apply(this, args);
  };
  t.after(() => { IDBObjectStore.prototype.getAll = getAll; });

  const escape = new Event('cancel', { cancelable: true });
  await act(async () => {
    dialog.dispatchEvent(escape);
    await until(controller, state => !state.busy);
  });
  assert.equal(escape.defaultPrevented, true);
  assert.equal(controller.getSnapshot().error?.code, 'StorageUnavailable');
  assert.equal(dialog.open, true, 'failed dismissal must leave the modal open');
  assert.ok(dialog.querySelector('[role="alert"][data-error-code="StorageUnavailable"]'));
  assert.equal(dialog.querySelector<HTMLButtonElement>('.dialog-actions .primary')?.disabled, false);
  assert.equal(dialog.querySelector<HTMLButtonElement>('.dialog-header button')?.disabled, false);

  IDBObjectStore.prototype.getAll = getAll;
  await act(async () => {
    dialog.dispatchEvent(new Event('cancel', { cancelable: true }));
    await until(controller, state => !state.busy);
  });
  assert.equal(controller.getSnapshot().invitation, null);
  assert.equal(controller.getSnapshot().error, null);
  assert.equal(dialog.open, false, 'successful dismissal closes the modal through state cleanup');
  assert.equal(container.querySelector('dialog'), null);
});

for (const queueNavigation of [false, true]) {
  test(`local-save recovery ignores duplicate invitations ${queueNavigation ? 'without replacing queued navigation' : 'without reopening them'}`, async t => {
    const f = fixture(t);
    const controller = f.create(ownerLink);
    await controller.start();
    const put = IDBObjectStore.prototype.put;
    let failSave = true;
    IDBObjectStore.prototype.put = function(value, key) {
      if (this.name === 'shares' && value.state === 'accepted' && failSave) throw new DOMException('quota', 'QuotaExceededError');
      return key === undefined ? put.call(this, value) : put.call(this, value, key);
    };
    t.after(() => { IDBObjectStore.prototype.put = put; });
    await controller.dispatch({ type: 'accept' });
    assert.equal(controller.getSnapshot().invitation?.kind, 'savePending');
    assert.equal(controller.getSnapshot().error?.code, 'StorageUnavailable');
    assert.equal(f.probes, 1);
    controller.receive(fragment(ownerLink));
    await controller.dispatch({ type: 'accept' });
    assert.equal(controller.getSnapshot().invitation?.kind, 'savePending', 'another failed save keeps the invitation identity');
    const next = { ...ownerLink, grantId: '33333333-3333-4333-8333-333333333333' };
    if (queueNavigation) {
      controller.receive(fragment(publicLink));
      controller.receive(fragment(next));
    }
    controller.receive(fragment(ownerLink));
    await controller.dispatch({ type: 'dismiss' });
    assert.equal(controller.getSnapshot().invitation?.kind, 'savePending', 'proof cannot be discarded by navigation or dismissal');
    failSave = false;
    await controller.dispatch({ type: 'accept' });
    await until(controller, state => !state.busy);
    assert.deepEqual(controller.getSnapshot().invitation, queueNavigation ? { kind: 'invitation', link: next } : null);
    assert.equal(controller.getSnapshot().active?.id, savedId);
    assert.equal(controller.getSnapshot().records[0]?.state, 'accepted');
    assert.equal(f.probes, 1, 'retry writes the confirmed proof locally');
  });
}

test('disposing during identity creation prevents late activation and publishes no further state', async t => {
  const f = fixture(t);
  let resolve!: (value: typeof f.identity) => void;
  f.transport.generateIdentity = () => new Promise(done => { resolve = done; });
  const controller = f.create(publicLink);
  const starting = controller.start();
  await controller.dispose();
  const closed = controller.getSnapshot();
  resolve(f.identity);
  await starting;
  assert.equal(controller.getSnapshot(), closed);
  assert.equal(closed.active, null);
  assert.equal(f.locked, false);
});

test('disposal releases an in-flight profile acquisition before a replacement controller opens it', async t => {
  const f = fixture(t);
  let generated!: () => void;
  const generating = new Promise<void>(resolve => { generated = resolve; });
  let finish!: (value: typeof f.identity) => void;
  f.transport.generateIdentity = () => {
    generated();
    return new Promise(resolve => { finish = resolve; });
  };
  const controller = f.create(null);
  const starting = controller.start();
  await generating;
  const disposing = controller.dispose();
  finish(f.identity);
  await disposing;
  await starting;
  assert.equal(f.locked, false);
  const replacement = f.create(null);
  await replacement.start();
  assert.equal(replacement.getSnapshot().browserId, f.identity.endpointId);
  assert.equal(replacement.getSnapshot().error, null);
});
