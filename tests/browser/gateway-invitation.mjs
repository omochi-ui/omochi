import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { chromium } from './cdp.mjs';
import { certificate } from './certificate.mjs';
import { staticHost } from './static-host.mjs';
const temp = await mkdtemp('/tmp/omochi-gateway-browser-');
let browser, host, second;
async function until(fn) {
  for (let i = 0; i < 200; i++) {
    try {
      if (await fn()) return;
    } catch {}
    await Bun.sleep(50);
  }
  throw Error('browser condition timed out');
}
try {
  const tls = await certificate(temp);
  host = staticHost({ assets: process.cwd() + '/packages/hub-ui/dist-hosted', ...tls });
  browser = await chromium(temp + '/profile', [
    `--ignore-certificate-errors-spki-list=${tls.spki}`,
  ]);
  await browser.command('Page.navigate', { url: host.url });
  await until(() => browser.evaluate('!!document.querySelector("[data-browser-id]")'));
  const payload = Buffer.from(
    JSON.stringify({
      version: 4,
      address: { id: 'a'.repeat(64), addrs: [] },
      secret: 'b'.repeat(64),
      expires: Math.floor(Date.now() / 1000) + 180,
    }),
  ).toString('base64url');
  second = await browser.tab(host.url + '#g:' + payload);
  await until(() => second.evaluate('document.body.textContent.includes("Invitation sent.")'));
  assert.equal(await second.evaluate('location.hash'), '');
  await until(() => browser.evaluate(`!!document.querySelector('[aria-label="Add gateway"]')`));
  assert.equal(
    await browser.evaluate(
      'Array.from(document.querySelectorAll("button")).find(b=>b.textContent==="Add to this hub").disabled',
    ),
    true,
  );
  assert.equal(
    await browser.evaluate(`document.querySelector('[aria-label="Add gateway"] input').value`),
    'Gateway aaaaaaaa',
  );
  assert.equal(
    await browser.evaluate(`Array.from(document.querySelectorAll('a')).find(a => a.textContent === 'Open in Omochi app')?.getAttribute('href')`),
    'omochi://invite/' + payload,
  );
  const shot = await browser.command('Page.captureScreenshot', { format: 'png' });
  await writeFile('/tmp/omochi-gateway-confirmation.png', Buffer.from(shot.data, 'base64'));
  console.log(
    'PASS: real Chromium fragment removal, cross-tab acknowledged transfer, confirmation required, default gateway name',
  );
} catch (error) {
  throw error;
} finally {
  await browser?.close();
  host?.close();
  await rm(temp, { recursive: true, force: true });
}
