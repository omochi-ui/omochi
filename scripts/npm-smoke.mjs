// Copied outside the checkout; SDK and frontend imports resolve only to npm tarballs.
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { App } from '@omochi-ui/sdk';
import * as nodeSdk from '@omochi-ui/sdk/node';
assert.equal(typeof App, 'function');
assert.equal(typeof nodeSdk.WebSocketServerTransport, 'function');
assert.equal(typeof nodeSdk.LocalServerTransport, 'function');
const require = createRequire(import.meta.url);
const libc =
  process.platform === 'linux'
    ? process.report.getReport().header.glibcVersionRuntime
      ? '-gnu'
      : '-musl'
    : '';
const hostAdapter = `@omochi-ui/local-${process.platform}-${process.arch}${libc}`;
assert.equal(process.env.OMOCHI_LOCAL_NODE, undefined);
assert.ok(
  require
    .resolve(hostAdapter)
    .startsWith(
      path.join(
        path.dirname(fileURLToPath(import.meta.url)),
        'node_modules',
        hostAdapter,
      ),
    ),
);
const runtimeDir = await mkdtemp(
  path.join(
    process.platform === 'win32' ? tmpdir() : '/tmp',
    'omochi-sdk-pack-',
  ),
);
const transport = new nodeSdk.LocalServerTransport({
  serviceId: 'con',
  runtimeDir,
});
try {
  await transport.ready;
  const manifest = JSON.parse(
    await readFile(path.join(transport.directory, 'manifest.json'), 'utf8'),
  );
  assert.equal(manifest.version, 2);
  assert.equal(manifest.serviceId, 'con');
  assert.equal(
    transport.endpoint.kind,
    process.platform === 'win32' ? 'namedPipe' : 'unix',
  );
} finally {
  await transport.close();
  await rm(runtimeDir, { recursive: true, force: true });
}
assert.ok(
  (
    await readFile(
      fileURLToPath(import.meta.resolve('@omochi-ui/renderer/styles.css')),
      'utf8',
    )
  ).length,
);
const html = await readFile(
  fileURLToPath(import.meta.resolve('@omochi-ui/hub-ui/assets')),
  'utf8',
);
const assets = [...html.matchAll(/(?:src|href)="(\/assets\/[^"]+)"/g)].map(
  (match) => match[1],
);
assert.ok(assets.length >= 2);
const index = new URL(import.meta.resolve('@omochi-ui/hub-ui/assets'));
for (const asset of assets)
  assert.ok((await readFile(new URL(asset.slice(1), index))).length);
