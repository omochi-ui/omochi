import { expect } from 'bun:test';
import { test, type TestContext } from '@omochi-ui/core/testing';
import {
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  symlink,
  unlink,
  writeFile,
} from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';

const script = join(import.meta.dir, 'format.ts');
const ignore = await readFile(
  join(import.meta.dir, '../.formatignore'),
  'utf8',
);

async function fixture(context: TestContext) {
  const root = await mkdtemp(join(tmpdir(), 'omochi-format-'));
  context.after(() => rm(root, { recursive: true, force: true }));
  const git = (...args: string[]) => {
    const result = Bun.spawnSync(['git', ...args], {
      cwd: root,
      stdout: 'pipe',
      stderr: 'pipe',
    });
    if (result.exitCode !== 0) throw new Error(result.stderr.toString());
  };
  git('init', '-q');
  const write = async (path: string, text: string) => {
    await mkdir(dirname(join(root, path)), { recursive: true });
    await writeFile(join(root, path), text);
  };
  await write('.formatignore', ignore);
  await write(
    '.prettierrc.json',
    '{"singleQuote":true,"semi":true,"tabWidth":2}\n',
  );
  const run = (args: string[], env: Record<string, string> = {}) =>
    Bun.spawnSync([process.execPath, script, ...args], {
      cwd: root,
      env: { ...process.env, ...env },
      stdout: 'pipe',
      stderr: 'pipe',
    });
  const bin = join(root, 'tools');
  await mkdir(bin);
  await symlink(Bun.which('git')!, join(bin, 'git'));
  const tool = async (name: string, body: string) => {
    await write(`tools/${name}`, `#!${process.execPath}\n${body}\n`);
    await chmod(join(bin, name), 0o755);
  };
  return { root, git, write, run, bin, tool };
}

test('web checks do not write; formatting covers new files and preserves excluded files', async (context) => {
  const f = await fixture(context);
  const source = 'const x={a:1,b:"two"}\n';
  await f.write('src/a space;$.ts', source);
  await f.write('src/deleted.ts', source);
  await f.write('src/.gitignore', 'local/\n');
  await f.write('vendor/third-party.ts', source);
  await f.write('build/tracked.ts', source);
  await f.write('crates/cli/tests/fixtures/catalog/expected.ts', source);
  f.git('add', '.');
  await unlink(join(f.root, 'src/deleted.ts'));
  await f.write('new.ts', source);
  await f.write('src/local/ignored.ts', source);
  await f.write('target/linked.ts', source);
  await symlink('target/linked.ts', join(f.root, 'link.ts'));

  const check = f.run(['--check', '--only', 'web']);
  expect(check.exitCode).toBe(1);
  expect(await readFile(join(f.root, 'src/a space;$.ts'), 'utf8')).toBe(source);
  expect(await readFile(join(f.root, 'new.ts'), 'utf8')).toBe(source);
  const formatted = f.run(['--only', 'web']);
  expect(formatted.stderr.toString()).toBe('');
  expect(formatted.exitCode).toBe(0);
  expect(await readFile(join(f.root, 'new.ts'), 'utf8')).toContain("b: 'two'");
  expect(await readFile(join(f.root, 'src/a space;$.ts'), 'utf8')).toContain(
    "b: 'two'",
  );
  for (const path of [
    'vendor/third-party.ts',
    'build/tracked.ts',
    'crates/cli/tests/fixtures/catalog/expected.ts',
    'src/local/ignored.ts',
    'target/linked.ts',
  ]) {
    expect(await readFile(join(f.root, path), 'utf8')).toBe(source);
  }
  expect(f.run(['--check', '--only', 'web']).exitCode).toBe(0);
});

test('missing tools are reported together before any other formatter writes', async (context) => {
  const f = await fixture(context);
  const source = 'const x={a:1}\n';
  await f.write('app.ts', source);
  await f.write('app.py', 'x=1\n');
  await f.write('task.sh', '#!/bin/sh\necho ok\n');
  const result = f.run(
    ['--only', 'web', '--only', 'python', '--only', 'shell'],
    { PATH: f.bin },
  );
  expect(result.exitCode).toBe(1);
  expect(result.stderr.toString()).toContain('uv');
  expect(result.stderr.toString()).toContain('shfmt');
  expect(await readFile(join(f.root, 'app.ts'), 'utf8')).toBe(source);
});

