import { expect, test } from 'bun:test';
import { nativeInvitationLink } from '../src/remote/native-link.js';
test('native handoff preserves each payload without a server query', () => {
  expect(nativeInvitationLink('#g:abc_12')).toBe('omochi://invite/abc_12');
  expect(nativeInvitationLink('#share:abc-12')).toBe('omochi://share/abc-12');
  expect(nativeInvitationLink('#owner:abc_12')).toBe('omochi://owner/abc_12');
  expect(nativeInvitationLink('#owner:abc?secret=x')).toBeNull();
  expect(nativeInvitationLink('#unknown:abc')).toBeNull();
});

import fixture from '../../../spec/fixtures/hub-invitations.json' with { type: 'json' };
import { parseShareLink } from '../src/remote/link.js';
test('web and native invitation fixtures agree', () => {
  for (const entry of fixture.cases) {
    if (entry.valid) expect(() => parseShareLink(entry.fragment)).not.toThrow();
    else expect(() => parseShareLink(entry.fragment)).toThrow();
    if (entry.valid)
      expect(nativeInvitationLink(entry.fragment)).toStartWith('omochi://');
  }
});
