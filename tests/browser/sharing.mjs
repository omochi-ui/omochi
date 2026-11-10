import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, rm, readdir } from 'node:fs/promises';
import { once } from 'node:events';
import os from 'node:os';
import path from 'node:path';
import { chromium } from './cdp.mjs';
import { certificate } from './certificate.mjs';
import { staticHost } from './static-host.mjs';

const productionRelay = process.env.SHARING_PRODUCTION_RELAY === '1';
const temporary = await mkdtemp(path.join(os.tmpdir(), 'omochi-share-test-'));
const output = process.env.SHARING_RESULTS
  ? path.resolve(process.env.SHARING_RESULTS)
  : path.join(temporary, 'results');
await mkdir(output, { recursive: true, mode: 0o700 });
const report = {
  startedAt: new Date().toISOString(),
  relay: productionRelay ? 'https://relay.omochi.fyi' : 'controlled-loopback',
  checks: [],
  errors: [],
};
let browser,
  host,
  child,
  recipient,
  failed = false;
let ready = null;
const messages = [];
let exceptions = 0;
async function until(predicate, label, timeout = 45000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (failed) throw new Error('Fixture terminated');
    if (await predicate()) return;
    await Bun.sleep(30);
  }
  throw new Error(`Timed out: ${label}`);
}
function passed(label, ...conditions) {
  if (conditions.length) assert.equal(conditions[0], true, label);
  report.checks.push(label);
  console.log(`Passed: ${label}`);
}
const button = (text) =>
  `Array.from(document.querySelectorAll('button')).find(b=>b.textContent.trim()===${JSON.stringify(text)}&&b.getClientRects().length)`;
