import assert from 'node:assert/strict';
import { test } from '@omochi-ui/core/testing';
import {
  OwnerBoardRepository,
  OwnerSharingRepository,
} from '../src/remote/owner-boards.js';
import { parseShareLink, validSavedShare } from '../src/remote/link.js';

test('owner adapters validate returned boards and presets and never expose owner administration', async () => {
  let result: unknown = [{ id: 'preset', name: 'Preset', panels: [] }];
  const requests: unknown[] = [];
  const session = {
    async control(command: unknown) {
      requests.push(command);
      return result;
    },
  };
  const boards = new OwnerBoardRepository(session);
  assert.deepEqual(await boards.presets(), result);
  result = { id: 'other', revision: 1, name: 'Board', panels: [] };
  await assert.rejects(boards.get('expected'), { code: 'StorageCorrupt' });
  result = [{ id: 'bad', revision: 0, name: 'Board', panels: [] }];
  await assert.rejects(boards.list(), { code: 'StorageCorrupt' });
  const sharing = new OwnerSharingRepository(session),
    count = requests.length;
  await assert.rejects(sharing.request('/api/owners', 'POST'));
  await assert.rejects(sharing.request('/api/owners/an-owner', 'DELETE'));
  assert.equal(requests.length, count);
  await assert.rejects(boards.delete('b', 0), { code: 'Conflict' });
});

test('owner fragments and saved authority cannot be confused with ordinary sharing links', () => {
  const payload = {
    version: 1,
    hubId: 'a'.repeat(64),
    relayUrls: ['https://relay.example/'],
    grantId: '11111111-1111-4111-8111-111111111111',
    mode: 'owner',
    secret: 'b'.repeat(64),
  };
  const encoded = Buffer.from(JSON.stringify(payload)).toString('base64url');
  assert.equal(parseShareLink(`#owner:${encoded}`).mode, 'owner');
  assert.throws(() => parseShareLink(`#share:${encoded}`), {
    code: 'InvalidInvitation',
  });
  const saved = {
    hubId: payload.hubId,
    relayUrls: payload.relayUrls,
    grantId: payload.grantId,
    name: 'Hub',
  };
  assert(validSavedShare(saved));
  assert(validSavedShare({ ...saved, owner: true }));
  assert(!validSavedShare({ ...saved, owner: false }));
});
