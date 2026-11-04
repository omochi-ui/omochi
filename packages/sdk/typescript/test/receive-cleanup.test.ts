import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { test } from "@omochi-ui/core/testing";

for (const mode of ["idle", "delayed", "rejected", "throwing", "exhausted", "stdio"]) {
  test(`receive source cleanup handles ${mode} in Node`, () => {
    const result = spawnSync("node", [
      "--unhandled-rejections=strict",
      fileURLToPath(new URL("./fixtures/receive-cleanup.mjs", import.meta.url)),
      mode,
    ], { encoding: "utf8", timeout: 10_000 });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), "closed");
  });
}