test('listing needs no formatters and includes shell shebangs but omits generated launchers', async (context) => {
  const f = await fixture(context);
  await f.write('run', '#!/usr/bin/env bash\necho ok\n');
  await f.write('apps/mobile/android/gradlew', '#!/bin/sh\necho generated\n');
  await f.write('apps/mobile/lib/l10n/generated/messages.dart', 'generated');
  await f.write('apps/mobile/lib/app.dart', 'source');
  await f.write(
    'apps/mobile/windows/flutter/generated_plugin_registrant.cc',
    'generated',
  );
  const result = f.run(['--list'], { PATH: f.bin });
  expect(result.exitCode).toBe(0);
  expect(result.stdout.toString()).toContain('[shell]\nrun');
  expect(result.stdout.toString()).toContain('apps/mobile/lib/app.dart');
  expect(result.stdout.toString()).not.toContain('generated');
  expect(result.stdout.toString()).not.toContain('gradlew');
  expect(f.run(['--only', 'typo']).exitCode).toBe(1);
});

test('JAR paths and source paths remain separate arguments, and failed checks do not hide later checks', async (context) => {
  const f = await fixture(context);
  await f.write('app.kt', 'kotlin');
  await f.write('App.java', 'java');
  await f.write('app.py', 'python');
  await f.write('format tools/ktfmt.jar', 'jar');
  await f.write('format tools/google-java-format.jar', 'jar');
  await f.tool(
    'java',
    `import {appendFileSync} from 'node:fs'; appendFileSync('commands.jsonl', JSON.stringify(process.argv.slice(2))+'\\n');`,
  );
  await f.tool(
    'uv',
    `import {appendFileSync} from 'node:fs'; appendFileSync('commands.jsonl', JSON.stringify(process.argv.slice(2))+'\\n'); process.exit(1);`,
  );
  const result = f.run(
    ['--check', '--only', 'python', '--only', 'kotlin', '--only', 'java'],
    {
      PATH: f.bin,
      KTFMT_JAR: join(f.root, 'format tools/ktfmt.jar'),
      GOOGLE_JAVA_FORMAT_JAR: join(
        f.root,
        'format tools/google-java-format.jar',
      ),
    },
  );
  expect(result.exitCode).toBe(1);
  const commands = (await readFile(join(f.root, 'commands.jsonl'), 'utf8'))
    .trim()
    .split('\n')
    .map((line) => JSON.parse(line));
  expect(commands).toEqual([
    ['run', '--locked', 'ruff', 'format', '--check', './app.py'],
    [
      '-jar',
      join(f.root, 'format tools/ktfmt.jar'),
      '--kotlinlang-style',
      '--dry-run',
      '--set-exit-if-changed',
      './app.kt',
    ],
    [
      '-jar',
      join(f.root, 'format tools/google-java-format.jar'),
      '--dry-run',
      '--set-exit-if-changed',
      './App.java',
    ],
  ]);
});

test('cargo formatting includes workspace macros and leaves path dependencies and golden modules untouched', async (context) => {
  if (!Bun.which('cargo')) context.skip('cargo is unavailable');
  const f = await fixture(context);
  await f.write(
    'Cargo.toml',
    '[workspace]\nmembers = ["sdk", "sdk/macros"]\nresolver = "3"\n',
  );
  await f.write(
    'sdk/Cargo.toml',
    '[package]\nname = "format-sdk"\nversion = "0.0.0"\nedition = "2024"\n[dependencies]\nformat-vendor = {path="../vendor/lib"}\n',
  );
  await f.write(
    'sdk/macros/Cargo.toml',
    '[package]\nname = "format-macros"\nversion = "0.0.0"\nedition = "2024"\n',
  );
  await f.write(
    'vendor/lib/Cargo.toml',
    '[package]\nname = "format-vendor"\nversion = "0.0.0"\nedition = "2024"\n',
  );
  const source = 'pub fn value()->u8{1}\n';
  await f.write('vendor/lib/src/lib.rs', source);
  await f.write('sdk/src/expected.rs', source);
  const sdk =
    '#[path = "expected.rs"]\n#[rustfmt::skip]\nmod expected;\npub fn sdk()->u8{1}\n';
  await f.write('sdk/src/lib.rs', sdk);
  await f.write('sdk/macros/src/lib.rs', source);
  expect(f.run(['--only', 'rust', '--check']).exitCode).toBe(1);
  expect(await readFile(join(f.root, 'sdk/src/lib.rs'), 'utf8')).toBe(sdk);
  expect(await readFile(join(f.root, 'sdk/macros/src/lib.rs'), 'utf8')).toBe(
    source,
  );
  const formatted = f.run(['--only', 'rust']);
  expect(formatted.stderr.toString()).toBe('');
  expect(formatted.exitCode).toBe(0);
  expect(
    await readFile(join(f.root, 'sdk/macros/src/lib.rs'), 'utf8'),
  ).toContain('pub fn value() -> u8 {');
  expect(await readFile(join(f.root, 'vendor/lib/src/lib.rs'), 'utf8')).toBe(
    source,
  );
  expect(await readFile(join(f.root, 'sdk/src/expected.rs'), 'utf8')).toBe(
    source,
  );
  expect(f.run(['--only', 'rust', '--check']).exitCode).toBe(0);
});
