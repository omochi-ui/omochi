import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { appendFile, cp, readFile, rm } from 'node:fs/promises';
import path from 'node:path';
import { npmStageId, root, run } from './release.ts';

const registry = 'https://registry.npmjs.org/';
export const crateNames = ['omochi-protocol', 'omochi-transport', 'omochi-local-discovery', 'omochi-gateway', 'omochi-hub', 'omochi-cli', 'omochi-utils'];
type Artifact = { name: string; version: string; filename: string; integrity?: string; sha256?: string };
type Bundle = { version: string; packages: Artifact[]; complete?: boolean; verified?: boolean; commit?: string; dirty?: boolean };
const targets = await Bun.file(path.join(root, 'scripts/native/targets.json')).json() as { os: string; cpu: string }[];
export const npmNames = [...targets.map(t => `@omochi-ui/native-${t.os}-${t.cpu}`), '@omochi-ui/cli', '@omochi-ui/utils'];

export async function nativeVersion(): Promise<string> {
  const toml = Bun.TOML.parse(await Bun.file(path.join(root, 'crates/Cargo.toml')).text()) as { workspace: { package: { version: string } } };
  const version = toml.workspace.package.version;
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error('Native release needs a stable version');
  return version;
}

export async function validateNativeSources(): Promise<void> {
  const version = await nativeVersion();
  const sdk = Bun.TOML.parse(await Bun.file(path.join(root, 'packages/sdk/rust/Cargo.toml')).text()) as { package: { version: string } };
  const workspace = Bun.TOML.parse(await Bun.file(path.join(root, 'crates/Cargo.toml')).text()) as { workspace: { members: string[] } };
  const names: string[] = [];
  const license = await readFile(path.join(root, 'LICENSE'));
  for (const member of workspace.workspace.members) {
    const directory = path.join(root, 'crates', member);
    const manifest = Bun.TOML.parse(await Bun.file(path.join(directory, 'Cargo.toml')).text()) as Record<string, any>;
    if (manifest.package.version?.workspace !== true) throw new Error(`${member} must inherit the native version`);
    if (manifest.package.publish === false) continue;
    names.push(manifest.package.name);
    if (!license.equals(await readFile(path.join(directory, 'LICENSE')))) throw new Error(`${member} LICENSE differs from the root`);
    for (const field of ['dependencies', 'dev-dependencies', 'build-dependencies']) {
      for (const [name, dependency] of Object.entries(manifest[field] ?? {}) as [string, any][]) {
        if (dependency.path && dependency.version !== `=${name === 'omochi-ui' ? sdk.package.version : version}`) {
          throw new Error(`${member}: path dependency ${name} needs the exact publication version`);
        }
      }
    }
  }
  if (names.length !== crateNames.length || crateNames.some(name => !names.includes(name))) throw new Error('Native crate publication graph differs from the workspace');
  for (const name of ['cli', 'utils']) {
    const stub = await Bun.file(path.join(root, 'packages/npm', name, 'package.json')).json();
    if (!stub.private || stub.version !== '0.0.0' || stub.name !== `@omochi-ui/${name}`
        || stub.omochiNative?.crate !== `omochi-${name}` || !crateNames.includes(stub.omochiNative.crate)
        || stub.dependencies || stub.devDependencies || stub.scripts || stub.exports) {
      throw new Error(`${name}: npm source must remain a private launcher stub; versions and implementation belong to Cargo`);
    }
  }

}

export function validateBundle(bundle: Bundle, version: string, kind: 'npm' | 'crates'): void {
  const expected = kind === 'npm' ? npmNames : crateNames;
  if (bundle.version !== version || bundle.packages.length !== expected.length
      || new Set(bundle.packages.map(p => p.name)).size !== expected.length
      || (kind === 'npm' && (!bundle.complete || bundle.dirty))
      || (kind === 'crates' && (!bundle.verified || bundle.dirty))) throw new Error('Incomplete, dirty or unverified native bundle');
  for (const artifact of bundle.packages) {
    const filename = kind === 'npm' ? `${artifact.name.replace('@', '').replace('/', '-')}-${version}.tgz` : `${artifact.name}-${version}.crate`;
    if (!expected.includes(artifact.name) || artifact.version !== version || artifact.filename !== filename) throw new Error('Unexpected native artifact');
  }
}

async function artifacts(kind: 'npm' | 'crates'): Promise<{ directory: string; bundle: Bundle }> {
  const directory = path.join(root, 'artifacts', kind === 'npm' ? 'native-npm' : 'crates');
  const bundle = await Bun.file(path.join(directory, 'manifest.json')).json() as Bundle;
  validateBundle(bundle, await nativeVersion(), kind);
  if (bundle.commit !== run('git', ['rev-parse', 'HEAD'])) throw new Error('Native artifacts belong to another commit');
  for (const artifact of bundle.packages) {
    const bytes = await readFile(path.join(directory, artifact.filename));
    if (kind === 'npm') {
      if (`sha512-${createHash('sha512').update(bytes).digest('base64')}` !== artifact.integrity) throw new Error(`Corrupt ${artifact.filename}`);
    } else if (createHash('sha256').update(bytes).digest('hex') !== artifact.sha256) throw new Error(`Corrupt ${artifact.filename}`);
  }
  return { directory, bundle };
}

