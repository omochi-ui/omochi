import assert from 'node:assert/strict';
import { unstable_dev } from 'wrangler';

// Exercise the built Worker and its assets without Cloudflare credentials.
const dev = await unstable_dev(undefined, {
  config: 'dist/omochi_mobile/wrangler.json',
  local: true,
  ip: '127.0.0.1',
  port: 0,
  inspectorPort: 0,
  logLevel: 'error',
  experimental: { disableExperimentalWarning: true, showInteractiveDevSession: false, watch: false },
});

try {
  const origin = `http://${dev.address}:${dev.port}`;
  const response = await fetch(origin);
  assert.equal(response.status, 200);
  const html = await response.text();
  assert.ok(html.includes('<h1>Omochi Mobile</h1>'));
  assert.ok(html.includes('Connect to Omochi Dashboard from your smartphone'));
  assert.ok(html.includes('href="https://github.com/omochi-ui/omochi"'));
  assert.ok(!html.includes('/@vite/client'));
  const stylesheet = html.match(/href="([^"]+\.css)"/);
  assert.ok(stylesheet, 'built stylesheet is linked');
  for (const path of ['/mark.svg', stylesheet[1]]) {
    assert.equal((await fetch(`${origin}${path}`)).status, 200, path);
  }
  for (const path of ['/open', '/.well-known/apple-app-site-association', '/.well-known/assetlinks.json', '/missing']) {
    assert.equal((await fetch(`${origin}${path}`)).status, 404, path);
  }
  console.log('Mobile landing page, built assets and 404 routes verified.');
} finally {
  await dev.stop();
}
