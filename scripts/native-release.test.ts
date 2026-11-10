import { expect, test } from 'bun:test';
import {
  crateNames,
  nativeVersion,
  npmNames,
  validateBundle,
  validateNativeSources,
} from './native-release.ts';
import { readFile } from 'node:fs/promises';
import { root } from './release.ts';
import path from 'node:path';

const version = '0.2.0';
function bundle(kind: 'npm' | 'crates') {
  return {
    version,
    complete: true,
    verified: true,
    dirty: false,
    packages: (kind === 'npm' ? npmNames : crateNames).map((name) => ({
      name,
      version,
      filename:
        kind === 'npm'
          ? `${name.replace('@', '').replace('/', '-')}-${version}.tgz`
          : `${name}-${version}.crate`,
    })),
  };
}
test('native publication requires every exact package with safe filenames and verified artifacts', () => {
  for (const kind of ['npm', 'crates'] as const) {
    const valid = bundle(kind);
    validateBundle(valid, version, kind);
    expect(() =>
      validateBundle({ ...valid, version: '0.1.2' }, version, kind),
    ).toThrow();
    expect(() =>
      validateBundle(
        { ...valid, packages: valid.packages.slice(1) },
        version,
        kind,
      ),
    ).toThrow();
    expect(() =>
      validateBundle(
        { ...valid, packages: valid.packages.map(() => valid.packages[0]!) },
        version,
        kind,
      ),
    ).toThrow();
    const escaped = structuredClone(valid);
    escaped.packages[0]!.filename = '../other.tgz';
    expect(() => validateBundle(escaped, version, kind)).toThrow();
  }
  expect(() =>
    validateBundle({ ...bundle('npm'), complete: false }, version, 'npm'),
  ).toThrow();
  expect(() =>
    validateBundle({ ...bundle('npm'), dirty: true }, version, 'npm'),
  ).toThrow();
  expect(() =>
    validateBundle({ ...bundle('crates'), verified: false }, version, 'crates'),
  ).toThrow();
  expect(() =>
    validateBundle({ ...bundle('crates'), localSdk: true }, version, 'crates'),
  ).toThrow();
});

test('native wrappers retain existing package names and release platforms match the build matrix', async () => {
  expect(npmNames.slice(-2)).toEqual(['@omochi-ui/cli', '@omochi-ui/utils']);
  const targets = (await Bun.file(
    path.join(root, 'scripts/native/targets.json'),
  ).json()) as { target: string; runner: string }[];
  const workflow = Bun.YAML.parse(
    await readFile(
      path.join(root, '.github/workflows/native-release.yml'),
      'utf8',
    ),
  ) as any;
  expect(workflow.jobs.build.strategy.matrix.include).toEqual(
    targets.map((t) => ({ os: t.runner, target: t.target })),
  );
  expect(await nativeVersion()).toMatch(/^\d+\.\d+\.\d+$/);
  await validateNativeSources();
});
