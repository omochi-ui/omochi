import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { Ajv2020 } from 'ajv/dist/2020.js';
import { test } from '@omochi-ui/core/testing';
import {
  BOARD_FILE_LIMIT,
  exportBoard,
  parseBoard,
  validateBoardInput,
} from '../src/board-codec.js';
import type { DashboardInput, Panel } from '../src/contracts.js';

interface Document {
  format: string;
  version: number;
  board: DashboardInput;
  sources: { nodeId: string; name?: string }[];
}
const fixtures = JSON.parse(
  readFileSync(
    new URL('../../../spec/fixtures/board-export.json', import.meta.url),
    'utf8',
  ),
) as {
  valid: { name: string; document: Document }[];
  invalid: {
    name: string;
    base: string;
    changes: { op: 'set' | 'delete'; path: string; value?: unknown }[];
    expected: string;
  }[];
};
const schema = JSON.parse(
  readFileSync(
    new URL('../../../spec/board-export.schema.json', import.meta.url),
    'utf8',
  ),
);
const schemaValidate = new Ajv2020({ strict: true }).compile(schema);
const encode = (value: unknown) =>
  new TextEncoder().encode(JSON.stringify(value));
const one = () =>
  structuredClone(
    fixtures.valid.find((value) => value.name === 'one-panel')!.document,
  );
function mutated(invalid: (typeof fixtures.invalid)[number]): Document {
  const document = structuredClone(
    fixtures.valid.find((value) => value.name === invalid.base)!.document,
  );
  for (const change of invalid.changes) {
    const path = change.path
      .slice(1)
      .split('/')
      .map((value) => value.replace(/~1/g, '/').replace(/~0/g, '~'));
    let parent = document as unknown as Record<string, unknown>;
    for (const key of path.slice(0, -1))
      parent = parent[key] as Record<string, unknown>;
    const key = path.at(-1)!;
    if (change.op === 'delete') Reflect.deleteProperty(parent, key);
    else
      Object.defineProperty(parent, key, {
        value: change.value,
        writable: true,
        configurable: true,
        enumerable: true,
      });
  }
  return document;
}

test('shared board documents satisfy JSON Schema and the strict semantic codec; every rejection fixture is rejected', () => {
  for (const fixture of fixtures.valid) {
    assert.equal(schemaValidate(fixture.document), true, fixture.name);
    const parsed = parseBoard(encode(fixture.document));
    assert.deepEqual(parsed.input, fixture.document.board);
    const exported = exportBoard(
      { ...parsed.input, id: 'not exported', revision: 12 },
      parsed.sources,
    );
    assert.equal(
      schemaValidate(JSON.parse(new TextDecoder().decode(exported))),
      true,
    );
    assert.deepEqual(parseBoard(exported), parsed);
  }
  let schemaRejected = 0,
    semanticRejected = 0;
  for (const fixture of fixtures.invalid) {
    const value = mutated(fixture);
    if (schemaValidate(value)) semanticRejected++;
    else schemaRejected++;
    assert.throws(
      () => parseBoard(encode(value)),
      { code: fixture.expected },
      fixture.name,
    );
  }
  assert(schemaRejected > 0);
  assert(semanticRejected > 0, 'semantic oracle supplements JSON Schema');
});

test('UTF-16 boundaries, blank values, all nested allowlists, sparse arrays and unsafe objects are rejected without coercion', () => {
  const document = one();
  const panel = document.board.panels[0]!;
  document.board.name = ' ' + '😀'.repeat(99) + ' ';
  panel.title = '😀'.repeat(100);
  panel.id = '😀'.repeat(50);
  panel.reference.surfaceId = '😀'.repeat(2048);
  document.sources[0]!.name = '😀'.repeat(128);
  const parsed = parseBoard(encode(document));
  assert.equal(
    parsed.input.name,
    document.board.name,
    'trimming happens only on save',
  );
  assert.equal(parsed.input.panels[0]!.title, panel.title);
  for (const change of [
    (d: Document) => {
      d.board.name = '😀'.repeat(100) + 'x';
    },
    (d: Document) => {
      d.board.panels[0]!.title = '😀'.repeat(100) + 'x';
    },
    (d: Document) => {
      d.board.panels[0]!.id = '😀'.repeat(50) + 'x';
    },
    (d: Document) => {
      d.board.panels[0]!.reference.surfaceId = '😀'.repeat(2048) + 'x';
    },
    (d: Document) => {
      d.sources[0]!.name = '😀'.repeat(128) + 'x';
    },
    (d: Document) => {
      d.sources[0]!.name = '\n\t';
    },
    (d: Document) => {
      d.board.panels[0]!.reference.surfaceId = ' ';
    },
  ]) {
    const invalid = structuredClone(document);
    change(invalid);
    assert.throws(() => parseBoard(encode(invalid)), { code: 'InvalidBoard' });
  }
  for (const target of [
    [],
    { name: 'x' },
    { name: 'x', panels: [], toJSON() {} },
    Object.assign(new Date(), { name: 'x', panels: [] }),
    { name: 'x', panels: Array(1) },
    { name: 'x', panels: [], [Symbol('extra')]: true },
  ])
    assert.throws(() => validateBoardInput(target), { code: 'InvalidBoard' });
  assert.throws(
    () =>
      parseBoard(
        encode(
          JSON.parse(
            JSON.stringify(one()).replace(
              '"format":',
              '"__proto__":{},"format":',
            ),
          ),
        ),
      ),
    { code: 'InvalidBoard' },
  );
});

