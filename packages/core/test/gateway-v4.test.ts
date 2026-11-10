import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from '../src/testing.js';
import {
  BASIC_CATALOG_ID,
  isU64Decimal,
  parseControlFrame,
  parseServiceFrame,
  validateClientMessage,
  validateServerMessage,
} from '../src/gateway-v4.js';

const fixtures = JSON.parse(
  readFileSync(
    new URL('../../../spec/fixtures/browser-gateway-v4.json', import.meta.url),
    'utf8',
  ),
) as {
  cases: {
    name: string;
    kind: 'control' | 'service';
    wire: string;
    bridge?: unknown;
    valid: boolean;
  }[];
};
for (const fixture of fixtures.cases)
  test(fixture.name, () => {
    const parse =
      fixture.kind === 'control' ? parseControlFrame : parseServiceFrame;
    const value = fixture.bridge ?? JSON.parse(fixture.wire);
    if (fixture.valid) assert.deepEqual(parse(value), value);
    else assert.throws(() => parse(value), { code: 'ProtocolError' });
  });

test('bridge identity integers are canonical decimal u64 strings, never rounded JS numbers', () => {
  for (const value of [
    1,
    0,
    18446744073709551615,
    '01',
    '+1',
    '-1',
    '',
    '1.0',
    '18446744073709551616',
    '0',
  ])
    assert.equal(isU64Decimal(value), false);
  assert.equal(isU64Decimal('1'), true);
  assert.equal(isU64Decimal('18446744073709551615'), true);
  assert.equal(isU64Decimal('0', false), true);
});

test('complete roster validation enforces metadata and count limits atomically', () => {
  const service = { id: 'app', name: 'App', generation: '1' };
  const services = Array.from({ length: 257 }, (_, i) => ({
    ...service,
    id: `app${i}`,
  }));
  assert.throws(() => parseControlFrame({ kind: 'services', services }), {
    code: 'ProtocolError',
  });
  assert.throws(
    () =>
      parseControlFrame({
        kind: 'services',
        services: [{ ...service, name: ' ' }],
      }),
    { code: 'ProtocolError' },
  );
  assert.throws(
    () =>
      parseControlFrame({
        kind: 'services',
        services: [{ ...service, icon: 'x'.repeat(1024 * 1024) }],
      }),
    { code: 'CapacityExceeded' },
  );
  assert.throws(
    () =>
      parseControlFrame({
        kind: 'services',
        services: [{ ...service, address: 'untrusted' }],
      }),
    { code: 'ProtocolError' },
  );
});

test('the exact roster limit counts native generation integers, excluding bridge-only quotes', () => {
  const native = {
    kind: 'services',
    services: [
      {
        id: 'app',
        name: 'App',
        generation: 18446744073709551615n.toString(),
        icon: '',
      },
    ],
  };
  // Native u64 has the same digits but no string quotes.
  const base = new TextEncoder().encode(JSON.stringify(native)).length - 2;
  native.services[0]!.icon = 'x'.repeat(1024 * 1024 - base);
  assert.deepEqual(parseControlFrame(native), native);
  native.services[0]!.icon += 'x';
  assert.throws(() => parseControlFrame(native), { code: 'CapacityExceeded' });
});

test('direct messages use bounded nonblank Surface IDs and the installed basic catalog', () => {
  const create = (surfaceId: string, catalogId = BASIC_CATALOG_ID) => ({
    version: 'v0.9',
    createSurface: { surfaceId, catalogId },
  });
  validateServerMessage(create('nested/main'));
  validateServerMessage(create('x'.repeat(4096)));
  for (const id of ['', ' ', 'x'.repeat(4097)])
    assert.throws(() => validateServerMessage(create(id)), {
      code: 'ProtocolError',
    });
  assert.throws(
    () =>
      validateServerMessage(
        create('main', 'https://example.invalid/untrusted'),
      ),
    { code: 'ProtocolError' },
  );
  assert.throws(
    () =>
      validateServerMessage({
        version: 'v0.9',
        updateDataModel: { surfaceId: 'main', value: undefined },
      }),
    { code: 'ProtocolError' },
  );
});

test('raw form metadata is retained by validation and JSON-like missing drafts are allowed', () => {
  const raw = {
    version: 'v0.9',
    action: {
      surfaceId: 'main',
      name: 'save',
      sourceComponentId: 'root',
      timestamp: '2026-11-03T00:00:00Z',
      context: {},
    },
    metadata: {
      a2uiClientDataModel: {
        version: 'v0.9',
        surfaces: { main: { draft: [1, undefined], omitted: undefined } },
      },
    },
  };
  assert.equal(validateClientMessage(raw), raw);
  assert.throws(
    () =>
      validateClientMessage({
        ...raw,
        action: { ...raw.action, timestamp: '' },
      }),
    { code: 'ProtocolError' },
  );
  assert.throws(
    () =>
      validateClientMessage({
        ...raw,
        error: { surfaceId: 'main', code: 'oops', message: 'failed' },
      }),
    { code: 'ProtocolError' },
  );
});
