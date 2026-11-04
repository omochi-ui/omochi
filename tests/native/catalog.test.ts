import { expect } from "bun:test";
import { test } from "@omochi-ui/core/testing";
import { copyFile, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { delimiter, join, resolve } from "node:path";

for (const fixture of ["catalog-arrays", "catalog-enums", "catalog-property-names", "catalog-python-keys", "catalog-allof", "catalog-required"]) {
  test(`native ${fixture} bindings preserve JSON types`, {
    skip: !process.env.OMOCHI_NATIVE_TEST,
  }, async (t) => {
    const root = await mkdtemp(join(tmpdir(), "omochi-catalog-"));
    t.after(() => rm(root, { recursive: true, force: true }));
    const workspace = resolve(import.meta.dir, "../..");
    const pythonBindings = fixture !== "catalog-arrays" && fixture !== "catalog-required";
    const profile = process.env.OMOCHI_NATIVE_PROFILE ?? "debug";
    if (profile !== "debug" && profile !== "release") throw new Error("invalid native profile");
    const generate = Bun.spawn([
      join(workspace, "crates/target", profile, "omochi-ui"),
      "generate", "catalog", "--catalog", fixture !== "catalog-arrays"
        ? join(workspace, `spec/fixtures/${fixture}.json`)
        : join(import.meta.dir, "fixtures/catalog-arrays.json"),
      "--typescript", join(root, "catalog.ts"),
      ...(pythonBindings ? ["--python", join(root, "catalog.py")] : []),
    ], { stdout: "pipe", stderr: "pipe" });
    const generationError = await new Response(generate.stderr).text();
    expect(await generate.exited, generationError).toBe(0);

    await copyFile(join(import.meta.dir, `fixtures/${fixture}.ts`), join(root, "usage.ts"));
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
    if (pythonBindings) {
      await copyFile(join(import.meta.dir, `fixtures/${fixture}.py`), join(root, "usage.py"));
      const python = Bun.spawn(["python3", "-B", join(root, "usage.py")], {
        env: { ...process.env, PYTHONPATH: join(workspace, "packages/sdk/python/src") },
        stdout: "pipe", stderr: "pipe",
      });
      const [pythonOut, pythonError, pythonStatus] = await Promise.all([
        new Response(python.stdout).text(), new Response(python.stderr).text(), python.exited,
      ]);
      expect(pythonStatus, pythonOut + pythonError).toBe(0);
      if (fixture === "catalog-property-names" || fixture === "catalog-python-keys") {
        const typescript = Bun.spawn([process.execPath, join(root, "usage.ts")], {
          stdout: "pipe", stderr: "pipe",
        });
        const [tsOut, tsError, tsStatus] = await Promise.all([
          new Response(typescript.stdout).text(), new Response(typescript.stderr).text(), typescript.exited,
        ]);
        expect(tsStatus, tsError).toBe(0);
        const expected = fixture === "catalog-python-keys" ? {
          class: "button", async: true, "data-label": "label", "1st": 1,
          "": "empty", __private: "private", "K": "kelvin", 'quote"line\n': "escaped",
        } : {
          sourceURL: "source", data_key: "data", type: "type", match: "match",
          self: "self", crate: "crate", super: "super", gen: "gen",
        };
        const expectedProps = [expected, fixture === "catalog-python-keys"
          ? { ...expected, None: "none", "optional key": "optional" }
          : { ...expected, optionalURL: "optional" }];
        expect(JSON.parse(tsOut)).toEqual(expectedProps);
        expect(JSON.parse(pythonOut)).toEqual(expectedProps);
      }
    }
  });
}

test("native Python empty and conditional allOf annotations evaluate", {
  skip: !process.env.OMOCHI_NATIVE_TEST,
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "omochi-catalog-allof-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const workspace = resolve(import.meta.dir, "../..");
  const profile = process.env.OMOCHI_NATIVE_PROFILE ?? "debug";
  if (profile !== "debug" && profile !== "release") throw new Error("invalid native profile");
  await writeFile(join(root, "input.json"), JSON.stringify({
    components: { Edge: { properties: {
      empty: { allOf: [] },
      conditional: { allOf: [{ if: {}, then: {} }] },
      array: { type: "array", items: { allOf: [] } },
      union: { oneOf: [{ allOf: [] }, { type: "string" }] },
    } } },
  }));
  const generate = Bun.spawn([
    join(workspace, "crates/target", profile, "omochi-ui"),
    "generate", "catalog", "--catalog", join(root, "input.json"),
    "--python", join(root, "catalog.py"),
  ], { stdout: "pipe", stderr: "pipe" });
  const generationError = await new Response(generate.stderr).text();
  expect(await generate.exited, generationError).toBe(0);
  const python = Bun.spawn(["python3", "-B", "-c", `
from typing import Any, get_type_hints
from catalog import EdgeProps
assert get_type_hints(EdgeProps) == {
    "empty": Any, "conditional": Any, "array": list[Any], "union": Any | str,
}
`], {
    env: { ...process.env, PYTHONPATH: [root, join(workspace, "packages/sdk/python/src")].join(delimiter) },
    stdout: "pipe", stderr: "pipe",
  });
  const pythonError = await new Response(python.stderr).text();
  expect(await python.exited, pythonError).toBe(0);
});