test('exact geometry preserves native y+h behavior, rejects duplicate references and overlap, and caps panels/sources at 200', () => {
  const document = one();
  document.board.panels[0] = {
    ...document.board.panels[0]!,
    x: 11,
    y: 100000,
    w: 1,
    h: 1000,
  };
  assert.equal(
    parseBoard(encode(document)).input.panels[0]!.y,
    100000,
    'no invented y+h bound',
  );
  for (const fields of [
    { x: -1 },
    { y: 100001 },
    { w: 0 },
    { h: 1001 },
    { x: 11, w: 2 },
    { y: 0.5 },
    { h: Infinity },
  ]) {
    const changed = structuredClone(document);
    Object.assign(changed.board.panels[0]!, fields);
    assert.throws(() => parseBoard(encode(changed)), { code: 'InvalidBoard' });
  }
  document.board.panels = Array.from({ length: 200 }, (_, index): Panel => ({
    ...document.board.panels[0]!,
    id: String(index),
    reference: {
      ...document.board.panels[0]!.reference,
      surfaceId: String(index),
    },
    x: 0,
    y: index,
    w: 1,
    h: 1,
  }));
  assert.equal(parseBoard(encode(document)).input.panels.length, 200);
  document.board.panels.push({
    ...document.board.panels[0]!,
    id: 'extra',
    reference: { ...document.board.panels[0]!.reference, surfaceId: 'extra' },
    y: 200,
  });
  assert.throws(() => parseBoard(encode(document)), { code: 'InvalidBoard' });
});

test('source IDs must be the exact sorted referenced set; export labels are advisory and never introduce addressing or trust', () => {
  const document = one(),
    b = 'b'.repeat(64);
  document.board.panels.push({
    ...document.board.panels[0]!,
    id: 'two',
    y: 6,
    reference: { ...document.board.panels[0]!.reference, nodeId: b },
  });
  document.sources.push({ nodeId: b });
  assert.equal(parseBoard(encode(document)).sources.length, 2);
  document.sources.reverse();
  assert.throws(() => parseBoard(encode(document)), { code: 'InvalidBoard' });
  const exported = JSON.parse(
    new TextDecoder().decode(
      exportBoard({ ...document.board, id: crypto.randomUUID(), revision: 3 }, [
        { nodeId: b, name: '<b>advisory</b>' },
        { nodeId: 'c'.repeat(64), name: 'unused' },
      ]),
    ),
  );
  assert.deepEqual(exported.sources, [
    { nodeId: 'a'.repeat(64) },
    { nodeId: b, name: '<b>advisory</b>' },
  ]);
  assert.deepEqual(Object.keys(exported).sort(), [
    'board',
    'format',
    'sources',
    'version',
  ]);
  assert(!JSON.stringify(exported).includes('revision'));
  assert(!JSON.stringify(exported).includes('unused'));
  const maximum = one();
  maximum.board.panels = Array.from({ length: 200 }, (_, index) => ({
    ...maximum.board.panels[0]!,
    id: String(index),
    x: 0,
    y: index,
    w: 1,
    h: 1,
    reference: {
      ...maximum.board.panels[0]!.reference,
      nodeId: index.toString(16).padStart(64, '0'),
    },
  }));
  maximum.sources = maximum.board.panels.map((panel) => ({
    nodeId: panel.reference.nodeId,
  }));
  assert.equal(parseBoard(encode(maximum)).sources.length, 200);
  maximum.sources.push({ nodeId: 'f'.repeat(64) });
  assert.throws(() => parseBoard(encode(maximum)), { code: 'InvalidBoard' });
});

test('raw UTF-8 size and nesting are checked before parsing, and escaped delimiters stay ordinary text', () => {
  const encoded = encode(fixtures.valid[0]!.document),
    exact = new Uint8Array(BOARD_FILE_LIMIT);
  exact.fill(32);
  exact.set(encoded);
  assert.deepEqual(parseBoard(exact).input, fixtures.valid[0]!.document.board);
  const overflow = new Uint8Array(BOARD_FILE_LIMIT + 1);
  overflow.fill(32);
  overflow.set(encoded);
  assert.throws(() => parseBoard(overflow), { code: 'InvalidBoard' });
  for (const bytes of [
    Uint8Array.of(255),
    encode({ ...one(), unknown: [] }),
    new TextEncoder().encode('['.repeat(129) + '0' + ']'.repeat(129)),
  ])
    assert.throws(() => parseBoard(bytes), { code: 'InvalidBoard' });
  const document = one();
  document.board.name = '[{\\"quoted"}]';
  assert.equal(parseBoard(encode(document)).input.name, document.board.name);
});
