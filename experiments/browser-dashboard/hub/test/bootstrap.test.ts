import assert from 'node:assert/strict';
import { test } from '@omochi-ui/core/testing';
import { captureConnectFragment, parseConnectFragment, parseInvitation, validRelayUrl } from '../src/bootstrap.js';

const native = (value: unknown) => `omochi://invite/${Buffer.from(JSON.stringify(value)).toString('hex')}`;
const invitation = (extra = {}) => native({ version: 4, address: { id: 'a'.repeat(64), addrs: [{ Ip: '127.0.0.1:1' }, { Relay: 'https://relay.example/' }] }, secret: 'AB'.repeat(32), expires: Math.floor(Date.now() / 1000) + 180, ...extra });
const wrapper = (value: unknown) => `#connect:${Buffer.from(JSON.stringify(value)).toString('base64url')}`;

test('bootstrap accepts only the bounded versioned wrapper, retains the exact source and ignores IP hints', () => {
  const text = invitation();
  assert.equal(parseConnectFragment(wrapper({ version: 1, invitation: text })), text);
  const parsed = parseInvitation(text);
  assert.equal(parsed.nodeId, 'a'.repeat(64)); assert.deepEqual(parsed.relayUrls, ['https://relay.example/']);
  assert.equal(Object.isFrozen(parsed), true); assert.equal(Object.isFrozen(parsed.relayUrls), true);
  assert.equal(parsed.secret, 'AB'.repeat(32));
  for (const value of [{ version: 2, invitation: text }, { version: 1, invitation: text, extra: true }, { invitation: text }, []]) {
    assert.throws(() => parseConnectFragment(wrapper(value)), { code: 'InvalidInvitation' });
  }
  for (const value of ['', '#connect:=', '#connect:a', '#connect:AB', '#connect:' + 'a'.repeat(32768), '#connect:' + Buffer.from([255]).toString('base64url')]) {
    assert.throws(() => parseConnectFragment(value), { code: 'InvalidInvitation' });
  }
});

test('native invitation expiry, secrets, bounds, nested fields and relay policy fail before network/storage work', () => {
  for (const extra of [{ version: 3 }, { secret: 'x'.repeat(64) }, { secret: 'a'.repeat(63) }, { expires: Math.floor(Date.now() / 1000) },
    { expires: Number.MAX_SAFE_INTEGER }, { expires: 1.5 }, { extra: 'unsupported' },
    { address: { id: 'a'.repeat(16), addrs: [{ Relay: 'https://r/' }] } },
    { address: { id: 'a'.repeat(64), addrs: [{ Relay: 'http://r/' }] } },
    { address: { id: 'a'.repeat(64), addrs: [{ Ip: '127.0.0.1:1' }] } },
    { address: { id: 'a'.repeat(64), addrs: [{ Relay: 'https://r/' }], extra: true } },
    { address: { id: 'a'.repeat(64), addrs: Array.from({ length: 5 }, (_, i) => ({ Relay: `https://r${i}/` })) } },
    { address: { id: 'a'.repeat(64), addrs: [{ Relay: 'https://r/', Ip: '127.0.0.1:1' }] } },
  ]) assert.throws(() => parseInvitation(invitation(extra)), { code: 'InvalidInvitation' });
  for (const relay of ['https://user:pass@r/', 'https://@r/', 'https:r', 'https://r/?x', 'https://r/#', 'https://r/?', 'wss://r/', 'https://r/' + 'x'.repeat(2048), ' https://r/']) assert.equal(validRelayUrl(relay), false);
  for (const text of ['omochi://invite/zz', 'omochi://invite/0', invitation() + 'a'.repeat(16384)]) assert.throws(() => parseInvitation(text), { code: 'InvalidInvitation' });
});

test('capture removes bearer fragments from browser history even when malformed', t => {
  const oldLocation = Object.getOwnPropertyDescriptor(globalThis, 'location');
  const oldHistory = Object.getOwnPropertyDescriptor(globalThis, 'history');
  t.after(() => {
    if (oldLocation) Object.defineProperty(globalThis, 'location', oldLocation); else Reflect.deleteProperty(globalThis, 'location');
    if (oldHistory) Object.defineProperty(globalThis, 'history', oldHistory); else Reflect.deleteProperty(globalThis, 'history');
  });
  const replaced: unknown[] = [];
  Object.defineProperty(globalThis, 'location', { configurable: true, value: { hash: '#connect:invalid', pathname: '/ui/', search: '' } });
  Object.defineProperty(globalThis, 'history', { configurable: true, value: { replaceState: (...args: unknown[]) => replaced.push(args) } });
  assert.throws(captureConnectFragment, { code: 'InvalidInvitation' });
  assert.deepEqual(replaced, [[null, '', '/ui/']]);
});
