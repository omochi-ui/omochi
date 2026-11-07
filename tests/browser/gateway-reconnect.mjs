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
    const element = `Array.from(document.querySelectorAll('.connection-record')).find(a=>a.textContent.includes(${JSON.stringify(name)}))`;
    await until(() => evaluate(`!!(${element}?.querySelector('.connection-select:not(:disabled)'))`), `connect ${name}`);
    await evaluate(`${element}.querySelector('button').click()`);
    await until(() => evaluate(`${element}?.classList.contains('selected') && !!document.querySelector('.connection-switch')`), 'connected');
  };
  const disconnect = async () => {
    await evaluate(`(() => {const invitation=document.querySelector('.gateway-invitation:modal');const disclosure=invitation?.querySelector('details:has(.shared-connections)');if(disclosure)disclosure.open=true;})()`);
    await click('Disconnect');
    await until(() => evaluate('!!document.querySelector(".connections-page")'), 'disconnected');
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
  assert.equal(await evaluate('document.querySelector(".gateway-invitation").matches(":modal")'), true, 'invitation uses a real modal');
  assert.equal(await evaluate('!!document.querySelector("#native-invitation-link")'), false, 'React takes over the bootstrap app handoff');
  assert.equal(await evaluate('document.body.innerText.includes("e".repeat(64))'), false, 'IDs stay in collapsed details');
  await click('Add to this hub');
  await nextCall(0, 'gatewayPairStart'); await reply(0, 'pairing');
  await nextCall(1, 'gatewayPairStatus'); // Hold an old-session poll across disconnect.
  await disconnect();
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
  await disconnect(); await connect('Other Hub');
  assert.equal(await evaluate(`!!(${button('Retry saving registration')})`), false);
  await connect('Original Hub'); await click('Retry saving registration');
  const save = await nextCall(3, 'gatewayPairRetrySave'); assert.equal(save.id, 'job1');
  await reply(3, 'registered');
  await until(async () => (await content()).includes('Gateway registered'), 'saved after reconnect');
  assert.deepEqual((await calls()).map(call => call.method), ['gatewayPairStart', 'gatewayPairStatus', 'gatewayPairStatus', 'gatewayPairRetrySave']);
  assert.ok((await calls()).every(call => call.source.name === 'Original Hub'), 'no job request is sent to another hub or grant');
  assert.equal(await evaluate('!!document.querySelector(".gateway-invitation a")'), false, 'a redeemed invitation no longer offers native handoff');
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
  await disconnect();
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
  await click('Dismiss invitation');

  // Incoming link failures stay visible with connection management closed.
  const invalidLink = async mode => {
    await evaluate(`location.hash = '#${mode}:invalid'`);
    await until(() => evaluate('location.hash === "" && !!document.querySelector("[data-error-code=InvalidInvitation]")'), 'invalid link feedback');
    assert.equal(await evaluate('!!document.querySelector(".connection-popover:modal")'), false);
    assert.equal(await evaluate('document.querySelector("[data-error-code=InvalidInvitation]").checkVisibility()'), true);
    assert.equal(await evaluate('document.querySelector("[data-error-code=InvalidInvitation]").getAttribute("role")'), 'alert');
    assert.equal(await evaluate('document.querySelector(".connection-switch").dataset.connectionState'), 'Connected');
  };
  await invalidLink('owner');
  await shot('invalid-owner-link');
  const publicPayload = { version: 2, hubId: 'a'.repeat(64), grantId: '44444444-4444-4444-8444-444444444444', relayUrls: ['https://relay.example/'], mode: 'share', secret: '2'.repeat(64) };
  const publicFragment = '#share:' + Buffer.from(JSON.stringify(publicPayload)).toString('base64url');
  await evaluate(`location.hash = ${JSON.stringify(publicFragment)}`);
  await until(() => evaluate('document.querySelector("h1")?.textContent === "Public dashboard"'), 'public dashboard');
  await invalidLink('share');
  const shareSessions = await evaluate('gatewayTest.sessions.length');
  const shareHandoff = await evaluate('document.querySelector(".workspace-status .connection-open-app").href');
  assert.equal(shareHandoff, 'omochi://share/' + publicFragment.slice(7), 'active dashboard has a share handoff');

  // An owner invitation has its own handoff without replacing the active share.
  const owner = Buffer.from(JSON.stringify({ version: 1, hubId: 'b'.repeat(64), grantId: '22222222-2222-4222-8222-222222222222', relayUrls: ['https://relay.example/'], mode: 'owner', secret: '1'.repeat(64) })).toString('base64url');
  await evaluate(`location.hash = ${JSON.stringify('#owner:' + owner)}`);
  await until(() => evaluate('!!document.querySelector(".owner-invitation:modal")'), 'owner invitation over share');
  assert.equal(await evaluate('document.querySelector(".workspace-status .connection-open-app").href'), shareHandoff);
  await until(() => evaluate('!!document.querySelector(".owner-invitation a")'), 'invitation profile ready');
  assert.equal(await evaluate('document.querySelector(".owner-invitation a").href'), 'omochi://owner/' + owner);
  await click('Not now');
  await until(() => evaluate('!document.querySelector(".owner-invitation")'), 'owner invitation dismissed');
  assert.equal(await evaluate('document.querySelector(".workspace-status .connection-open-app").href'), shareHandoff);
  assert.deepEqual(JSON.parse(await evaluate('sessionStorage.getItem("omochi.shared.public")')), publicPayload);
  await evaluate(`location.hash = ${JSON.stringify(publicFragment)}`);
  await until(() => evaluate('location.hash === ""'), 'duplicate share consumed');
  assert.equal(await evaluate('gatewayTest.sessions.length'), shareSessions, 'dismissal and duplicate links retain the share session');


  // Cancelling a dirty switch preserves the share; accepting it removes its bearer.
  await click('Edit dashboard');
  await evaluate(`(() => { const input = document.querySelector('.dashboard-name'); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, 'Unsaved shared work'); input.dispatchEvent(new Event('input', { bubbles: true })); })()`);
  await click('Disconnect');
  await until(() => evaluate('!!document.querySelector("#confirm-title")'), 'disconnect asks before leaving dirty work');
  await evaluate('document.querySelector("[aria-labelledby=confirm-title] .toolbar button").click()');
  await until(() => evaluate('!document.querySelector("#confirm-title")'), 'disconnect cancelled');
  assert.equal(await evaluate('document.querySelector(".dashboard-name").value'), 'Unsaved shared work');
  assert.equal(await evaluate('gatewayTest.sessions.length'), shareSessions);
  const selectSaved = async () => {
    await evaluate('Array.from(document.querySelectorAll(".sidebar .connection-record")).find(record => record.textContent.includes("Other Hub")).querySelector("button").click()');
    await until(() => evaluate('!!document.querySelector("#confirm-title")'), 'confirm saved connection switch');
  };
  await selectSaved();
  await evaluate('document.querySelector("[aria-labelledby=confirm-title] .toolbar button").click()');
  await until(() => evaluate('!document.querySelector("#confirm-title") && !!document.querySelector(".connection-record button:not(:disabled)")'), 'switch cancelled');
  assert.equal(await evaluate('gatewayTest.sessions.length'), shareSessions);
  assert.equal(await evaluate('document.querySelector(".dashboard-name").value'), 'Unsaved shared work');
  assert.equal(await evaluate('document.querySelector(".workspace-status .connection-open-app").href'), shareHandoff);
  assert.deepEqual(JSON.parse(await evaluate('sessionStorage.getItem("omochi.shared.public")')), publicPayload);
  await selectSaved(); await click('Discard changes');
  await until(() => evaluate('!!sessionStorage.getItem("omochi.shared.selected") && !document.querySelector(".dashboard-name")'), 'saved owner connected');
  assert.equal(await evaluate('sessionStorage.getItem("omochi.shared.public")'), null);
  assert.equal(await evaluate('!!document.querySelector(".workspace-status .connection-open-app")'), false, 'saved owner has no stale share handoff');
  await evaluate(`location.hash = ${JSON.stringify(publicFragment)}`);
  await until(() => evaluate('document.querySelector("h1")?.textContent === "Public dashboard"'), 'previous share can be opened again after switching away');
  await connect('Original Hub');

  // Warm owner links still reach the parent while the child holds an invitation.
  await invite();
  await evaluate(`location.hash=${JSON.stringify('#owner:' + owner)}`);
  await until(() => evaluate(`location.hash===''&&!!(${button('Accept invitation')})`), 'warm owner link');
  assert.ok((await content()).includes('Gateway name'), 'warm parent navigation preserves child invitation');
  assert.equal(await evaluate('document.querySelector(".owner-invitation").matches(":modal")'), true);
  assert.equal(await evaluate('document.querySelector(".gateway-invitation").matches(":modal")'), false, 'owner confirmation suspends the gateway modal');
  assert.equal(await evaluate('!!document.querySelector(".hub-layout")'), true, 'receiving an owner invitation keeps the current board');
  const sessionCount = await evaluate('gatewayTest.sessions.length');
  await shot('warm-owner-invitation');
  await click('Not now');
  assert.equal(await evaluate('gatewayTest.sessions.length'), sessionCount, 'dismissing does not disconnect or reconnect');
  await until(() => evaluate('document.querySelector(".gateway-invitation").matches(":modal")'), 'pending gateway resumes after owner dismissal');
  await click('Dismiss invitation');
  assert.equal(await evaluate('!!document.querySelector("#native-invitation-link")'), false, 'dismissal leaves no stale app banner');
  assert.equal(await evaluate('!!document.querySelector(".connection-popover:modal")'), false, 'connection management starts collapsed');
  assert.equal(await evaluate('!!document.querySelector(".sidebar .connection-list")'), true, 'connections remain visible');

  // A pending owner invitation must not erase or prematurely confirm a dirty editor.
  await click('Create dashboard');
  await evaluate(`(() => { const input = document.querySelector('.dashboard-name'); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, 'Unsaved work'); input.dispatchEvent(new Event('input', { bubbles: true })); })()`);
  await evaluate(`location.hash=${JSON.stringify('#owner:' + owner)}`);
  await until(() => evaluate('!!document.querySelector(".owner-invitation:modal")'), 'owner modal above dirty draft');
  assert.equal(await evaluate('document.querySelector(".dashboard-name").value'), 'Unsaved work');
  assert.equal(await evaluate('!!document.querySelector("#confirm-title")'), false, 'receiving a link does not ask to discard edits');
  await click('Accept invitation');
  await until(() => evaluate('!!document.querySelector("#confirm-title")'), 'accepting asks before leaving dirty work');
  await evaluate('document.querySelector("[aria-labelledby=confirm-title] .toolbar button").click()');
  await until(() => evaluate('!!document.querySelector(".owner-invitation button:not(:disabled)")'), 'invitation usable after cancelling discard');
  await evaluate('document.querySelector(".owner-invitation .dialog-header button").click()');
  await until(() => evaluate('!document.querySelector(".owner-invitation")'), 'top-right close dismisses owner invitation');
  assert.equal(await evaluate('document.querySelector(".dashboard-name").value'), 'Unsaved work');
  assert.equal(await evaluate('gatewayTest.sessions.length'), sessionCount);

  // Button-shaped links remain one control at narrow widths, with localized text.
  await invite();
  await browser.command('Emulation.setDeviceMetricsOverride', { width: 320, height: 900, deviceScaleFactor: 1, mobile: false });
  assert.equal(await evaluate('document.querySelector(".gateway-invitation a.hub-button").getClientRects().length'), 1);
  assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true);
  await shot('modal-320');
  await evaluate(`(() => { const select = document.querySelector('.language-switcher select'); select.value = 'ja'; select.dispatchEvent(new Event('change', { bubbles: true })); })()`);
  await until(() => evaluate('document.querySelector(".gateway-invitation a").textContent === "Omochiアプリで開く"'), 'native handoff follows UI language');
  await evaluate('document.querySelector(".gateway-invitation .dialog-header button").click()');
  await until(() => evaluate('!document.querySelector(".gateway-invitation")'), 'top-right close dismisses gateway invitation');
  await until(() => evaluate('document.activeElement !== document.body'), 'dismissal restores a focusable background target');

  // Browser settings preserve the live session and draft; reset still requires both confirmations.
  const settingsSessionCount = await evaluate('gatewayTest.sessions.length');
  await evaluate('document.querySelector(".mobile-nav").click()');
  const key = async (key, code, windowsVirtualKeyCode) => {
    await browser.command('Input.dispatchKeyEvent', { type: 'keyDown', key, code, windowsVirtualKeyCode, ...(key === 'Enter' ? { text: '\r', unmodifiedText: '\r' } : {}) });
    await browser.command('Input.dispatchKeyEvent', { type: 'keyUp', key, code, windowsVirtualKeyCode });
  };
  const openSettings = async () => {
    await evaluate('new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))');
    await evaluate('document.querySelector(".settings-button").focus()');
    assert.equal(await evaluate('document.activeElement.matches(".settings-button")'), true, 'settings trigger is focusable');
    await key('Enter', 'Enter', 13);
    await until(() => evaluate('!!document.querySelector(".settings-dialog:modal")'), 'settings opened with keyboard');
  };
  await openSettings();
  assert.equal(await evaluate('document.querySelector(".settings-dialog").contains(document.activeElement)'), true);
  assert.equal(await evaluate('!!document.querySelector(".sidebar .language-switcher, .connection-popover [data-browser-id]")'), false);
  assert.equal(await evaluate('document.querySelector(".settings-dialog [data-browser-id]").textContent'), '9'.repeat(64));
  assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true);
  await shot('settings-ja-320');
  await evaluate('document.documentElement.style.fontSize = "200%"; document.documentElement.dir = "rtl"');
  assert.equal(await evaluate('document.querySelector(".settings-dialog").scrollWidth <= document.querySelector(".settings-dialog").clientWidth'), true);
  await shot('settings-200-rtl');
  await evaluate('document.documentElement.style.fontSize = ""; document.documentElement.dir = "ltr"');
  await evaluate(`(() => { const select = document.querySelector('.settings-dialog select'); select.value = 'en'; select.dispatchEvent(new Event('change', { bubbles: true })); })()`);
  await until(() => evaluate('document.documentElement.lang === "en"'), 'settings language change');
  assert.equal(await evaluate('localStorage.getItem("omochi.dashboard.language")'), 'en');
  assert.equal(await evaluate('document.querySelector(".dashboard-name").value'), 'Unsaved work');
  assert.equal(await evaluate('gatewayTest.sessions.length'), settingsSessionCount);

  await browser.command('Browser.grantPermissions', { origin: `http://127.0.0.1:${server.port}`, permissions: ['clipboardReadWrite', 'clipboardSanitizedWrite'] });
  await click('Copy browser ID');
  await until(() => evaluate('document.querySelector(".settings-dialog [role=status]").textContent === "Browser ID copied."'), 'browser ID copied');
  assert.equal(await evaluate('navigator.clipboard.readText()'), '9'.repeat(64));
  await evaluate('navigator.clipboard.writeText = async () => { throw new Error("Clipboard denied"); }');
  await click('Copy browser ID');
  await until(() => evaluate('document.querySelector(".settings-dialog [role=status]").textContent.includes("selected")'), 'clipboard fallback');
  assert.equal(await evaluate('window.getSelection().toString()'), '9'.repeat(64));
  await key('Escape', 'Escape', 27);
  await until(() => evaluate('!document.querySelector(".settings-dialog:modal") && document.activeElement.matches(".settings-button")'), 'settings closes back to trigger');
  await openSettings(); await click('Reset browser identity…');
  await until(() => evaluate('!!document.querySelector("#confirm-title") && !document.querySelector(".settings-dialog:modal")'), 'reset confirmation replaces settings');
  await evaluate('document.querySelector("[aria-labelledby=confirm-title] .toolbar button").click()');
  await until(() => evaluate('!document.querySelector("#confirm-title") && document.activeElement.matches(".settings-button")'), 'cancel reset restores settings trigger');
  assert.equal(await evaluate('document.querySelector(".dashboard-name").value'), 'Unsaved work');
  await openSettings(); await click('Reset browser identity…'); await click('Confirm identity reset');
  await until(() => evaluate('document.querySelector("#confirm-title")?.textContent === "Discard unsaved changes?"'), 'reset asks before losing edits');
  await evaluate('document.querySelector("[aria-labelledby=confirm-title] .toolbar button").click()');
  await until(() => evaluate('!document.querySelector("#confirm-title")'), 'cancel discard');
  assert.equal(await evaluate('document.querySelector(".dashboard-name").value'), 'Unsaved work');
  assert.equal(await evaluate('document.querySelector(".settings-dialog [data-browser-id]").textContent'), '9'.repeat(64));
  assert.equal(await evaluate('gatewayTest.sessions.length'), settingsSessionCount);

  await evaluate('gatewayTest.changeIdentity({ restore: false, endpointId: "6".repeat(64), secretHex: "7".repeat(64) })');
  await openSettings(); await click('Reset browser identity…'); await click('Confirm identity reset');
  await until(() => evaluate('document.querySelector("#confirm-title")?.textContent === "Discard unsaved changes?"'), 'confirm reset of dirty draft');
  await click('Discard changes');
  await until(() => evaluate('!!document.querySelector(".connections-page") && document.querySelector("[data-browser-id]")?.textContent === "6".repeat(64)'), 'reset creates a new browser identity');
  assert.equal(await evaluate('document.querySelectorAll(".connection-record").length'), 0, 'reset clears saved connections');
  assert.equal(await evaluate('!!document.querySelector(".sidebar")'), false, 'idle screen centers connection navigation');
  assert.equal(await evaluate('!!document.querySelector(".connections-page .shared-connections")'), true);
  assert.equal(await evaluate('document.querySelector(".connections-page .workspace-status select").value'), 'en', 'idle language stays in its original location and survives identity reset');
  await shot('settings-reset-idle');

  assert.equal(await count(), 14, 'navigation and dismissal never automatically enroll');
  assert.deepEqual(errors, []);
  console.log(`PASS: gateway reconnect, authority isolation, save retry, late results, dismissal, UTF-8 name validation, visible link errors, share handoff, saved connection switching and browser settings. Screenshots: ${screenshots}`);
} catch (error) {
  console.error(await browser?.evaluate('document.body.innerText'));
  throw error;
} finally {
  await browser?.close(); server?.stop(true); await rm(temp, { recursive: true, force: true });
}
