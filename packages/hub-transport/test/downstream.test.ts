import { expect, test } from 'bun:test';
import assert from 'node:assert/strict';
import { createTransportFacade } from '../src/index.js';
import { FakeDownstreamState, fixture } from './fixtures.js';

test('downstream facade delegates application and acknowledgement and frees state once', () => {
  const f = fixture();
  let raw!: FakeDownstreamState;
  f.module.DownstreamState = class extends FakeDownstreamState { constructor() { super(); raw = this; } };
  const state = createTransportFacade(f.module).createDownstreamState('invite', { interact: true, editLayout: false });
  expect(state.accept({ kind: 'snapshotEnd', sequence: '18446744073709551615' }).sequence).toBe('18446744073709551615');
  expect(raw.acknowledgements).toEqual([]);
  state.acknowledge('18446744073709551615');
  expect(raw.acknowledgements).toEqual(['18446744073709551615']);
  state.close(); state.close();
  expect(raw.closes).toBe(1); expect(raw.frees).toBe(1);
  assert.throws(() => state.accept({}), { code: 'ScopeExpired' });
});

test('downstream facade preserves typed core rejection codes', () => {
  const f = fixture();
  f.module.DownstreamState = class extends FakeDownstreamState { accept(): never { throw 'ProtocolError'; } };
  const state = createTransportFacade(f.module).createDownstreamState('owner', { interact: true, editLayout: true });
  try { assert.throws(() => state.accept({}), { code: 'ProtocolError' }); } finally { state.close(); }
});

test('retry decisions come from the shared core export', () => {
  const f = fixture();
  const calls: string[] = [];
  f.module.downstream_retryable = code => { calls.push(code); return code === 'Timeout'; };
  const transport = createTransportFacade(f.module);
  expect(transport.isRetryable('Timeout')).toBe(true);
  expect(transport.isRetryable('RenderError')).toBe(false);
  expect(calls).toEqual(['Timeout', 'RenderError']);
});
