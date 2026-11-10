// Build hosted assets first, then: bun tests/browser/gateway-bootstrap.mjs
// Delay the real transport module to exercise invitation capture before React mounts.
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { chromium } from './cdp.mjs';
const temp = await mkdtemp('/tmp/omochi-gateway-bootstrap-');
const assets = process.cwd() + '/packages/hub-ui/dist-hosted';
let browser,
  release = () => {},
  waiting = false,
  gate = Promise.resolve();
const server = Bun.serve({
  hostname: '127.0.0.1',
  port: 0,
  async fetch(request) {
    const pathname = new URL(request.url).pathname;
    if (pathname === '/iroh/iroh.js') {
      waiting = true;
      await gate;
    }
    const file = Bun.file(
      assets + (pathname === '/' ? '/index.html' : pathname),
    );
    return new Response(file, { headers: { 'Cache-Control': 'no-store' } });
  },
});
async function until(check, label) {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    if (await check()) return;
    await Bun.sleep(25);
  }
  throw Error(`Timed out: ${label}`);
}
const invitation = (id) =>
  '#g:' +
  Buffer.from(
    JSON.stringify({
      version: 4,
      address: { id: id.repeat(64), addrs: [] },
      secret: 'f'.repeat(64),
      expires: Math.floor(Date.now() / 1000) + 180,
    }),
  ).toString('base64url');
try {
  browser = await chromium(temp + '/chrome');
  for (const [index, initial] of [
    '',
    invitation('a'),
    '#owner:obsolete',
  ].entries()) {
    waiting = false;
    gate = new Promise((resolve) => {
      release = resolve;
    });
    await browser.command('Page.navigate', {
      url: `http://127.0.0.1:${server.port}/?scenario=${index}${initial}`,
    });
    await until(() => waiting, 'transport bootstrap paused');
    assert.equal(
      await browser.evaluate('location.hash === ""'),
      true,
      'initial bearer is scrubbed before transport loads',
    );
    await browser.evaluate(`location.hash=${JSON.stringify(invitation('b'))}`);
    await until(
      () => browser.evaluate('location.hash === ""'),
      'navigation scrubbed while transport is paused',
    );
    release();
    await until(
      () =>
        browser.evaluate(
          '!!document.querySelector("[data-browser-id]") && !!document.querySelector(".gateway-invitation")',
        ),
      'queued gateway invitation displayed',
    );
    assert.equal(
      await browser.evaluate(
        `document.querySelector('.gateway-invitation code').textContent === '${'b'.repeat(64)}'`,
      ),
      true,
      'latest startup invitation wins',
    );
    assert.equal(
      await browser.evaluate('!!document.querySelector("[data-error-code]")'),
      false,
      'superseded owner fragment does not produce a bootstrap error',
    );
    // Dismiss, then send another invitation after mounting. A second hash listener
    // would either lose this delivery after scrubbing or show a duplicate/busy error.
    await browser.evaluate(
      "[...document.querySelectorAll('.gateway-invitation button')].at(-1).click()",
    );
    await until(
      () => browser.evaluate('!document.querySelector(".gateway-invitation")'),
      'initial invitation dismissed',
    );
    await browser.evaluate(`location.hash=${JSON.stringify(invitation('c'))}`);
    await until(
      () =>
        browser.evaluate(
          'location.hash === "" && !!document.querySelector(".gateway-invitation")',
        ),
      'live invitation routed and scrubbed',
    );
    assert.equal(
      await browser.evaluate(
        `document.querySelector('.gateway-invitation code').textContent === '${'c'.repeat(64)}'`,
      ),
      true,
      'live gateway invitation reaches the gateway component',
    );
    assert.equal(
      await browser.evaluate(
        "[...document.querySelectorAll('.gateway-invitation [role=status]')].every(element => !element.textContent)",
      ),
      true,
      'live invitation is delivered once',
    );
  }
  console.log(
    'PASS: delayed bootstrap scrubs and routes current gateway invitations; latest startup navigation and live delivery work.',
  );
} finally {
  release();
  await browser?.close();
  server.stop(true);
  await rm(temp, { recursive: true, force: true });
}
