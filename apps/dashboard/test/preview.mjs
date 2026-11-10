import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { unstable_dev } from 'wrangler';

const contract = JSON.parse(
  await readFile(new URL('../headers.json', import.meta.url), 'utf8'),
);
// Runs the real local Worker and asset binding, without Cloudflare credentials.
const dev = await unstable_dev('src/index.ts', {
  config: 'wrangler.jsonc',
  local: true,
  ip: '127.0.0.1',
  port: 0,
  inspectorPort: 0,
  logLevel: 'error',
  experimental: {
    disableExperimentalWarning: true,
    showInteractiveDevSession: false,
    watch: false,
  },
});
let checks = 0;
try {
  const origin = `http://${dev.address}:${dev.port}`;
  const assets = new URL(
    '../../../packages/hub-ui/dist-hosted/',
    import.meta.url,
  );
  const files = await readdir(assets, { recursive: true });
  for (const path of [
    '/',
    '/index.html',
    ...files.filter((p) => /\.(js|css|wasm)$/.test(p)).map((p) => `/${p}`),
  ]) {
    const response = await fetch(`${origin}${path}`);
    assert.equal(response.status, 200, path);
    for (const [name, value] of Object.entries(contract.headers))
      assert.equal(response.headers.get(name), value, `${path}: ${name}`);
    const ext = path === '/' ? '.html' : path.slice(path.lastIndexOf('.'));
    assert.equal(
      response.headers.get('Content-Type'),
      contract.mime[ext],
      path,
    );
    assert.equal(
      response.headers.get('Cache-Control'),
      path.startsWith('/assets/') ? contract.cache.assets : 'no-cache',
      path,
    );
    const source = await readFile(
      new URL(path === '/' ? 'index.html' : path.slice(1), assets),
    );
    assert.deepEqual(Buffer.from(await response.arrayBuffer()), source, path);
    const head = await fetch(`${origin}${path}`, { method: 'HEAD' });
    assert.equal(head.status, 200, path);
    assert.equal((await head.arrayBuffer()).byteLength, 0, path);
    assert.equal(head.headers.get('ETag'), response.headers.get('ETag'), path);
    if (response.headers.has('ETag')) {
      const cached = await fetch(`${origin}${path}`, {
        headers: { 'If-None-Match': response.headers.get('ETag') },
      });
      assert.equal(cached.status, 304, path);
    }
    checks++;
  }
  for (const path of [
    '/api/dashboards',
    '/ws',
    '/images/id',
    '/resources/id',
    '/missing.js',
    '/other/',
    '/assets/missing.js',
    '/iroh/missing.wasm',
  ]) {
    const response = await fetch(`${origin}${path}`, {
      headers: { 'Sec-Fetch-Mode': 'navigate' },
    });
    assert.equal(response.status, 404, path);
    checks++;
  }
  assert.equal((await fetch(origin, { method: 'POST' })).status, 404);
  checks++;
  console.log(
    `Wrangler preview passed (${checks} routes; bytes, headers, HEAD, conditional requests and 404s).`,
  );
} finally {
  await dev.stop();
}
