import { SessionFault } from '@omochi-ui/core/browser';
import { LIMITS } from './budget.js';

export interface PngInfo {
  width: number;
  height: number;
}
const crcTable = Uint32Array.from({ length: 256 }, (_, value) => {
  for (let bit = 0; bit < 8; bit++)
    value = value & 1 ? (value >>> 1) ^ 0xedb88320 : value >>> 1;
  return value >>> 0;
});
function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = crcTable[(crc ^ byte) & 255]! ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}
const invalid = (): never => {
  throw new SessionFault('InvalidImage');
};
const unsupported = (): never => {
  throw new SessionFault('UnsupportedMedia');
};
function check(signal: AbortSignal, current: () => void): void {
  current();
  if (signal.aborted) throw new SessionFault('Cancelled');
}

/** No decoded image or inflated scanline is retained. IDAT views share the reserved encoded buffer. */
export async function validatePng(
  bytes: Uint8Array<ArrayBuffer>,
  signal: AbortSignal,
  current: () => void = () => {},
): Promise<PngInfo> {
  check(signal, current);
  if (bytes.length > LIMITS.png) throw new SessionFault('CapacityExceeded');
  if (bytes.length < 33) return invalid();
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint32(0) !== 0x89504e47 || view.getUint32(4) !== 0x0d0a1a0a)
    return invalid();
  let width = 0,
    height = 0,
    channels = 0,
    chunks = 0,
    offset = 8;
  let seenHeader = false,
    seenData = false,
    endedData = false,
    seenPalette = false,
    seenEnd = false;
  let supportedVariant = true;
  const data: Uint8Array<ArrayBuffer>[] = [];
  while (offset < bytes.length) {
    check(signal, current);
    if (++chunks > 4096) throw new SessionFault('CapacityExceeded');
    if (bytes.length - offset < 12) return invalid();
    const length = view.getUint32(offset),
      end = offset + length + 12;
    if (length > 0x7fffffff || end > bytes.length) return invalid();
    const type = String.fromCharCode(...bytes.subarray(offset + 4, offset + 8));
    if (
      !/^[A-Za-z]{2}[A-Z][A-Za-z]$/.test(type) ||
      crc32(bytes.subarray(offset + 4, end - 4)) !== view.getUint32(end - 4)
    )
      return invalid();
    if (!seenHeader && type !== 'IHDR') return invalid();
    if (type === 'IHDR') {
      if (seenHeader || length !== 13) return invalid();
      seenHeader = true;
      width = view.getUint32(offset + 8);
      height = view.getUint32(offset + 12);
      const depth = bytes[offset + 16]!,
        color = bytes[offset + 17]!;
      const legalDepths: Record<number, readonly number[]> = {
        0: [1, 2, 4, 8, 16],
        2: [8, 16],
        3: [1, 2, 4, 8],
        4: [8, 16],
        6: [8, 16],
      };
      if (
        !width ||
        !height ||
        width > 0x7fffffff ||
        height > 0x7fffffff ||
        !legalDepths[color]?.includes(depth) ||
        bytes[offset + 18] !== 0 ||
        bytes[offset + 19] !== 0 ||
        bytes[offset + 20]! > 1
      )
        return invalid();
      if (
        width > LIMITS.imageSide ||
        height > LIMITS.imageSide ||
        width * height > LIMITS.imagePixels
      )
        throw new SessionFault('CapacityExceeded');
      if (
        depth !== 8 ||
        (color !== 2 && color !== 6) ||
        bytes[offset + 20] !== 0
      )
        supportedVariant = false;
      channels = color === 2 ? 3 : 4;
    } else if (type === 'PLTE') {
      if (seenPalette || seenData || length === 0 || length > 768 || length % 3)
        return invalid();
      seenPalette = true;
    } else if (type === 'IDAT') {
      if (endedData) return invalid();
      seenData = true;
      data.push(bytes.subarray(offset + 8, end - 4));
    } else if (type === 'IEND') {
      if (!seenData || length !== 0 || end !== bytes.length) return invalid();
      seenEnd = true;
    } else {
      if (type === 'acTL' || type === 'fcTL' || type === 'fdAT')
        supportedVariant = false;
      if (type.charCodeAt(0) < 97) return invalid();
      if (seenData) endedData = true;
    }
    offset = end;
  }
  if (!seenEnd) return invalid();
  if (!supportedVariant) return unsupported();
  check(signal, current);
  let index = 0;
  const input = new ReadableStream<BufferSource>({
    pull(controller) {
      if (index < data.length) controller.enqueue(data[index++]!);
      else controller.close();
    },
  });
  const reader = input
    .pipeThrough(new DecompressionStream('deflate'))
    .getReader();
  const abort = () => {
    void reader.cancel().catch(() => {});
  };
  signal.addEventListener('abort', abort, { once: true });
  let actual = 0;
  const rowBytes = 1 + width * channels,
    expected = height * rowBytes;
  try {
    check(signal, current);
    while (true) {
      const next = await reader.read();
      check(signal, current);
      if (next.done) break;
      if (next.value.byteLength > expected - actual) return invalid();
      let filter = actual % rowBytes === 0 ? 0 : rowBytes - (actual % rowBytes);
      for (; filter < next.value.length; filter += rowBytes)
        if (next.value[filter]! > 4) return invalid();
      actual += next.value.byteLength;
    }
    if (actual !== expected) return invalid();
    return { width, height };
  } catch (error) {
    check(signal, current);
    if (error instanceof SessionFault) throw error;
    return invalid();
  } finally {
    signal.removeEventListener('abort', abort);
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}
