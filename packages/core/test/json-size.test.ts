import assert from 'node:assert/strict';
import { test } from '../src/testing.js';
import { jsonBytes, measureJson } from '../src/json-size.js';

test('JSON size matches compact wire serialization without encoding a complete tree', () => {
  const values = [null, true, false, 0, -0, 1e-20, 1e30, '', '"\\\b\f\n\r\t\u0000\u001f',
    'ASCII 日本語 é 😀 \ud800 \udfff', [1, , undefined, null],
    { '😀"': { x: [null, 'é'], omitted: undefined } }, undefined];
  for (const value of values) {
    assert.equal(jsonBytes(value), value === undefined ? 0 : new TextEncoder().encode(JSON.stringify(value)).length);
  }
});

test('invalid client JSON never invokes accessors and rejects cycles, keys and excessive depth', () => {
  let calls = 0;
  const getter = { get x() { calls++; return 'secret'; } };
  const cycle: unknown[] = []; cycle.push(cycle);
  const invalid = [getter, cycle, NaN, Infinity, 1n, new Date(), () => 1,
    JSON.parse('{"__proto__":1}'), { nested: { constructor: 1 } }, { [Symbol()]: 1 }];
  for (const value of invalid) assert.throws(() => jsonBytes(value), { code: 'ProtocolError' });
  assert.equal(calls, 0);
  let root: unknown = 1;
  for (let i = 0; i < 128; i++) root = [root];
  assert.equal(measureJson(root).maxDepth, 128);
  assert.throws(() => jsonBytes([root]), { code: 'ProtocolError' });
});

test('only deeply immutable trees have cached sizes', () => {
  const child = { text: 'a' };
  const root = Object.freeze({ child });
  const first = jsonBytes(root);
  child.text = 'longer';
  assert.equal(jsonBytes(root), first + 5);
  Object.freeze(child);
  assert.equal(jsonBytes(root), new TextEncoder().encode(JSON.stringify(root)).length);
});

test('sparse measurement counts gaps arithmetically', () => {
  const value = new Array(100000000);
  value[99999999] = 'a';
  assert.equal(jsonBytes(value), 500000000);
});
