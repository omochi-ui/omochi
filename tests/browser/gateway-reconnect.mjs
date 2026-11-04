// bun tests/browser/gateway-reconnect.mjs
// Real React components in Chromium; controlled session responses avoid network timing.
import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile, mkdir } from 'node:fs/promises';
import { chromium } from './cdp.mjs';
const temp = await mkdtemp('/tmp/omochi-gateway-reconnect-');
const screenshots = process.env.SCREENSHOT_DIR ?? '/tmp/omochi-gateway-reconnect';
let browser, server;
async function until(check, label) {
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) { if (await check()) return; await Bun.sleep(25); }
  throw Error(`Timed out: ${label}`);
}
try {
  const built = await Bun.build({ entrypoints: ['packages/hub-ui/test/browser/gateway-reconnect.jsx'], outdir: temp, target: 'browser' });
  assert.equal(built.success, true, String(built.logs));
  await writeFile(temp + '/index.html', '<html><head><link rel="stylesheet" href="/gateway-reconnect.css"></head><body><div id="root"></div><script type="module" src="/gateway-reconnect.js"></script></body></html>');
  server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) { const pathname = new URL(request.url).pathname; const file = Bun.file(temp + (pathname === '/' ? '/index.html' : pathname)); return await file.exists() ? new Response(file) : new Response('Not found', { status: 404 }); } });
  browser = await chromium(temp + '/chrome');
  const errors = [];
  browser.onEvent(event => { if (event.method === 'Runtime.exceptionThrown') errors.push(event.params.exceptionDetails); });
  await browser.command('Runtime.enable');
  await browser.command('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1100, deviceScaleFactor: 1, mobile: false });
  await browser.command('Page.navigate', { url: `http://127.0.0.1:${server.port}/` });
  const evaluate = expression => browser.evaluate(expression);
  const advance = async milliseconds => {
    // Drain React work, then advance only the component's one-second poll timers.
    await evaluate('new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))');
    for (let elapsed = 0; elapsed < milliseconds; elapsed += 1000) await evaluate('gatewayTest.tick()');
  };
  const content = () => evaluate("document.querySelector('.gateway-invitation')?.textContent ?? ''");
  const button = label => `Array.from(document.querySelectorAll('button')).find(b=>b.textContent.trim()===${JSON.stringify(label)}&&!b.disabled)`;
  const click = async label => {
    await until(() => evaluate(`!!(${button(label)})`), label);
    await evaluate(`${button(label)}.click()`);
  };
  const connect = async name => {
    const element = `Array.from(document.querySelectorAll('article.notice')).find(a=>a.textContent.includes(${JSON.stringify(name)}))`;
    await until(() => evaluate(`!!(${element}?.querySelector('button:not(:disabled)'))`), `connect ${name}`);
    await evaluate(`${element}.querySelector('button').click()`);
    await until(() => evaluate(`!!(${button('Disconnect shared dashboard')})`), 'connected');
  };
  const count = () => evaluate('gatewayTest.calls.length');
  const calls = () => evaluate('gatewayTest.calls');
  const reply = async (index, status, id) => evaluate(`gatewayTest.reply(${index},${JSON.stringify(status)},${JSON.stringify(id ?? 'job1')})`);
  const nextCall = async (index, method) => {
    if (method === 'gatewayPairStatus') await advance(1100);
    await until(async () => (await count()) > index, method);
    const call = (await calls())[index]; assert.equal(call.method, method); return call;
  };
  const invite = async () => {
    await evaluate('gatewayTest.invite()');
    await until(async () => (await content()).includes('Gateway name'), 'invitation received');
  };
  const shot = async name => {
    await mkdir(screenshots, { recursive: true });
    const image = await browser.command('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true });
    await writeFile(`${screenshots}/${name}.png`, Buffer.from(image.data, 'base64'));
  };
  await connect('Original Hub');
  await click('Add to this hub');
  await nextCall(0, 'gatewayPairStart'); await reply(0, 'pairing');
  await nextCall(1, 'gatewayPairStatus'); // Hold an old-session poll across disconnect.
  await click('Disconnect shared dashboard');
  await until(async () => (await content()).includes('Reconnect to Original Hub'), 'disconnected recovery');
  await shot('disconnected');
  await connect('Other Hub');
  await advance(2000);
  assert.equal(await count(), 2, 'no status query on another hub');
  assert.equal(await evaluate(`!!(${button('Add to this hub')})`), false);
  await reply(1, 'registered'); // A stopped poll cannot replace the retained pairing state.
  await until(async () => (await content()).includes('Registering gateway'), 'ignore old poll');
  await connect('Other owner grant');
  await advance(2000);
  assert.equal(await count(), 2, 'no status query under another owner grant');
  assert.equal(await evaluate(`!!(${button('Add to this hub')})`), false);
  await connect('Original Hub');
  const resumed = await nextCall(2, 'gatewayPairStatus');
  assert.equal(resumed.id, 'job1'); assert.notEqual(resumed.session, (await calls())[1].session);
  assert.equal(resumed.source.name, 'Original Hub');
  await advance(2000);
  assert.equal(await count(), 3, 'status requests never overlap');
  await reply(2, 'savePending');
  await until(async () => (await content()).includes('saving on the hub failed'), 'save pending');
  await click('Disconnect shared dashboard'); await connect('Other Hub');
  assert.equal(await evaluate(`!!(${button('Retry saving registration')})`), false);
  await connect('Original Hub'); await click('Retry saving registration');
  const save = await nextCall(3, 'gatewayPairRetrySave'); assert.equal(save.id, 'job1');
  await reply(3, 'registered');
  await until(async () => (await content()).includes('Gateway registered'), 'saved after reconnect');
  assert.deepEqual((await calls()).map(call => call.method), ['gatewayPairStart', 'gatewayPairStatus', 'gatewayPairStatus', 'gatewayPairRetrySave']);
  assert.ok((await calls()).every(call => call.source.name === 'Original Hub'), 'no job request is sent to another hub or grant');
  await shot('registered');

  // Starting requests retain their original target even if the parent switches.
  await click('Dismiss invitation'); await invite(); await click('Add to this hub');
  await nextCall(4, 'gatewayPairStart'); await connect('Other Hub');
  await reply(4, 'savePending', 'job2');
  await until(async () => (await content()).includes('Reconnect to Original Hub'), 'late start stays scoped');
  assert.equal(await evaluate(`!!(${button('Retry saving registration')})`), false);
  await connect('Original Hub'); await click('Retry saving registration');
  await nextCall(5, 'gatewayPairRetrySave');
  await evaluate('gatewayTest.fail(5)');
  await until(async () => (await content()).includes('Registration status unavailable'), 'save retry error visible');
  await click('Retry saving registration'); await nextCall(6, 'gatewayPairRetrySave');
  // Dismiss an in-flight save; the next invitation must not inherit its response.
  await click('Dismiss invitation'); await invite(); await reply(6, 'registered', 'job2');
  await click('Add to this hub'); await nextCall(7, 'gatewayPairStart');
  await reply(7, 'pairing', 'job3'); await nextCall(8, 'gatewayPairStatus');
  await click('Dismiss invitation'); await invite(); await reply(8, 'registered', 'job3');
  await click('Add to this hub'); await nextCall(9, 'gatewayPairStart');
  await click('Dismiss invitation'); await invite(); await reply(9, 'registered', 'job4');
  await click('Add to this hub'); await nextCall(10, 'gatewayPairStart');
  await evaluate('gatewayTest.fail(10)');
  await until(async () => (await content()).includes('Registration outcome unknown'), 'lost start response');
  assert.equal(await evaluate(`!!(${button('Add to this hub')})`), false, 'unknown start cannot redeem again');
  await shot('outcome-unknown');
  await click('Dismiss invitation'); await invite();
  assert.equal(await evaluate(`!!(${button('Add to this hub')})`), true, 'dismiss clears busy/pending state');

  // Replacing the browser identity cannot resume a job even on the same grant.
  await click('Add to this hub'); await nextCall(11, 'gatewayPairStart'); await reply(11, 'pairing', 'job5');
  await click('Disconnect shared dashboard');
  await evaluate('gatewayTest.changeIdentity()');
  await click('Reset browser identity…'); await click('Confirm identity reset');
  await until(() => evaluate(`document.querySelector('[data-browser-id]')?.textContent === '9'.repeat(64)`), 'new browser identity');
  await connect('Original Hub'); await advance(2000);
  assert.equal(await count(), 12, 'same hub and grant with a different identity cannot query the old job');
  assert.ok((await content()).includes('Reconnect to Original Hub'));
  await click('Dismiss invitation'); await invite();

  // Names use the hub's UTF-8 byte limit, and invalid input must remain editable.
  const nameInput = '.gateway-invitation input';
  const enterName = async value => {
    await evaluate(`(() => { const input = document.querySelector(${JSON.stringify(nameInput)}); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, ${JSON.stringify(value)}); input.dispatchEvent(new Event('input', { bubbles: true })); })()`);
    await until(() => evaluate(`document.querySelector(${JSON.stringify(nameInput)}).value === ${JSON.stringify(value)}`), 'name input');
  };
  await enterName('あ'.repeat(86)); // 258 bytes, despite fitting maxLength=256.
  await until(() => evaluate(`document.querySelector(${JSON.stringify(nameInput)}).getAttribute('aria-invalid') === 'true'`), 'invalid UTF-8 name');
  assert.equal(await evaluate(`!!(${button('Add to this hub')})`), false, 'oversized UTF-8 name cannot submit');
  assert.ok((await content()).includes('Gateway name is too long. Use a shorter name.'));
  await shot('name-too-long-en');
  await evaluate(`(() => { const select = document.querySelector('.language-switcher select'); select.value = 'ja'; select.dispatchEvent(new Event('change', { bubbles: true })); })()`);
  await until(async () => (await content()).includes('ゲートウェイ名が長すぎます。短くしてください。'), 'Japanese name validation');
  assert.equal(await count(), 12, 'local validation sends no enrollment request');
  assert.equal(await evaluate(`document.querySelector(${JSON.stringify(nameInput)}).disabled`), false, 'invalid name can be shortened');
  await shot('name-too-long-ja');
  await evaluate(`(() => { const select = document.querySelector('.language-switcher select'); select.value = 'en'; select.dispatchEvent(new Event('change', { bubbles: true })); })()`);
  await enterName('あ'.repeat(85)); // 255 bytes.
  await click('Add to this hub');
  assert.equal((await nextCall(12, 'gatewayPairStart')).name, 'あ'.repeat(85));
  await reply(12, 'registered', 'job6');
  await until(async () => (await content()).includes('Gateway registered'), 'shortened name registers');
  await click('Dismiss invitation'); await invite();
  await enterName('あ'.repeat(85) + 'a'); // Exactly 256 bytes is also accepted.
  await click('Add to this hub');
  assert.equal((await nextCall(13, 'gatewayPairStart')).name, 'あ'.repeat(85) + 'a');
  await reply(13, 'registered', 'job7');
  await until(async () => (await content()).includes('Gateway registered'), '256-byte name registers');
  await click('Dismiss invitation'); await invite();

  // Warm owner links still reach the parent while the child holds an invitation.
  const owner = Buffer.from(JSON.stringify({ version: 1, hubId: 'b'.repeat(64), grantId: '22222222-2222-4222-8222-222222222222', relayUrls: ['https://relay.example/'], mode: 'owner', secret: '1'.repeat(64) })).toString('base64url');
  await evaluate(`location.hash=${JSON.stringify('#owner:' + owner)}`);
  await until(() => evaluate(`location.hash===''&&!!(${button('Accept invitation')})`), 'warm owner link');
  assert.ok((await content()).includes('Gateway name'), 'warm parent navigation preserves child invitation');
  await shot('warm-owner-invitation');
  assert.equal(await count(), 14, 'navigation and dismissal never automatically enroll');
  assert.deepEqual(errors, []);
  console.log(`PASS: gateway reconnect, authority isolation, save retry, late results, dismissal, UTF-8 name validation, next invitation and warm navigation. Screenshots: ${screenshots}`);
} catch (error) {
  console.error(await browser?.evaluate('document.body.innerText'));
  throw error;
} finally {
  await browser?.close(); server?.stop(true); await rm(temp, { recursive: true, force: true });
}
