import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { root, run } from './release.ts';

const directory = path.join(root, 'artifacts/native-npm');
const artifacts = (await Bun.file(
  path.join(directory, 'manifest.json'),
).json()) as {
  version: string;
  packages: { name: string; filename: string }[];
};
const host = `@omochi-ui/native-${process.platform}-${process.arch}`;
const selected = artifacts.packages.filter((p) =>
  [host, '@omochi-ui/cli', '@omochi-ui/utils'].includes(p.name),
);
if (selected.length !== 3) throw new Error('Missing host npm artifacts');
const staging = await mkdtemp(path.join(tmpdir(), 'omochi-native-installed-'));
// macOS TMPDIR can already consume most of sockaddr_un.sun_path. Keep sockets
// in a separate short, private directory; npm installation still uses TMPDIR.
const runtime = await mkdtemp('/tmp/omochi-npm-');
try {
  await writeFile(
    path.join(staging, 'package.json'),
    JSON.stringify({
      private: true,
      dependencies: Object.fromEntries(
        selected.map((p) => [
          p.name,
          `file:${path.join(directory, p.filename)}`,
        ]),
      ),
    }),
  );
  run(
    'npm',
    [
      'install',
      '--ignore-scripts',
      '--include=optional',
      '--no-audit',
      '--no-fund',
    ],
    staging,
  );
  for (const binary of ['omochi-ui', 'omochi-utils']) {
    const actual = run(
      path.join(staging, 'node_modules/.bin', binary),
      ['--version'],
      staging,
    );
    if (actual !== `${binary} ${artifacts.version}`)
      throw new Error(`Wrong native version: ${actual}`);
  }
  // Starting a real hub checks assets and signal forwarding across the npm launcher.
  const node = run('node', ['-p', 'process.execPath']);
  const child = Bun.spawn(
    [
      node,
      path.join(staging, 'node_modules/@omochi-ui/cli/bin/omochi-ui.cjs'),
      '--config-dir',
      path.join(staging, 'config'),
      '--runtime-dir',
      path.join(runtime, 'omochi'),
      'hub',
      '--port',
      '0',
    ],
    {
      cwd: staging,
      env: { ...process.env, PATH: '/nonexistent' },
      stdout: 'ignore',
      stderr: 'pipe',
    },
  );
  let logs = '';
  const reading = (async () => {
    for await (const chunk of child.stderr)
      logs += Buffer.from(chunk).toString();
  })();
  const deadline = Date.now() + 10_000;
  try {
    while (!/http:\/\/127\.0\.0\.1:\d+/.test(logs)) {
      if (Date.now() > deadline || child.exitCode !== null)
        throw new Error(`Native npm startup failed: ${logs}`);
      await Bun.sleep(20);
    }
    const url = logs.match(/http:\/\/127\.0\.0\.1:\d+/)![0]!;
    const response = await fetch(url);
    if (!response.ok || !(await response.text()).includes('<html'))
      throw new Error('Missing bundled dashboard');
    child.kill('SIGTERM');
    const status = await Promise.race([
      child.exited,
      Bun.sleep(5000).then(() => {
        throw new Error('Launcher did not forward shutdown');
      }),
    ]);
    if (status !== 0) throw new Error(`Hub exit status ${status}`);
  } finally {
    if (child.exitCode === null) child.kill('SIGKILL');
    await child.exited;
    await reading;
  }
  console.log(
    'Native npm tarballs: isolated install, both commands, bundled dashboard and SIGTERM passed',
  );
} finally {
  await rm(runtime, { recursive: true, force: true });
  await rm(staging, { recursive: true, force: true });
}
