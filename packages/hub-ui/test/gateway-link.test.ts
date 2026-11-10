import { test, expect } from 'bun:test';
import { parseGatewayLink } from '../src/remote/gateway-link.js';
const value = {
  version: 4,
  address: { id: 'a'.repeat(64), addrs: [] },
  secret: 'b'.repeat(64),
  expires: Math.floor(Date.now() / 1000) + 180,
};
const link = (v: unknown) =>
  '#g:' + Buffer.from(JSON.stringify(v)).toString('base64url');
test('gateway URL shares the CLI payload without a nested encoding', () => {
  const fragment = link(value),
    decoded = parseGatewayLink(fragment);
  expect(decoded.gatewayId).toBe(value.address.id);
  expect(decoded.invitation).toBe('omochi://invite/' + fragment.slice(3));
});
test('rejects stale, malformed, unsupported and unknown invitation fields', () => {
  for (const fragment of [
    link({ ...value, expires: 0 }),
    link({ ...value, version: 3 }),
    link({ ...value, extra: 1 }),
    link({ ...value, secret: 'oops' }),
    '#g:====',
    '#connect:abc',
    '#g:' + 'a'.repeat(16384),
  ])
    expect(() => parseGatewayLink(fragment)).toThrow();
});

test('shared hosted invitation fixtures', async () => {
  const fixtures = await Bun.file(
    new URL('../../../spec/fixtures/gateway-invitations.json', import.meta.url),
  ).json();
  for (const item of fixtures.cases) {
    if (item.input.startsWith('https:'))
      expect(parseGatewayLink(new URL(item.input).hash).gatewayId).toBe(
        'a'.repeat(64),
      );
  }
});
