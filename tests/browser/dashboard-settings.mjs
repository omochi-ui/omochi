// bun tests/browser/dashboard-settings.mjs
// Start directly from a public share without first opening a registered profile.
import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile, mkdir } from 'node:fs/promises';
import { chromium } from './cdp.mjs';

const temp = await mkdtemp('/tmp/omochi-dashboard-settings-');
const screenshots = process.env.SCREENSHOT_DIR ?? '/tmp/omochi-dashboard-settings';
let browser, server;
async function until(check, label) {
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) { if (await check()) return; await Bun.sleep(25); }
  throw Error(`Timed out: ${label}`);
}
try {
  const built = await Bun.build({ entrypoints: ['packages/hub-ui/test/browser/gateway-reconnect.jsx'], outdir: temp, target: 'browser' });
  assert.equal(built.success, true, String(built.logs));
  await writeFile(temp + '/index.html', '<html><head><meta name="viewport" content="width=device-width, initial-scale=1"><link rel="stylesheet" href="/gateway-reconnect.css"></head><body><div id="root"></div><script type="module" src="/gateway-reconnect.js"></script></body></html>');
  server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
    const pathname = new URL(request.url).pathname;
    const file = Bun.file(temp + (pathname === '/' ? '/index.html' : pathname));
    return await file.exists() ? new Response(file) : new Response('Not found', { status: 404 });
  } });
  browser = await chromium(temp + '/chrome');
  const errors = [];
  browser.onEvent(event => { if (event.method === 'Runtime.exceptionThrown') errors.push(event.params.exceptionDetails); });
  await browser.command('Runtime.enable');
  await browser.command('Page.navigate', { url: `http://127.0.0.1:${server.port}/?public` });
  const evaluate = expression => browser.evaluate(expression);
  await until(() => evaluate('document.querySelector(".connection-control")?.dataset.connectionState === "Connected"'), 'public share connected');
  assert.equal(await evaluate('indexedDB.databases().then(databases => databases.length)'), 0, 'public viewing does not create a registered profile');
  await evaluate('document.querySelector(".settings-button").click()');
  await until(() => evaluate('!!document.querySelector(".settings-dialog:modal")'), 'settings opened');
  assert.equal(await evaluate('!!document.querySelector(".settings-dialog .settings-reset")'), true, 'public-share settings must include browser identity management');
  await until(() => evaluate('document.querySelector(".settings-dialog [data-browser-id]")?.textContent === "c".repeat(64)'), 'settings loads browser identity');
  assert.equal(await evaluate('gatewayTest.sessions.length'), 1, 'opening settings does not reconnect the share');
  assert.equal(await evaluate('document.querySelector(".connection-control").dataset.connectionState'), 'Connected');
  assert.equal(await evaluate('!!sessionStorage.getItem("omochi.shared.public")'), true, 'settings preserves sharing authority');
  await mkdir(screenshots, { recursive: true });
  const screenshot = await browser.command('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
  await writeFile(`${screenshots}/public-share-settings.png`, Buffer.from(screenshot.data, 'base64'));

  // A profile owned by another tab must not prevent public viewing or hide settings.
  const other = await browser.tab(`http://127.0.0.1:${server.port}/?public`);
  await other.command('Runtime.enable');
  await until(() => other.evaluate('document.querySelector(".connection-control")?.dataset.connectionState === "Connected"'), 'public share opens while another tab owns the profile');
  await other.evaluate('document.querySelector(".settings-button").click()');
  await until(() => other.evaluate('/another tab/i.test(document.querySelector(".settings-dialog [role=alert]")?.textContent ?? "")'), 'profile lock error in settings');
  assert.equal(await other.evaluate('document.querySelector(".settings-dialog .settings-reset").checkVisibility()'), true, 'identity controls stay visible when loading fails');
  assert.equal(await other.evaluate('gatewayTest.sessions.length'), 1, 'failed profile load does not replace the share session');
  assert.equal(await other.evaluate('document.querySelector(".connection-control").dataset.connectionState'), 'Connected');
  await browser.command('Page.navigate', { url: 'about:blank' });
  await until(() => other.evaluate('navigator.locks.query().then(state => state.held.length === 0)'), 'first tab releases profile');
  await other.evaluate('Array.from(document.querySelectorAll(".settings-dialog button")).find(button => button.textContent === "Load browser ID").click()');
  await until(() => other.evaluate('document.querySelector(".settings-dialog [data-browser-id]")?.textContent === "c".repeat(64)'), 'retry loads the saved browser identity');
  assert.equal(await other.evaluate('gatewayTest.sessions.length'), 1);
  assert.equal(await other.evaluate('!!document.querySelector(".settings-dialog [role=alert]")'), false);

  await other.evaluate('gatewayTest.changeIdentity({ restore: false, endpointId: "6".repeat(64), secretHex: "7".repeat(64) }); document.querySelector(".settings-reset button").click()');
  await until(() => other.evaluate('!!document.querySelector("[aria-labelledby=confirm-title]:modal")'), 'reset confirmation from public share');
  await other.evaluate('document.querySelector("[aria-labelledby=confirm-title] .danger").click()');
  await until(() => other.evaluate('!!document.querySelector(".connections-page") && document.querySelector("[data-browser-id]")?.textContent === "6".repeat(64)'), 'public-share settings can reset the browser identity');
  assert.equal(await other.evaluate('sessionStorage.getItem("omochi.shared.public")'), null);
  assert.equal(await other.evaluate('document.querySelector(".connections-page .workspace-status select").checkVisibility()'), true, 'idle language selector retains its original placement');
  assert.deepEqual(errors, []);
  console.log(`PASS: public-share settings load browser identity, preserve the connection, recover from profile contention and reset explicitly. Screenshots: ${screenshots}`);
} finally {
  await browser?.close(); server?.stop(true); await rm(temp, { recursive: true, force: true });
}