async function missing(kind: 'npm' | 'crates', bundle: Bundle): Promise<Artifact[]> {
  const pending: Artifact[] = [];
  // Validate every existing version before uploading anything, including on retries.
  for (const artifact of bundle.packages) {
    const url = kind === 'npm' ? `${registry}${encodeURIComponent(artifact.name)}/${artifact.version}` : `https://crates.io/api/v1/crates/${artifact.name}/${artifact.version}`;
    const response = await fetch(url, { signal: AbortSignal.timeout(30_000), headers: { 'User-Agent': 'omochi-release (https://github.com/omochi-ui/omochi)' } });
    if (response.status === 404) pending.push(artifact);
    else if (response.ok) {
      const data = await response.json() as { dist?: { integrity: string }; version?: { checksum: string } };
      if ((kind === 'npm' ? data.dist?.integrity !== artifact.integrity : data.version?.checksum !== artifact.sha256)) throw new Error(`${artifact.name}@${artifact.version} already exists with different contents`);
      console.log(`Already published ${artifact.name}@${artifact.version}`);
    } else throw new Error(`Registry lookup failed: ${url}: ${response.status}`);
  }
  return pending;
}

async function stageNpm(): Promise<void> {
  const { directory, bundle } = await artifacts('npm');
  const pending = await missing('npm', bundle);
  // Platform packages precede wrappers in the approval receipt.
  pending.sort((a, b) => npmNames.indexOf(a.name) - npmNames.indexOf(b.name));
  for (const artifact of pending) {
    const result = run('npm', ['stage', 'publish', path.join(directory, artifact.filename), '--access', 'public', '--tag', 'latest', '--provenance', '--ignore-scripts', '--json', '--registry', registry]);
    const stage = npmStageId(result, { ...artifact, integrity: artifact.integrity! });
    const message = `${artifact.name}@${artifact.version}: ${stage} — approve with maintainer 2FA; approve platform packages before wrappers.`;
    console.log(message);
    if (process.env.GITHUB_STEP_SUMMARY) await appendFile(process.env.GITHUB_STEP_SUMMARY, `- ${message}\n`);
  }
}

async function crates(publish: boolean): Promise<void> {
  if (publish && run('git', ['status', '--porcelain'])) throw new Error('Commit source changes before publishing native crates');
  const { bundle } = await artifacts('crates');
  const pending = await missing('crates', bundle);
  if (!publish) {
    console.log(`${pending.length} native crates ready to publish`);
    return;
  }
  if (!pending.length) return;
  const assets = path.join(root, 'crates/hub/assets');
  if (await Bun.file(path.join(assets, 'index.html')).exists()) throw new Error('Stale hub/assets directory');
  try {
    await cp(path.join(root, 'packages/hub-ui/dist'), assets, { recursive: true });
    if (run('git', ['status', '--porcelain'])) throw new Error('Source tree changed before publishing');
    // Explicitly included generated assets are dirty to Cargo even when gitignored.
    const args = ['publish', '--manifest-path', 'crates/Cargo.toml', '--workspace', '--locked', '--allow-dirty'];
    for (const name of crateNames) if (!pending.some(p => p.name === name)) args.push('--exclude', name);
    const result = spawnSync('cargo', args, { cwd: root, stdio: 'inherit', timeout: 1_800_000 });
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error('Native cargo publication failed; inspect published versions before retrying');
  } finally { await rm(assets, { recursive: true, force: true }); }
}

if (import.meta.main) {
  const [command, tag] = process.argv.slice(2);
  if (command === 'check') {
    await validateNativeSources();
    const version = await nativeVersion();
    if (tag !== undefined) {
      if (tag !== `native-v${version}`) throw new Error(`Expected native-v${version}`);
      const commit = run('git', ['rev-parse', '--verify', `refs/tags/${tag}^{commit}`]);
      if (commit !== run('git', ['rev-parse', 'HEAD'])) throw new Error('Checkout does not match native tag');
      run('git', ['merge-base', '--is-ancestor', commit, 'origin/main']);
    }
    console.log(`Native ${version}: npm cli/utils and ${crateNames.length} Cargo crates`);
  } else if (command === 'stage-npm') await stageNpm();
  else if (command === 'check-crates') await crates(false);
  else if (command === 'publish-crates') await crates(true);
  else throw new Error('Usage: native-release.ts check [native-vX.Y.Z] | stage-npm | check-crates | publish-crates');
}
