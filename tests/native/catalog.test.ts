import { expect } from "bun:test";
import { test } from "@omochi-ui/core/testing";
import { copyFile, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

test("native catalog arrays preserve union element types in TypeScript", {
  skip: !process.env.OMOCHI_NATIVE_TEST,
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "omochi-catalog-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const workspace = resolve(import.meta.dir, "../..");
  const profile = process.env.OMOCHI_NATIVE_PROFILE ?? "debug";
  if (profile !== "debug" && profile !== "release") throw new Error("invalid native profile");
  const generate = Bun.spawn([
    join(workspace, "crates/target", profile, "omochi-ui"),
    "generate", "catalog", "--catalog", join(import.meta.dir, "fixtures/catalog-arrays.json"),
    "--typescript", join(root, "catalog.ts"),
  ], { stdout: "pipe", stderr: "pipe" });
  const generationError = await new Response(generate.stderr).text();
  expect(await generate.exited, generationError).toBe(0);

  await copyFile(join(import.meta.dir, "fixtures/catalog-arrays.ts"), join(root, "usage.ts"));
  await symlink(join(workspace, "node_modules"), join(root, "node_modules"));
  await writeFile(join(root, "tsconfig.json"), JSON.stringify({
    compilerOptions: {
      target: "ES2023", module: "Preserve", moduleResolution: "Bundler",
      strict: true, noEmit: true, skipLibCheck: true, types: [],
    },
    files: ["catalog.ts", "usage.ts"],
  }));
  const compile = Bun.spawn([
    process.execPath, join(workspace, "node_modules/typescript/bin/tsc"),
    "--project", join(root, "tsconfig.json"), "--pretty", "false",
  ], { stdout: "pipe", stderr: "pipe" });
  const [stdout, stderr, status] = await Promise.all([
    new Response(compile.stdout).text(), new Response(compile.stderr).text(), compile.exited,
  ]);
  expect(status, stdout + stderr).toBe(0);
});
