import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { test } from "@omochi-ui/core/testing";

for (const runtime of ["node", "bun"]) {
  test(`bounds stdio frame bytes before parsing in ${runtime}`, { timeout: 25_000 }, () => {
    const result = spawnSync(runtime, [
      "--unhandled-rejections=strict",
      fileURLToPath(new URL("./fixtures/stdio-frame-limits.mjs", import.meta.url)),
    ], { encoding: "utf8", timeout: 20_000 });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), "stdio frames bounded");
  });
}
