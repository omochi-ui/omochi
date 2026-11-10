import { expect } from 'bun:test';
import { test } from '@omochi-ui/core/testing';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';

test('typecheck entries bypass check scripts, run no tests and reject invalid source', async (context) => {
  const temporary = await mkdtemp(join(tmpdir(), 'omochi-typecheck-'));
  context.after(() => rm(temporary, { recursive: true, force: true }));
  const cases = [
    {
      directory: '..',
      script: 'release:typecheck',
      config: 'scripts/tsconfig.json',
    },
    {
      directory: '../apps/dashboard',
      script: 'typecheck',
      config: 'tsconfig.json',
    },
    {
      directory: '../apps/mobile-site',
      script: 'typecheck',
      config: 'tsconfig.json',
    },
  ];
  for (const [index, entry] of cases.entries()) {
    const manifest = await Bun.file(
      join(import.meta.dir, entry.directory, 'package.json'),
    ).json();
    const root = join(temporary, String(index));
    await mkdir(dirname(join(root, entry.config)), { recursive: true });
    await writeFile(
      join(root, 'package.json'),
      JSON.stringify({
        scripts: {
          check: 'bun -e \'throw new Error("CHECK_SCRIPT_EXECUTED")\'',
          test: 'bun -e \'throw new Error("TEST_SCRIPT_EXECUTED")\'',
          [entry.script]: manifest.scripts[entry.script],
        },
      }),
    );
    await writeFile(
      join(root, entry.config),
      JSON.stringify({
        compilerOptions: { strict: true, noEmit: true, types: [] },
        include: [
          entry.config.startsWith('scripts/') ? '../source.ts' : 'source.ts',
        ],
      }),
    );
    const run = () =>
      Bun.spawnSync([process.execPath, 'run', entry.script], {
        cwd: root,
        stdout: 'pipe',
        stderr: 'pipe',
      });
    await writeFile(
      join(root, 'source.ts'),
      'export const count: number = 1;\n',
    );
    const valid = run();
    expect(valid.stderr.toString()).not.toContain('CHECK_SCRIPT_EXECUTED');
    expect(valid.stderr.toString()).not.toContain('TEST_SCRIPT_EXECUTED');
    expect(valid.exitCode).toBe(0);
    await writeFile(
      join(root, 'source.ts'),
      'export const count: number = "invalid";\n',
    );
    const invalid = run();
    expect(invalid.exitCode).toBe(1);
    expect(invalid.stdout.toString() + invalid.stderr.toString()).toContain(
      'TS2322',
    );
  }
});