async function click(page, text) {
  await until(
    async () =>
      await page.evaluate(
        `!!${button(text)}&&!${button(text)}.matches(':disabled')`,
      ),
    `enabled ${text}`,
  );
  await page.evaluate(`${button(text)}.click()`);
}
async function closeSharing(page) {
  await until(
    async () =>
      await page.evaluate(
        "!!document.querySelector('.share-dialog[open] .dialog-header button:not(:disabled)')",
      ),
    'enabled sharing close',
  );
  await page.evaluate(
    "document.querySelector('.share-dialog .dialog-header button').click()",
  );
}
async function navigate(page, url) {
  await page.command('Page.enable');
  await page.command('Runtime.enable');
  await page.command('Page.navigate', { url });
}
async function warmNavigate(page, url) {
  const marker = await page.evaluate(
    'window.__navigationMarker ??= crypto.randomUUID()',
  );
  await page.evaluate(
    `(()=>{const next=new URL(location.href);next.hash=${JSON.stringify(new URL(url).hash)};location.href=next.href})()`,
  );
  await until(
    async () => await page.evaluate('location.hash === ""'),
    'warm bearer removal',
  );
  assert.equal(
    await page.evaluate('window.__navigationMarker'),
    marker,
    'navigation retains the document',
  );
}
async function language(page, value) {
  await page.evaluate(
    `(()=>{const select=document.querySelector('.language-switcher select');select.value=${JSON.stringify(value)};select.dispatchEvent(new Event('change',{bubbles:true}));})()`,
  );
  await until(
    async () =>
      await page.evaluate(
        `document.documentElement.lang===${JSON.stringify(value)}`,
      ),
    `language ${value}`,
  );
}
async function live(page, count = 2) {
  await until(
    async () =>
      await page.evaluate(
        `document.querySelectorAll('[data-surface]').length===${count}&&document.querySelector('.connection-control')?.dataset.connectionState === 'Connected'`,
      ),
    'live shared panels',
  );
}
async function createLink(interact = false, edit = false) {
  await click(browser, 'Share dashboard');
  await until(
    async () =>
      await browser.evaluate(
        'document.querySelectorAll(".share-dialog input[type=checkbox]").length===2&&!document.querySelector(".share-dialog input[type=checkbox]").matches(":disabled")',
      ),
    'permission toggles',
  );
  await browser.evaluate(
    `(()=>{const inputs=document.querySelectorAll('.share-dialog input[type=checkbox]');if(inputs[0].checked!==${interact})inputs[0].click();if(inputs[1].checked!==${edit})inputs[1].click()})()`,
  );
  await click(browser, 'Create sharing link');
  await until(
    async () =>
      await browser.evaluate(
        'document.querySelector("#sharing-link")?.value.startsWith("https:")',
      ),
    'created sharing link',
  );
  const url = await browser.evaluate(
    'document.querySelector("#sharing-link").value',
  );
  await closeSharing(browser);
  const payload = JSON.parse(
    Buffer.from(new URL(url).hash.slice(7), 'base64url').toString(),
  );
  return { url, id: payload.grantId };
}
async function board() {
  return await (
    await fetch(`${ready.ownerUrl}api/dashboards/${ready.boardId}`)
  ).json();
}
async function update(input) {
  const response = await fetch(
    `${ready.ownerUrl}api/dashboards/${ready.boardId}`,
    {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(input),
    },
  );
  assert.equal(response.status, 200);
  return response.json();
}
try {
  const assets = path.resolve('packages/hub-ui/dist-hosted');
  assert(
    await Bun.file(path.join(assets, 'iroh/iroh_bg.wasm')).exists(),
    'Build hosted assets first',
  );
  const tls = await certificate(temporary);
  host = staticHost({ assets, ...tls, base: '/dashboard/', gzip: true });
  child = spawn(
    path.resolve('crates/target/debug/examples/dashboard_fixture'),
    [
      '--cert',
      tls.cert,
      '--key',
      tls.key,
      '--ca',
      tls.ca,
      '--web-url',
      host.url,
      ...(productionRelay ? ['--production-relay'] : []),
    ],
    { stdio: ['pipe', 'pipe', 'pipe'] },
  );
  let buffered = '';
  child.once('error', () => {
    failed = true;
  });
  child.once('exit', () => {
    failed = true;
  });
  child.stderr.on('data', () => {});
  child.stdout.on('data', (data) => {
    buffered += data;
    if (buffered.length > 128 * 1024) {
      failed = true;
      return;
    }
    let end;
    while ((end = buffered.indexOf('\n')) >= 0) {
      const line = buffered.slice(0, end);
      buffered = buffered.slice(end + 1);
      try {
        const value = JSON.parse(line);
        if (value.kind === 'ready') ready = value;
        else messages.push(value);
      } catch {
        failed = true;
      }
    }
  });
  await until(() => !!ready, 'native SDK/hub/relay fixture', 60000);
  browser = await chromium(path.join(temporary, 'chromium'), [
    `--ignore-certificate-errors-spki-list=${tls.spki}`,
  ]);
  report.browser = (await browser.command('Browser.getVersion')).product;
  assert(Number(report.browser.match(/\/(\d+)/)?.[1]) >= 156);
  browser.onEvent((event) => {
    if (event.method === 'Runtime.exceptionThrown') {
      exceptions++;
      (report.browserExceptions ??= []).push(
        event.params.exceptionDetails?.exception?.description ??
          event.params.exceptionDetails?.text,
      );
    }
  });
  browser.onEvent((event) => {
    if (
      event.method === 'Runtime.consoleAPICalled' &&
      event.params.type === 'error'
    )
      (report.consoleErrors ??= []).push(
        event.params.args.map((arg) => arg.description ?? arg.value),
      );
  });
  await navigate(browser, ready.ownerUrl);
  await live(browser);
  passed(
    'local owner uses the native HTTP/WS dashboard with two real SDK sources',
  );
  const publicLink = await createLink();
  const viewer = await browser.tab();
  await navigate(viewer, publicLink.url);
  await live(viewer);
  passed(
    'public URL opens both permitted live views without identity registration',
  );
  passed(
    'public UI hides owner controls and disables app interaction',
    await viewer.evaluate(
      `!Array.from(document.querySelectorAll('button')).some(b=>b.getClientRects().length&&['Edit dashboard','Delete dashboard','Create dashboard','Share dashboard'].includes(b.textContent.trim()))&&document.querySelector('[data-panel-id="p0"] input[type="text"]')?.matches(':disabled')`,
    ),
  );
  passed(
    'unshared same-service Surface is never rendered',
    await viewer.evaluate(
      '!document.body.textContent.includes("PRIVATE CANARY")',
    ),
  );
  await until(
    async () =>
      await viewer.evaluate(
        'Array.from(document.querySelectorAll("canvas")).filter(c=>c.width===2&&c.height===1).length===2',
      ),
    'validated PNG canvas pixels',
  );
  passed(
    'actual SDK PNG pixels pass scoped native resource delivery',
    await viewer.evaluate(
      'Array.from(document.querySelectorAll("canvas")).filter(c=>c.width===2).every(c=>{const p=c.getContext("2d").getImageData(0,0,1,1).data;return p[0]===255&&p[1]===0&&p[2]===0&&p[3]===255})',
    ),
  );
  passed(
    'bearer fragment is removed from browser history',
    await viewer.evaluate('location.hash===""'),
  );
  passed(
    'public viewing does not create an enrolled browser profile',
    await viewer.evaluate(
      `(async()=>!(await indexedDB.databases()).some(db=>db.name==='omochi.hub-client.v1'))()`,
    ),
  );
  await viewer.evaluate(
    "void (window.savedSurface=document.querySelector('.omochi-surface'))",
  );
  await language(viewer, 'ja');
  passed(
    'hosted Japanese switching preserves the live surface and readonly permissions',
    await viewer.evaluate(
      "window.savedSurface===document.querySelector('.omochi-surface')&&document.body.textContent.includes('アプリは操作できません。')&&document.querySelector('[data-panel-id=\"p0\"] input[type=\"text\"]').matches(':disabled')",
    ),
  );
  const japaneseScreenshot = await viewer.command('Page.captureScreenshot', {
    format: 'png',
    captureBeyondViewport: false,
  });
  await writeFile(
    path.join(output, 'shared-ja.png'),
    Buffer.from(japaneseScreenshot.data, 'base64'),
  );
  await viewer.command('Page.reload');
  await until(
    async () =>
      await viewer.evaluate(
        "document.documentElement.lang==='ja'&&document.querySelectorAll('[data-surface]').length===2",
      ),
    'hosted Japanese preference persists',
  );
  await language(viewer, 'en');
  await viewer.command('Page.reload');
  await live(viewer);
  passed('public link reload restores only its per-tab bearer context');
  const files = [];
  async function scan(directory, prefix = '') {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const relative = prefix + entry.name;
      if (entry.isDirectory())
        await scan(path.join(directory, entry.name), relative + '/');
      else if (/\.(?:js|css|wasm|html)$/.test(relative)) files.push(relative);
    }
  }
  await scan(assets);
  for (const file of files) {
    const audit = await viewer.evaluate(
      `(async()=>{const response=await fetch(new URL(${JSON.stringify(file)},document.baseURI),{headers:{'X-Omochi-Asset-Audit':'1'}});return {status:response.status,redirected:response.redirected,headers:Object.fromEntries(response.headers)}})()`,
    );
    assert.equal(audit.status, 200);
    assert.equal(audit.redirected, false);
    assert.equal(
      audit.headers['content-type'],
      host.contract.mime[path.extname(file)],
    );
    for (const [name, value] of Object.entries(host.contract.headers))
      assert.equal(audit.headers[name.toLowerCase()], value);
    assert.equal(
      audit.headers['cache-control'],
      file.startsWith('assets/')
        ? host.contract.cache.assets
        : file.startsWith('iroh/')
          ? host.contract.cache.iroh
          : host.contract.cache.html,
    );
    if (/\.(?:js|css|wasm)$/.test(file))
      assert.equal(audit.headers['content-encoding'], 'gzip');
  }
  passed('nested HTTPS assets have exact CSP/MIME/cache headers and gzip');
  passed(
    'static host has no API or missing-asset fallback',
    await viewer.evaluate(
      `(async()=>{for(const path of ['api/dashboards','missing.js'])if((await fetch(new URL(path,document.baseURI),{headers:{'X-Omochi-Asset-Audit':'1'}})).status!==404)return false;return true})()`,
    ),
  );

  const secondPublic = await createLink();
  await warmNavigate(viewer, secondPublic.url);
  await live(viewer);
  await until(
    async () =>
      await viewer.evaluate(
        `JSON.parse(sessionStorage.getItem('omochi.shared.public'))?.grantId===${JSON.stringify(secondPublic.id)}`,
      ),
    'new public link context',
  );
  await warmNavigate(viewer, publicLink.url);
  await live(viewer);
  await until(
    async () =>
      await viewer.evaluate(
        `JSON.parse(sessionStorage.getItem('omochi.shared.public'))?.grantId===${JSON.stringify(publicLink.id)}`,
      ),
    'original public link context',
  );
  passed(
    'same-document public navigation switches bearer context and clears both fragments',
  );

  const enabledLink = await createLink(true, true);
  recipient = await browser.tab();
  await navigate(recipient, enabledLink.url);
  await live(recipient);
  passed(
    'operation-enabled link opens without enrollment',
    await recipient.evaluate(
      `!!${button('Edit dashboard')}&&!document.querySelector('[data-panel-id="p0"] input[type="text"]').matches(':disabled')`,
    ),
  );
  const forwarded = await browser.tab();
  await navigate(forwarded, enabledLink.url);
  await live(forwarded);
  passed(
    'forwarded link grants the same permissions to another endpoint',
    await forwarded.evaluate(`!!${button('Edit dashboard')}`),
  );
  await click(recipient, 'Increment 0');
  await until(
    async () =>
      await recipient.evaluate(
        'document.body.textContent.includes("App 0 count: 1")',
      ),
    'shared action',
  );
  await click(recipient, 'Edit dashboard');
  await recipient.evaluate(
    `(()=>{const e=document.querySelector('#dashboard-name');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Edited by recipient');e.dispatchEvent(new Event('input',{bubbles:true}));})()`,
  );
  await click(recipient, 'Save dashboard');
  await until(
    async () => (await board()).name === 'Edited by recipient',
    'shared layout save',
  );
  await recipient.command('Page.reload');
  await live(recipient);
  passed(
    'operation-enabled link reload retains its permissions',
    await recipient.evaluate(`!!${button('Edit dashboard')}`),
  );
  if (!productionRelay) {
    child.stdin.write('{"op":"relay-stop"}\n');
    await until(
      () => messages.some((m) => m.kind === 'relay-stopped'),
      'relay stopped',
    );
    await until(
      async () =>
        await recipient.evaluate(
          'document.querySelectorAll("[data-surface]").length===0',
        ),
      'loss retracts stale views',
      90000,
    );
    const resumedAt = Date.now();
    child.stdin.write('{"op":"relay-start"}\n');
    await until(
      () => messages.some((m) => m.kind === 'relay-started'),
      'relay restarted',
    );
    await until(
      async () =>
        await recipient.evaluate(
          'document.querySelectorAll("[data-surface]").length===2&&document.querySelector(".connection-control")?.dataset.connectionState === "Connected"',
        ),
      'relay recovery',
      90000,
    );
    await live(viewer);
    report.recoveryMilliseconds = Date.now() - resumedAt;
    passed(
      'controlled relay reconnects fresh snapshots within 90 seconds',
      report.recoveryMilliseconds <= 90000,
    );
  } else {
    passed(
      'hosted browser shares authenticate through the deployed production relay',
    );
  }
  passed(
    productionRelay
      ? 'opening shares never replays app actions'
      : 'reconnect never replays app actions',
    await recipient.evaluate(
      'document.body.textContent.includes("App 0 count: 1")',
    ),
  );
  await click(recipient, 'Increment 0');
  await until(
    async () =>
      await recipient.evaluate(
        'document.body.textContent.includes("App 0 count: 2")',
      ),
    'post-reconnect action',
  );
  passed(
    productionRelay
      ? 'shared app actions remain usable through the production relay'
      : 'shared app actions resume after fresh snapshots',
  );
  const current = await board();
  await update({ ...current, panels: [current.panels[0]] });
  await live(viewer, 1);
  await live(recipient, 1);
  passed('owner panel removal withdraws that view from all existing shares');
  const conflict = await fetch(
    `${ready.ownerUrl}api/dashboards/${ready.boardId}`,
    {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(current),
    },
  );
  passed('native revision conflict remains enforced', conflict.status === 409);
  const revoke = await fetch(`${ready.ownerUrl}api/shares/${publicLink.id}`, {
    method: 'DELETE',
  });
  assert.equal(revoke.status, 200);
  await until(
    async () =>
      await viewer.evaluate(
        'document.querySelectorAll("[data-surface]").length===0',
      ),
    'public link revocation',
  );
  await live(recipient, 1);
  passed(
    'public link revocation retracts access while an independent link remains usable',
  );
  await recipient.command('Page.bringToFront');
  const screenshot = await recipient.command('Page.captureScreenshot', {
    format: 'png',
    captureBeyondViewport: false,
  });
  await writeFile(
    path.join(output, 'shared-editor.png'),
    Buffer.from(screenshot.data, 'base64'),
  );
  const response = await fetch(
    `${ready.ownerUrl}api/shares/${enabledLink.id}`,
    { method: 'DELETE' },
  );
  assert.equal(response.status, 200);
  await until(
    async () =>
      await recipient.evaluate(
        'document.querySelectorAll("[data-surface]").length===0',
      ),
    'link revocation',
  );
  passed('revoking one link stops every endpoint using it');
  const invalid = new URL(publicLink.url);
  const bad = JSON.parse(
    Buffer.from(invalid.hash.slice(7), 'base64url').toString(),
  );
  bad.secret = '0'.repeat(64);
  invalid.hash =
    '#share:' + Buffer.from(JSON.stringify(bad)).toString('base64url');
  const denied = await browser.tab();
  await navigate(denied, invalid.href);
  await until(
    async () =>
      await denied.evaluate(
        `!!document.querySelector('[data-error-code="NotAuthorized"]')`,
      ),
    'invalid public secret rejection',
  );
  passed(
    'an invalid public bearer never renders a board',
    await denied.evaluate(
      'document.querySelectorAll("[data-surface]").length===0',
    ),
  );
  await denied.close();
  await until(
    async () =>
      await forwarded.evaluate(
        'document.querySelectorAll("[data-surface]").length===0',
      ),
    'forwarded link revoked',
  );
  await forwarded.close();
  await recipient.close();
  const layoutLink = await createLink(false, true);
  recipient = await browser.tab();
  await navigate(recipient, layoutLink.url);
  await live(recipient, 1);
  passed(
    'layout editing without app interaction is described without claiming the dashboard is readonly',
    await recipient.evaluate(
      `document.body.textContent.includes('App controls are disabled.')&&document.body.textContent.includes('You can edit names, positions and sizes.')&&!document.body.textContent.includes('Readonly dashboard')&&!!${button('Edit dashboard')}&&document.querySelector('[data-panel-id="p0"] input[type="text"]').matches(':disabled')`,
    ),
  );
  await language(recipient, 'ja');
  passed(
    'Japanese distinguishes layout editing from app interaction',
    await recipient.evaluate(
      `document.body.textContent.includes('アプリは操作できません。')&&document.body.textContent.includes('名前・位置・サイズを編集できます。')&&!document.body.textContent.includes('閲覧専用のダッシュボード')`,
    ),
  );
  await language(recipient, 'en');
  await recipient.close();
  const registrationResponse = await fetch(`${ready.ownerUrl}api/owners`, {
    method: 'POST',
  });
  assert.equal(registrationResponse.status, 200);
  const registration = await registrationResponse.json();
  const ownerPayload = JSON.parse(
    Buffer.from(
      new URL(registration.url).hash.slice(7),
      'base64url',
    ).toString(),
  );
  recipient = await browser.tab();
  await navigate(recipient, host.url);
  await until(
    async () =>
      await recipient.evaluate('!!document.querySelector("[data-browser-id]")'),
    'idle hosted connection screen',
  );
  await warmNavigate(recipient, host.url + '#owner:invalid');
  await until(
    async () =>
      await recipient.evaluate(
        '!!document.querySelector("[data-error-code=InvalidInvitation]")',
      ),
    'malformed warm invitation rejected',
  );
  passed(
    'malformed warm owner invitation is cleared without losing the profile',
    await recipient.evaluate('!!document.querySelector("[data-browser-id]")'),
  );
  await warmNavigate(recipient, registration.url);
  await until(
    async () => await recipient.evaluate(`!!${button('Accept invitation')}`),
    'warm owner confirmation',
  );
  passed(
    'idle warm owner link clears the fragment without enrolling',
    !(await (await fetch(`${ready.ownerUrl}api/owners`)).json()).find(
      (grant) => grant.id === ownerPayload.grantId,
    ).peer,
  );
  const ownerScreenshot = await recipient.command('Page.captureScreenshot', {
    format: 'png',
    captureBeyondViewport: false,
  });
  await writeFile(
    path.join(output, 'warm-owner-invitation.png'),
    Buffer.from(ownerScreenshot.data, 'base64'),
  );
  await click(recipient, 'Accept invitation');
  await until(
    async () =>
      await recipient.evaluate(
        "document.querySelector('.connection-control')?.dataset.connectionState === 'Connected'&&Array.from(document.querySelectorAll('button')).some(b=>b.textContent.trim()==='Edit dashboard')",
      ),
    'owner connection',
  );
  passed(
    'owner enrollment opens the existing dashboard with full owner controls',
  );
  await click(recipient, 'Create dashboard');
  await recipient.evaluate(
    `(()=>{const e=document.querySelector('.dashboard-name');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Remote owner board');e.dispatchEvent(new Event('input',{bubbles:true}));})()`,
  );
  await click(recipient, 'Save dashboard');
  await until(
    async () =>
      await recipient.evaluate(
        "document.querySelector('h1')?.textContent==='Remote owner board'",
      ),
    'owner creates dashboard',
  );
  passed('remote owner creates a native hub-owned dashboard');
  await click(recipient, 'Share dashboard');
  await until(
    async () =>
      await recipient.evaluate(
        "document.querySelectorAll('.share-dialog input[type=checkbox]').length===2",
      ),
    'owner share management',
  );
  await closeSharing(recipient);
  passed('remote owner has the same share management screen');
  await warmNavigate(recipient, layoutLink.url);
  await live(recipient, 1);
  await recipient.evaluate(
    'document.querySelector(".connection-chip").click()',
  );
  await until(
    async () =>
      await recipient.evaluate(
        '!!document.querySelector(".connection-popover:modal")',
      ),
    'saved connections over public share',
  );
  await click(recipient, 'Connect');
  await until(
    async () =>
      await recipient.evaluate(
        `document.querySelector('.connection-control')?.dataset.connectionState === 'Connected'&&!!${button('Create dashboard')}`,
      ),
    'saved owner connection restored',
  );
  passed(
    'switching from a public share to a saved owner clears its bearer and native handoff',
    await recipient.evaluate(
      "sessionStorage.getItem('omochi.shared.public')===null&&!document.querySelector('.workspace-status .connection-open-app')",
    ),
  );
  await recipient.command('Page.reload');
  await until(
    async () =>
      await recipient.evaluate(
        `document.querySelector('.connection-control')?.dataset.connectionState === 'Connected'&&!!${button('Create dashboard')}`,
      ),
    'owner reload',
  );
  passed('owner identity reconnects without retaining its invitation secret');
  assert.equal(
    (
      await fetch(`${ready.ownerUrl}api/owners/${ownerPayload.grantId}`, {
        method: 'DELETE',
      })
    ).status,
    200,
  );
  await until(
    async () =>
      await recipient.evaluate(
        "document.querySelector('.connection-control')?.dataset.connectionState !== 'Connected'",
      ),
    'owner revocation',
  );
  passed('local owner revocation tears down the remote owner connection');
  await until(
    async () =>
      await recipient.evaluate(
        `!!document.querySelector('[data-error-code="NotAuthorized"]')`,
      ),
    'revoked owner authorization result after reconnect',
  );
  passed(
    'revoked owner access shows recovery instructions without claiming automatic reconnection',
    await recipient.evaluate(
      `document.querySelector('.connection-chip').textContent.includes('Disconnected')&&!document.body.textContent.includes('Reconnecting automatically')&&document.querySelector('[data-error-code="NotAuthorized"]').textContent.includes('Ask the owner to check your access')`,
    ),
  );

  passed(
    'no static host app-data/API fallback',
    !host.requests.some(
      (r) =>
        !r.audit &&
        /^\/dashboard\/(?:api|images|resources|events|services|ws)(?:\/|$)/.test(
          r.path,
        ),
    ),
  );
  passed('no unhandled browser exceptions', exceptions === 0);
  console.log(
    `Sharing browser acceptance passed (${report.checks.length} checks).`,
  );
} catch (error) {
  report.errors.push(error.message);
  console.error(error.message);
  process.exitCode = 1;
  const diagnosticPage = recipient ?? browser;
  if (diagnosticPage) {
    report.diagnostics = await diagnosticPage
      .evaluate(
        `({text:document.body.innerText,status:document.querySelector('.connection-control')?.textContent, codes:Array.from(document.querySelectorAll('[data-error-code]')).map(e=>e.dataset.errorCode), surfaces:document.querySelectorAll('[data-surface]').length, inputs:Array.from(document.querySelectorAll('input')).map(e=>({type:e.type,label:e.getAttribute('aria-label'),disabled:e.matches(':disabled')}))})`,
      )
      .catch(() => ({}));
    const screenshot = await diagnosticPage
      .command('Page.captureScreenshot', {
        format: 'png',
        captureBeyondViewport: false,
      })
      .catch(() => null);
    if (screenshot)
      await writeFile(
        path.join(output, 'failure.png'),
        Buffer.from(screenshot.data, 'base64'),
      );
  }
} finally {
  await writeFile(
    path.join(output, 'result.json'),
    JSON.stringify(report, null, 2),
  );
  await browser?.close();
  host?.close();
  if (child && child.exitCode === null && child.signalCode === null) {
    const exit = once(child, 'exit').catch(() => {});
    child.stdin.write('{"op":"stop"}\n');
    const timer = setTimeout(() => child.kill('SIGKILL'), 5000);
    await exit;
    clearTimeout(timer);
  }
  if (!process.env.SHARING_RESULTS)
    await rm(temporary, { recursive: true, force: true });
}
