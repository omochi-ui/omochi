import { cp, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { root, run } from './release.ts';

// Install only release tarballs in a directory outside the monorepo. Workspace
// symlinks must not conceal missing dependencies, entrypoints or browser assets.
const staging = await mkdtemp(path.join(tmpdir(), 'omochi-installed-'));
try {
  const artifacts = await Bun.file(path.join(root, 'artifacts/npm/manifest.json')).json() as {
    packages: { name: string; filename: string }[];
  };
  await writeFile(path.join(staging, 'package.json'), JSON.stringify({
    name: 'omochi-release-smoke', private: true, type: 'module',
    dependencies: Object.fromEntries(artifacts.packages.map(({ name, filename }) => [name, `file:${path.join(root, 'artifacts/npm', filename)}`])),
  }));
  run('npm', ['install', '--ignore-scripts', '--no-audit', '--no-fund', '--registry', 'https://registry.npmjs.org/'], staging);
  await mkdir(path.join(staging, 'runtime'), { mode: 0o700 });
  await cp(path.join(root, 'scripts/npm-smoke.mjs'), path.join(staging, 'smoke.mjs'));
  const previous = process.env.XDG_RUNTIME_DIR;
  try {
    process.env.XDG_RUNTIME_DIR = path.join(staging, 'runtime');
    run(process.execPath, ['smoke.mjs'], staging);
  } finally {
    if (previous === undefined) delete process.env.XDG_RUNTIME_DIR;
    else process.env.XDG_RUNTIME_DIR = previous;
  }
  console.log('Installed npm tarballs: SDK imports, renderer CSS and hub assets passed (services ship as native archives)');
} finally {
  await rm(staging, { recursive: true, force: true });
}
