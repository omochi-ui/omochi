// Copied outside the checkout; SDK and frontend imports resolve only to npm tarballs.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { App } from '@omochi-ui/sdk';
import * as nodeSdk from '@omochi-ui/sdk/node';
assert.equal(typeof App, 'function');
assert.equal(typeof nodeSdk.WebSocketServerTransport, 'function');
assert.equal(typeof nodeSdk.UnixSocketServerTransport, 'function');
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
