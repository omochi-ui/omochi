import assert from "node:assert/strict";
import { test } from "@omochi-ui/core/testing";
import { JsonlFramer, JsonlFrameTooLargeError, MAX_FRAME_BYTES } from "../src/node/jsonl-framer.js";

test("JSONL counts bytes before decoding and preserves UTF-8 split across chunks", () => {
  const lines: string[] = [];
  const framer = new JsonlFramer(line => { lines.push(line); return true; });
  for (const byte of Buffer.from("🌸あé\r\nnext\nlast")) framer.write(Uint8Array.of(byte));
  assert.deepEqual(lines, ["🌸あé\r", "next"]);
  assert.equal(framer.end(), true);
  assert.deepEqual(lines, ["🌸あé\r", "next", "last"]);
  assert.equal(framer.write(Buffer.from("after EOF\n")), false);
});

test("JSONL bounds a partial frame over many tiny writes before delivering any line", () => {
  let delivered = 0;
  const framer = new JsonlFramer(() => { delivered++; return true; });
  const byte = Uint8Array.of(0x20);
  for (let i = 0; i < MAX_FRAME_BYTES; i++) assert.equal(framer.write(byte), true);
  assert.equal(delivered, 0);
  assert.throws(() => framer.write(byte), JsonlFrameTooLargeError);
  assert.equal(delivered, 0);
  assert.equal(framer.write(Buffer.from("\nignored\n")), false);
  assert.equal(framer.end(), false);
});

test("JSONL applies the limit per line, allowing an exact-limit LF frame in a larger chunk", () => {
  const lengths: number[] = [];
  const framer = new JsonlFramer(line => { lengths.push(Buffer.byteLength(line)); return true; });
  const frame = Buffer.alloc(MAX_FRAME_BYTES, " ");
  assert.equal(framer.write(Buffer.concat([frame, Buffer.from("\n"), frame, Buffer.from("\n")])), true);
  assert.deepEqual(lengths, [MAX_FRAME_BYTES, MAX_FRAME_BYTES]);
  assert.equal(framer.end(), true);
  assert.equal(lengths.length, 2, "terminal LF does not synthesize an EOF line");
});

test("JSONL includes CR and multibyte UTF-8 in its raw byte budget", () => {
  for (const suffix of [Buffer.from("\r\n"), Buffer.from("あ\n")]) {
    let delivered = false;
    const framer = new JsonlFramer(() => { delivered = true; return true; });
    framer.write(Buffer.alloc(MAX_FRAME_BYTES, " "));
    assert.throws(() => framer.write(suffix), JsonlFrameTooLargeError);
    assert.equal(delivered, false);
  }
});

test("JSONL rejects an oversized single chunk before delivering or retaining its tail", () => {
  let delivered = false;
  const framer = new JsonlFramer(() => { delivered = true; return true; });
  assert.throws(() => framer.write(Buffer.alloc(8 * 1024 * 1024, " ")), JsonlFrameTooLargeError);
  assert.equal(delivered, false);
  assert.equal(framer.end(), false);
});

test("JSONL stops the rest of a chunk when a consumer rejects a line or closes", () => {
  for (const close of [false, true]) {
    const lines: string[] = [];
    const framer = new JsonlFramer(line => {
      lines.push(line);
      if (close) framer.close();
      return close;
    });
    framer.write(Buffer.from("part"));
    assert.equal(framer.write(Buffer.from("ial\nnever\n")), false);
    assert.deepEqual(lines, ["partial"]);
    assert.equal(framer.end(), false);
  }
});
