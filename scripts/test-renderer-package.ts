import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { distributionManifest, loadRelease, root } from './release.ts';

async function run(command: string[], cwd: string): Promise<string> {
  const child = Bun.spawn(command, { cwd, stdout: 'pipe', stderr: 'pipe' });
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (code !== 0)
    throw new Error(`${command[0]} failed (${code})\n${stdout}\n${stderr}`);
  return stdout;
}

// Consume packed declarations outside this workspace with no ambient Node types.
// These local tarballs are never published and cannot resolve private build inputs.
const temporary = await mkdtemp(
  path.join(tmpdir(), 'omochi-renderer-consumer-'),
);
try {
  const release = await loadRelease();
  const versions = new Map(
    release.packages.map(({ manifest }) => [manifest.name, manifest.version]),
  );
  const dependencies: Record<string, string> = {};
  for (const name of ['@omochi-ui/core', '@omochi-ui/renderer']) {
    const workspace = release.packages.find(
      ({ manifest }) => manifest.name === name,
    )!;
    const stage = path.join(temporary, name.split('/')[1]!);
    await mkdir(stage);
    for (const file of workspace.manifest.files ?? []) {
      await cp(
        path.join(root, workspace.directory, file),
        path.join(stage, file),
        { recursive: true },
      );
    }
    await writeFile(
      path.join(stage, 'package.json'),
      JSON.stringify(distributionManifest(workspace.manifest, versions)),
    );
    await cp(path.join(root, 'LICENSE'), path.join(stage, 'LICENSE'));
    const packed = JSON.parse(
      await run(
        [
          'npm',
          'pack',
          '--ignore-scripts',
          '--json',
          '--pack-destination',
          temporary,
        ],
        stage,
      ),
    ) as { filename: string }[];
    dependencies[name] = `file:${path.join(temporary, packed[0]!.filename)}`;
  }
  const renderer = JSON.parse(
    await readFile(path.join(root, 'packages/renderer/package.json'), 'utf8'),
  );
  const consumer = path.join(temporary, 'consumer');
  await mkdir(consumer);
  await writeFile(
    path.join(consumer, 'package.json'),
    JSON.stringify({
      private: true,
      type: 'module',
      dependencies,
      devDependencies: {
        typescript: renderer.devDependencies.typescript,
        '@types/react': renderer.devDependencies['@types/react'],
        '@types/react-dom': renderer.devDependencies['@types/react-dom'],
      },
    }),
  );
  await writeFile(
    path.join(consumer, 'tsconfig.json'),
    JSON.stringify({
      compilerOptions: {
        target: 'ES2023',
        lib: ['ES2023', 'DOM'],
        module: 'ESNext',
        moduleResolution: 'Bundler',
        strict: true,
        skipLibCheck: false,
        noEmit: true,
        types: [],
        jsx: 'react-jsx',
      },
      include: ['consumer.ts'],
    }),
  );
  await writeFile(
    path.join(consumer, 'consumer.ts'),
    [
      "import { SurfaceView, useA2uiSession, useA2uiSource, createA2uiSession, createLocalHubSession, createResourceCatalog, A2uiWebSocketClient, BrowserImages } from '@omochi-ui/renderer';",
      "import type { LiveSession, RenderBudget } from '@omochi-ui/core/browser';",
      'export const publicApi = { SurfaceView, useA2uiSession, useA2uiSource, createA2uiSession, createLocalHubSession, createResourceCatalog, A2uiWebSocketClient, BrowserImages };',
      'export type BrowserContracts = { session: LiveSession; budget: RenderBudget };',
    ].join('\n'),
  );
  await run(
    ['npm', 'install', '--ignore-scripts', '--no-audit', '--no-fund'],
    consumer,
  );
  await run(
    [path.join(consumer, 'node_modules/.bin/tsc'), '-p', 'tsconfig.json'],
    consumer,
  );
  console.log(
    'Renderer tarball declarations pass in an isolated browser consumer.',
  );
} finally {
  await rm(temporary, { recursive: true, force: true });
}
