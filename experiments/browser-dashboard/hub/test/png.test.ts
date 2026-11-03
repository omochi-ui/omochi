import assert from 'node:assert/strict';
import { deflateSync } from 'node:zlib';
import { test } from '@omochi-ui/core/testing';
import { validatePng } from '../src/png.js';

// Independent bitwise oracle for fixture CRCs; production uses a lookup table.
function chunk(type: string, data: Uint8Array = new Uint8Array(0)): Uint8Array<ArrayBuffer> {
  const output = new Uint8Array(data.length + 12), view = new DataView(output.buffer);
  view.setUint32(0, data.length); output.set(new TextEncoder().encode(type), 4); output.set(data, 8);
  let crc = 0xffffffff;
  for (const byte of output.subarray(4, -4)) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  view.setUint32(output.length - 4, (crc ^ 0xffffffff) >>> 0); return output;
}
function join(...parts: Uint8Array[]): Uint8Array<ArrayBuffer> {
  const bytes = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let offset = 0; for (const part of parts) { bytes.set(part, offset); offset += part.length; } return bytes;
}
const signature = Uint8Array.of(137, 80, 78, 71, 13, 10, 26, 10);
function header(width = 1, height = 1, color = 6, depth = 8, interlace = 0, compression = 0, filter = 0) {
  const bytes = new Uint8Array(13), view = new DataView(bytes.buffer);
  view.setUint32(0, width); view.setUint32(4, height); bytes.set([depth, color, compression, filter, interlace], 8);
  return chunk('IHDR', bytes);
}
function png(options: { width?: number; height?: number; color?: number; depth?: number; interlace?: number; raw?: Uint8Array; compressed?: Uint8Array;
  before?: Uint8Array[]; dataChunks?: Uint8Array[]; after?: Uint8Array[] } = {}) {
  const data = options.compressed ?? deflateSync(options.raw ?? Uint8Array.of(0, 255, 0, 0, 255));
  return join(signature, header(options.width, options.height, options.color, options.depth, options.interlace), ...(options.before ?? []),
    ...(options.dataChunks ?? [chunk('IDAT', data)]), ...(options.after ?? []), chunk('IEND'));
}
const signal = () => new AbortController().signal;

test('PNG validates RGB/RGBA, split IDAT, all row filters and bounded ancillary data without decoding', async () => {
  assert.deepEqual(await validatePng(png(), signal()), { width: 1, height: 1 });
  assert.deepEqual(await validatePng(png({ color: 2, raw: Uint8Array.of(0, 255, 0, 0) }), signal()), { width: 1, height: 1 });
  const compressed = deflateSync(Uint8Array.of(0, 255, 0, 0, 255));
  await validatePng(png({ before: [chunk('tEXt', Uint8Array.of(0))], dataChunks: [chunk('IDAT', compressed.subarray(0, 3)), chunk('IDAT', compressed.subarray(3))] }), signal());
  const raw = new Uint8Array(5 * 5); for (let row = 0; row < 5; row++) raw[row * 5] = row;
  await validatePng(png({ height: 5, raw }), signal());
});

test('PNG rejects malformed signatures, chunk lengths/CRCs/order, duplicate headers, trailing data and unknown critical chunks', async () => {
  const valid = png(); const badCrc = valid.slice(); badCrc[badCrc.length - 1]! ^= 1;
  const badLength = valid.slice(); new DataView(badLength.buffer).setUint32(33, 0x80000000);
  const malformed = [new Uint8Array(32), join(Uint8Array.of(0), valid.subarray(1)), badCrc, badLength, valid.subarray(0, -1), join(valid, Uint8Array.of(0)),
    join(signature, chunk('IDAT'), header(), chunk('IEND')), png({ before: [header()] }), png({ before: [chunk('ABCD')] }),
    png({ before: [chunk('tesT')] }), png({ before: [chunk('PLTE', Uint8Array.of(1))] }),
    png({ dataChunks: [] }), png({ after: [chunk('IEND', Uint8Array.of(1))] }),
    png({ dataChunks: [chunk('IDAT'), chunk('tEXt'), chunk('IDAT')] }),
    join(signature, header(0), chunk('IDAT'), chunk('IEND')), join(signature, header(1, 1, 6, 3), chunk('IDAT'), chunk('IEND')),
    join(signature, header(1, 1, 6, 8, 0, 1), chunk('IDAT'), chunk('IEND')),
  ];
  for (const bytes of malformed) await assert.rejects(validatePng(bytes, signal()), { code: 'InvalidImage' });
});

test('PNG restricts valid variants and APNG, and enforces encoded/chunk/side/pixel capacity', async () => {
  for (const options of [{ depth: 16 }, { color: 0 }, { color: 3, depth: 4 }, { interlace: 1 },
    ...['acTL', 'fcTL', 'fdAT'].map(type => ({ before: [chunk(type)] })),
  ]) await assert.rejects(validatePng(png(options), signal()), { code: 'UnsupportedMedia' });
  for (const bytes of [new Uint8Array(8 * 1024 * 1024 + 1), png({ width: 8193 }), png({ height: 8193 }), png({ width: 2049, height: 2048 }),
    png({ before: Array.from({ length: 4094 }, () => chunk('tEXt')) }),
  ]) await assert.rejects(validatePng(bytes, signal()), { code: 'CapacityExceeded' });
  await validatePng(png({ before: Array.from({ length: 4093 }, () => chunk('tEXt')) }), signal());
  // Largest accepted pixel count, compressed scanlines are counted rather than retained.
  await validatePng(png({ width: 2048, height: 2048, raw: new Uint8Array(2048 * (1 + 2048 * 4)) }), signal());
});

test('inflation requires exact scanline length and valid filter bytes, and rejects corrupt zlib and compression bombs', async () => {
  const compressed = Uint8Array.from(deflateSync(Uint8Array.of(0, 255, 0, 0, 255))); compressed[compressed.length - 1]! ^= 1;
  for (const options of [{ raw: new Uint8Array(4) }, { raw: new Uint8Array(6) }, { raw: new Uint8Array(1024 * 1024) },
    { raw: Uint8Array.of(5, 255, 0, 0, 255) }, { compressed }, { compressed: Uint8Array.of(1, 2, 3) },
  ]) await assert.rejects(validatePng(png(options), signal()), { code: 'InvalidImage' });
});

test('PNG cancellation and captured-scope revalidation run before and after inflation awaits', async () => {
  const cancel = new AbortController(); cancel.abort();
  await assert.rejects(validatePng(png(), cancel.signal), { code: 'Cancelled' });
  const running = new AbortController(); const pending = validatePng(png(), running.signal); running.abort();
  await assert.rejects(pending, { code: 'Cancelled' });
  let checks = 0;
  await assert.rejects(validatePng(png(), signal(), () => { if (++checks > 5) throw Object.assign(new Error(), { code: 'ScopeExpired' }); }), { code: 'ScopeExpired' });
});
