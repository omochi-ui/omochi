import { cp, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { root, run, smokePackages } from './release.ts';

// Install only release tarballs in a directory outside the monorepo. Workspace
// symlinks must not conceal missing dependencies, entrypoints or browser assets.
const staging = await mkdtemp(path.join(tmpdir(), 'omochi-installed-'));
try {
  const artifacts = await Bun.file(path.join(root, 'artifacts/npm/manifest.json')).json() as {
    packages: { name: string; filename: string }[];
  };
  const libc = process.platform === 'linux' ? (process.report.getReport() as { header: { glibcVersionRuntime?: string } }).header.glibcVersionRuntime ? '-gnu' : '-musl' : '';
  const hostAdapter = `@omochi-ui/local-${process.platform}-${process.arch}${libc}`;
  await writeFile(path.join(staging, 'package.json'), JSON.stringify({
    name: 'omochi-release-smoke', private: true, type: 'module',
    dependencies: Object.fromEntries(smokePackages(artifacts.packages, hostAdapter).map(({ name, filename }) => [name, `file:${path.join(root, 'artifacts/npm', filename)}`])),
  }));
  run('npm', ['install', '--ignore-scripts', '--no-audit', '--no-fund', '--registry', 'https://registry.npmjs.org/'], staging);
  await mkdir(path.join(staging, 'runtime'), { mode: 0o700 });
  await cp(path.join(root, 'scripts/npm-smoke.mjs'), path.join(staging, 'smoke.mjs'));
  const previous = process.env.XDG_RUNTIME_DIR;
  const previousAdapter = process.env.OMOCHI_LOCAL_NODE;
  try {
    process.env.XDG_RUNTIME_DIR = path.join(staging, 'runtime');
    delete process.env.OMOCHI_LOCAL_NODE;
    run(process.execPath, ['smoke.mjs'], staging);
  } finally {
    if (previous === undefined) delete process.env.XDG_RUNTIME_DIR;
    else process.env.XDG_RUNTIME_DIR = previous;
    if (previousAdapter === undefined) delete process.env.OMOCHI_LOCAL_NODE;
    else process.env.OMOCHI_LOCAL_NODE = previousAdapter;
  }
  console.log('Installed npm tarballs: SDK imports, renderer CSS and hub assets passed (services ship as native archives)');
} finally {
  await rm(staging, { recursive: true, force: true });
}
