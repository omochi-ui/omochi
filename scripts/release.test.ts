import { expect, test } from 'bun:test';
import { distributionManifest, existingPyPiFiles, npmStageId, publicationOrder, publicSdkVersions, sdkWorkspaces, validateContents, validateVersion, type Manifest } from './release.ts';

test('SDK version selection excludes independent private browser and native build inputs', () => {
  const manifests: Manifest[] = [
    { name: '@omochi-ui/core', version: '0.1.2' },
    { name: '@omochi-ui/hub-transport', version: '0.0.0', private: true },
    { name: '@omochi-ui/dashboard-app', version: '0.0.0', private: true },
    { name: '@omochi-ui/browser-hub', version: '0.0.0', private: true },
    { name: '@omochi-ui/browser-transport', version: '0.0.0', private: true },
    { name: '@omochi-ui/browser-dashboard-experiment', version: '0.0.0', private: true },
    { name: '@omochi-ui/cli', version: '0.2.0', omochiNative: { crate: 'cli', binary: 'omochi-ui' } },
  ];
  const workspaces = manifests.map(manifest => ({ directory: manifest.name, manifest }));
  expect(publicSdkVersions(workspaces)).toEqual({ '@omochi-ui/core': '0.1.2' });
  expect(sdkWorkspaces(workspaces).map(workspace => workspace.manifest.name)).toEqual(['@omochi-ui/core']);
  expect(() => validateVersion('0.1.2', publicSdkVersions([
    ...workspaces, { directory: 'renderer', manifest: { name: '@omochi-ui/renderer', version: '0.2.0' } },
  ]))).toThrow('@omochi-ui/renderer has version');
  for (const field of ['dependencies', 'optionalDependencies', 'peerDependencies'] as const) {
    const publicPackage = { directory: 'renderer', manifest: { name: 'renderer', version: '0.1.2', [field]: { '@omochi-ui/browser-hub': 'workspace:*' } } };
    expect(() => publicationOrder([...workspaces, publicPackage])).toThrow('private');
  }
});

test('release versions must agree across SDKs and match a stable tag', () => {
  validateVersion('0.1.0', { npm: '0.1.0', rust: '0.1.0' }, 'v0.1.0');
  expect(() => validateVersion('0.1.0', { rust: '0.2.0' })).toThrow('rust has version');
  expect(() => validateVersion('0.1.0', {}, 'v0.2.0')).toThrow('Expected tag');
  for (const version of ['0.1.0-beta.1', '01.1.0', 'v0.1.0', 'main']) {
    expect(() => validateVersion(version, {})).toThrow('Invalid stable');
  }
});

test('public packages are ordered by dependencies and cannot depend on private workspaces', () => {
  const core = { directory: 'core', manifest: { name: '@omochi-ui/core', version: '0.1.0' } };
  const cli = { directory: 'cli', manifest: { name: '@omochi-ui/cli', version: '0.1.0', dependencies: { '@omochi-ui/core': 'workspace:*' } } };
  expect(publicationOrder([cli, core]).map(({ directory }) => directory)).toEqual(['core', 'cli']);
  expect(() => publicationOrder([cli, { ...core, manifest: { ...core.manifest, private: true } }])).toThrow('private');
  expect(() => publicationOrder([cli])).toThrow('Unknown workspace');
  expect(() => publicationOrder([cli, { ...core, manifest: { ...core.manifest, dependencies: { '@omochi-ui/cli': 'workspace:*' } } }])).toThrow('cycle');
});

test('distribution manifests resolve runtime workspace dependencies without changing source manifests', () => {
  const source: Manifest = { name: '@omochi-ui/cli', version: '0.1.0', dependencies: { '@omochi-ui/core': 'workspace:*', example: '^1.0.0' }, devDependencies: { typescript: '*' }, scripts: { build: 'tsc' } };
  const distributed = distributionManifest(source, new Map([['@omochi-ui/core', '0.1.0']]));
  expect(distributed.dependencies).toEqual({ '@omochi-ui/core': '0.1.0', example: '^1.0.0' });
  expect(distributed.devDependencies).toBeUndefined();
  expect(distributed.scripts).toBeUndefined();
  expect(source.dependencies!['@omochi-ui/core']).toBe('workspace:*');
  expect(() => distributionManifest(source, new Map())).toThrow('Cannot publish');
});

test('package contents must include exported assets, binaries, types and license', () => {
  const manifest: Manifest = { name: '@omochi-ui/hub-ui', version: '0.1.0', exports: { './assets': './dist/index.html', './contracts': { types: './dist/contracts.d.ts', default: './dist/contracts.js' } } };
  const files = new Set(['package.json', 'LICENSE', 'README.md', 'dist/index.html', 'dist/contracts.d.ts', 'dist/contracts.js']);
  validateContents(manifest, files);
  files.delete('dist/index.html');
  expect(() => validateContents(manifest, files)).toThrow('dist/index.html');
});

test('PyPI retries skip identical files and fail before uploading conflicting distributions', () => {
  const wheel = { filename: 'omochi_ui-0.1.0-py3-none-any.whl', digests: { sha256: 'wheel' } };
  const sdist = { filename: 'omochi_ui-0.1.0.tar.gz', digests: { sha256: 'sdist' } };
  expect(existingPyPiFiles([wheel, sdist], [])).toEqual([]);
  expect(existingPyPiFiles([wheel, sdist], [wheel])).toEqual([wheel.filename]);
  expect(existingPyPiFiles([wheel, sdist], [wheel, sdist])).toEqual([wheel.filename, sdist.filename]);
  expect(() => existingPyPiFiles([wheel, sdist], [wheel, { ...sdist, digests: { sha256: 'other-source' } }])).toThrow('different contents');
});

test('approval summaries identify the exact staged tarball and reject missing or mismatched stage receipts', () => {
  const artifact = { name: '@omochi-ui/sdk', version: '0.1.0', integrity: 'sha512-verified-tarball' };
  const stageId = 'ee383402-ad45-47f4-a9d9-88bb2b250113';
  const receipt = { ...artifact, stageId };
  const encode = (value: object) => JSON.stringify({ [artifact.name]: value });
  expect(npmStageId(encode(receipt), artifact)).toBe(stageId);
  for (const changes of [{ stageId: undefined }, { stageId: 'invalid-id' }, { version: '0.2.0' }, { name: '@omochi-ui/cli' }, { integrity: 'sha512-other-tarball' }]) {
    expect(() => npmStageId(encode({ ...receipt, ...changes }), artifact)).toThrow('Unexpected npm staging result');
  }
  expect(() => npmStageId('{}', artifact)).toThrow('Unexpected npm staging result');
});
