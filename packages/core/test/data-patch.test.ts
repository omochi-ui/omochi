import assert from 'node:assert/strict';
import { test } from '../src/testing.js';
import { planDataPatch } from '../src/data-patch.js';
import { jsonBytes } from '../src/json-size.js';
import { A2uiSnapshot } from '../src/a2ui-snapshot.js';

test('pointer patches match native snapshot semantics including holes and empty segments', () => {
  const fixtures: [unknown, string, boolean, unknown][] = [
    [undefined, '/0', true, 'first'], [undefined, '/a/b', false, undefined],
    [null, '/x', true, 2], [4, '/a/b', true, null],
    [[1, 2, 3], '/1', false, undefined], [[1], '/4', true, 5],
    [{ a: 1 }, '/a/0', true, 2], [{}, '//x', true, 'empty key'],
    [{}, '/a~1b/~0', true, 1], [{}, '/01', true, 2],
    [{ x: 1 }, '/', true, null], [{ x: 1 }, '', false, undefined],
  ];
  for (const [root, path, present, value] of fixtures) {
    const snapshot = new A2uiSnapshot();
    snapshot.apply({ version: 'v0.9', createSurface: { surfaceId: 's', catalogId: 'test' } });
    if (root !== undefined) snapshot.apply({ version: 'v0.9', updateDataModel: { surfaceId: 's', value: root } });
    snapshot.apply({ version: 'v0.9', updateDataModel: { surfaceId: 's', path, ...(present ? { value } : {}) } });
    const expected = snapshot.messages().at(-1)!;
    assert.ok('updateDataModel' in expected);
    const plan = planDataPatch(root, path, present, value);
    const result = plan.materialize();
    assert.equal(plan.byteSize, jsonBytes(result));
    assert.deepEqual(result, expected.updateDataModel.value === undefined ? undefined : JSON.parse(JSON.stringify(expected.updateDataModel.value)));
  }
});

test('plans reject path/prototype/depth errors before any sparse expansion', () => {
  for (const path of ['a', '/constructor/x', '/prototype', '/__proto__', '/100001', '/50001/50000', '/a'.repeat(129)]) {
    assert.throws(() => planDataPatch({}, path, true, 1), { code: 'ProtocolError' });
  }
  assert.throws(() => planDataPatch([], '/01', true, 1), { code: 'ProtocolError' });
  const plan = planDataPatch(undefined, '/100000', true, 1);
  assert.equal(plan.byteSize, 500003);
  let value: unknown = 1;
  for (let i = 0; i < 128; i++) value = [value];
  assert.throws(() => planDataPatch({}, '/x', true, value), { code: 'ProtocolError' });
});

test('materialization detaches input and normalizes undefined without losing drafts', () => {
  const root = { draft: { text: 'editing' }, rows: [undefined, { x: 1 }] };
  const input = { nested: [2] };
  const plan = planDataPatch(root, '/remote', true, input);
  const output = plan.materialize() as typeof root & { remote: typeof input };
  output.draft.text = 'changed'; output.remote.nested.push(3);
  assert.equal(root.draft.text, 'editing'); assert.deepEqual(input.nested, [2]);
  assert.equal(output.rows[0], null);
  assert.equal(plan.byteSize, jsonBytes(plan.materialize()));
});
