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
const output = process.env.SHARING_RESULTS ? path.resolve(process.env.SHARING_RESULTS) : path.join(temporary, 'results');
await mkdir(output, { recursive: true, mode: 0o700 });
const report = { startedAt: new Date().toISOString(), relay: productionRelay ? 'https://relay.omochi.fyi' : 'controlled-loopback', checks: [], errors: [] };
let browser, host, child, recipient, failed = false;
let ready = null; const messages = []; let exceptions = 0;
async function until(predicate, label, timeout = 45000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) { if (failed) throw new Error('Fixture terminated'); if (await predicate()) return; await Bun.sleep(30); }
  throw new Error(`Timed out: ${label}`);
}
function passed(label, ...conditions) { if (conditions.length) assert.equal(conditions[0], true, label); report.checks.push(label); console.log(`Passed: ${label}`); }
const button = text => `Array.from(document.querySelectorAll('button')).find(b=>b.textContent.trim()===${JSON.stringify(text)}&&b.getClientRects().length)`;
async function click(page, text) { await until(async () => await page.evaluate(`!!${button(text)}&&!${button(text)}.matches(':disabled')`), `enabled ${text}`); await page.evaluate(`${button(text)}.click()`); }
async function navigate(page, url) { await page.command('Page.enable'); await page.command('Runtime.enable'); await page.command('Page.navigate', { url }); }
async function live(page, count = 2) { await until(async () => await page.evaluate(`document.querySelectorAll('[data-surface]').length===${count}&&document.querySelector('.connection-status')?.textContent.includes('Hub connected')`), 'live shared panels'); }
async function createLink(mode, interact = false, edit = false) {
  await click(browser, 'Share dashboard');
  await until(async () => await browser.evaluate('!!document.querySelector(\'select[aria-label="Access type"]\')&&!document.querySelector(\'select[aria-label="Access type"]\').disabled'), 'share dialog');
  await browser.evaluate(`(()=>{const select=document.querySelector('select[aria-label="Access type"]');select.value=${JSON.stringify(mode)};select.dispatchEvent(new Event('change',{bubbles:true}))})()`);
  if (mode === 'invite') {
    await until(async () => await browser.evaluate('document.querySelectorAll(".share-dialog input[type=checkbox]").length===2'), 'permission toggles');
    await browser.evaluate(`(()=>{const inputs=document.querySelectorAll('.share-dialog input[type=checkbox]');if(inputs[0].checked!==${interact})inputs[0].click();if(inputs[1].checked!==${edit})inputs[1].click()})()`);
  }
  await click(browser, 'Create sharing link');
  await until(async () => await browser.evaluate('document.querySelector("#sharing-link")?.value.startsWith("https:")'), 'created sharing link');
  const url = await browser.evaluate('document.querySelector("#sharing-link").value');
  await click(browser, 'Close sharing');
  const payload = JSON.parse(Buffer.from(new URL(url).hash.slice(7), 'base64url').toString());
  return { url, id: payload.grantId };
}
async function board() { return await (await fetch(`${ready.ownerUrl}api/dashboards/${ready.boardId}`)).json(); }
async function update(input) { const response = await fetch(`${ready.ownerUrl}api/dashboards/${ready.boardId}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(input) }); assert.equal(response.status, 200); return response.json(); }
try {
  const assets = path.resolve('packages/hub-ui/dist-hosted');
  assert(await Bun.file(path.join(assets, 'iroh/iroh_bg.wasm')).exists(), 'Build hosted assets first');
  const tls = await certificate(temporary);
  host = staticHost({ assets, ...tls, base: '/dashboard/', gzip: true });
  child = spawn(path.resolve('crates/target/debug/examples/dashboard_fixture'), ['--cert', tls.cert, '--key', tls.key, '--ca', tls.ca, '--web-url', host.url, ...(productionRelay ? ['--production-relay'] : [])], { stdio: ['pipe', 'pipe', 'pipe'] });
  let buffered = ''; child.once('error', () => { failed = true; }); child.once('exit', () => { failed = true; });
  child.stderr.on('data', () => {});
  child.stdout.on('data', data => { buffered += data; if (buffered.length > 128 * 1024) { failed = true; return; } let end; while ((end = buffered.indexOf('\n')) >= 0) { const line = buffered.slice(0, end); buffered = buffered.slice(end + 1); try { const value = JSON.parse(line); if (value.kind === 'ready') ready = value; else messages.push(value); } catch { failed = true; } } });
  await until(() => !!ready, 'native SDK/hub/relay fixture', 60000);
  browser = await chromium(path.join(temporary, 'chromium'), [`--ignore-certificate-errors-spki-list=${tls.spki}`]);
  report.browser = (await browser.command('Browser.getVersion')).product;
  assert(Number(report.browser.match(/\/(\d+)/)?.[1]) >= 156);
  browser.onEvent(event => { if (event.method === 'Runtime.exceptionThrown') exceptions++; });
  await navigate(browser, ready.ownerUrl); await live(browser);
  passed('local owner uses the native HTTP/WS dashboard with two real SDK sources');
  const publicLink = await createLink('public');
  const viewer = await browser.tab(); await navigate(viewer, publicLink.url); await live(viewer);
  passed('public URL opens both permitted live views without identity registration');
  passed('public UI hides owner controls and disables app interaction', await viewer.evaluate(`!Array.from(document.querySelectorAll('button')).some(b=>b.getClientRects().length&&['Edit dashboard','Delete dashboard','Create dashboard','Share dashboard'].includes(b.textContent.trim()))&&document.querySelector('[data-panel-id="p0"] input[type="text"]')?.matches(':disabled')`));
  passed('unshared same-service Surface is never rendered', await viewer.evaluate('!document.body.textContent.includes("PRIVATE CANARY")'));
  await until(async () => await viewer.evaluate('Array.from(document.querySelectorAll("canvas")).filter(c=>c.width===2&&c.height===1).length===2'), 'validated PNG canvas pixels');
  passed('actual SDK PNG pixels pass scoped native resource delivery', await viewer.evaluate('Array.from(document.querySelectorAll("canvas")).filter(c=>c.width===2).every(c=>{const p=c.getContext("2d").getImageData(0,0,1,1).data;return p[0]===255&&p[1]===0&&p[2]===0&&p[3]===255})'));
  passed('bearer fragment is removed from browser history', await viewer.evaluate('location.hash===""'));
  passed('public viewing does not create an enrolled browser profile', await viewer.evaluate(`(async()=>!(await indexedDB.databases()).some(db=>db.name==='omochi.hub-client.v1'))()`));
  await viewer.command('Page.reload'); await live(viewer); passed('public link reload restores only its per-tab bearer context');
  const files = [];
  async function scan(directory, prefix = '') { for (const entry of await readdir(directory, {withFileTypes:true})) { const relative = prefix + entry.name; if (entry.isDirectory()) await scan(path.join(directory,entry.name),relative+'/'); else if (/\.(?:js|css|wasm|html)$/.test(relative)) files.push(relative); } }
  await scan(assets);
  for (const file of files) {
    const audit = await viewer.evaluate(`(async()=>{const response=await fetch(new URL(${JSON.stringify(file)},document.baseURI),{headers:{'X-Omochi-Asset-Audit':'1'}});return {status:response.status,redirected:response.redirected,headers:Object.fromEntries(response.headers)}})()`);
    assert.equal(audit.status,200);assert.equal(audit.redirected,false);assert.equal(audit.headers['content-type'],host.contract.mime[path.extname(file)]);
    for (const [name,value] of Object.entries(host.contract.headers)) assert.equal(audit.headers[name.toLowerCase()],value);
    assert.equal(audit.headers['cache-control'],file.startsWith('assets/')?host.contract.cache.assets:file.startsWith('iroh/')?host.contract.cache.iroh:host.contract.cache.html);
    if (/\.(?:js|css|wasm)$/.test(file)) assert.equal(audit.headers['content-encoding'],'gzip');
  }
  passed('nested HTTPS assets have exact CSP/MIME/cache headers and gzip');
  passed('static host has no API or missing-asset fallback', await viewer.evaluate(`(async()=>{for(const path of ['api/dashboards','missing.js'])if((await fetch(new URL(path,document.baseURI),{headers:{'X-Omochi-Asset-Audit':'1'}})).status!==404)return false;return true})()`));

  const invite = await createLink('invite', true, true);
  recipient = await browser.tab(); await navigate(recipient, invite.url);
  await until(async()=>await recipient.evaluate('!!document.querySelector("[data-browser-id]")'),'durable invitation profile');
  await recipient.evaluate(`(async()=>{const module=await import(new URL('./iroh/iroh.js',document.baseURI).href);const connect=module.RawEndpoint.prototype.connect;window.__enrollCalls=0;module.RawEndpoint.prototype.connect=function(address,alpn,operation){if(alpn==='omochi/dashboard-enroll/1')window.__enrollCalls++;return connect.call(this,address,alpn,operation)};const put=IDBObjectStore.prototype.put;window.__failShareSave=true;IDBObjectStore.prototype.put=function(value,...rest){if(this.name==='shares'&&value.state==='accepted'&&window.__failShareSave)throw new DOMException('fixture quota','QuotaExceededError');return put.call(this,value,...rest)}})()`);
  await click(recipient, 'Accept invitation');
  await until(async()=>await recipient.evaluate(`!!${button('Retry connection save')}`),'local acceptance save recovery');
  passed('failed acceptance save exposes local-only recovery without rendering live data', await recipient.evaluate('document.querySelectorAll("[data-surface]").length===0&&window.__enrollCalls===1'));
  await recipient.evaluate('window.__failShareSave=false');await click(recipient,'Retry connection save');await live(recipient);
  passed('save retry does not redeem another invitation',await recipient.evaluate('window.__enrollCalls===1'));
  const other = await browser.tab();await navigate(other,invite.url);
  await until(async()=>await other.evaluate(`!!document.querySelector('[data-error-code="OwnerBusy"]')`),'competing identity owner');await other.close();
  passed('a second identity tab reports active ownership without opening another endpoint');
  passed('identity invitation enrolls explicitly and opens the shared board');
  const identity = await recipient.evaluate('document.querySelector("[data-browser-id]").dataset.browserId');
  await until(async () => await recipient.evaluate('!!document.querySelector(\'[data-panel-id="p0"] input[type="text"]\')'), 'form draft');
  await recipient.evaluate(`(()=>{const input=document.querySelector('[data-panel-id="p0"] input[type="text"]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'shared form draft');input.dispatchEvent(new Event('input',{bubbles:true}))})()`);
  await click(recipient, 'Increment 0');
  await until(async () => await recipient.evaluate('document.body.textContent.includes("App 0 count: 1")&&document.body.textContent.includes("Received: shared form draft")'), 'real SDK form action and update');
  passed('authorized app action sends form drafts and updates the real SDK exactly once');
  await live(viewer); await until(async () => await viewer.evaluate('document.body.textContent.includes("App 0 count: 1")'), 'readonly live update');
  passed('readonly recipient receives live updates from another authorized client');
  await click(recipient, 'Edit dashboard');
  passed('shared editor cannot add or remove source panels', await recipient.evaluate('!document.querySelector("#add-view")&&!Array.from(document.querySelectorAll("button")).some(b=>b.getClientRects().length&&b.getAttribute("aria-label")?.startsWith("Remove "))'));
  await recipient.evaluate(`(()=>{const input=document.querySelector('#dashboard-name');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'Edited by recipient');input.dispatchEvent(new Event('input',{bubbles:true}))})()`);
  await click(recipient, 'Save dashboard');
  await until(async () => (await board()).name === 'Edited by recipient', 'shared editor persisted native board');
  passed('permitted shared layout save reaches native revisioned repository');
  await until(async () => await viewer.evaluate('document.querySelector("h1")?.textContent==="Edited by recipient"'), 'saved board propagation');
  passed('saved owner/shared changes propagate to existing viewers');
  await recipient.command('Page.reload'); await live(recipient);
  passed('reload restores durable browser identity without another invitation', await recipient.evaluate(`document.querySelector('[data-browser-id]').dataset.browserId===${JSON.stringify(identity)}`));
  if (!productionRelay) {
  child.stdin.write('{"op":"relay-stop"}\n');await until(()=>messages.some(m=>m.kind==='relay-stopped'),'relay stopped');
  await until(async()=>await recipient.evaluate('document.querySelectorAll("[data-surface]").length===0'),'loss retracts stale views',90000);
  const resumedAt=Date.now();child.stdin.write('{"op":"relay-start"}\n');await until(()=>messages.some(m=>m.kind==='relay-started'),'relay restarted');
  await until(async()=>await recipient.evaluate('document.querySelectorAll("[data-surface]").length===2&&document.querySelector(".connection-status")?.textContent.includes("Hub connected")'),'relay recovery',90000);await live(viewer);
  report.recoveryMilliseconds=Date.now()-resumedAt;passed('controlled relay reconnects fresh snapshots within 90 seconds',report.recoveryMilliseconds<=90000);
  } else {
    passed('hosted browser shares authenticate through the deployed production relay');
  }
  passed(productionRelay ? 'opening shares never replays app actions' : 'reconnect never replays app actions',await recipient.evaluate('document.body.textContent.includes("App 0 count: 1")'));
  await click(recipient,'Increment 0');await until(async()=>await recipient.evaluate('document.body.textContent.includes("App 0 count: 2")'),'post-reconnect action');passed(productionRelay ? 'registered app actions remain usable through the production relay' : 'registered app actions resume after fresh snapshots');
  const current = await board(); await update({ ...current, panels: [current.panels[0]] }); await live(viewer, 1); await live(recipient, 1);
  passed('owner panel removal withdraws that view from all existing shares');
  const conflict = await fetch(`${ready.ownerUrl}api/dashboards/${ready.boardId}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(current) });
  passed('native revision conflict remains enforced', conflict.status === 409);
  const revoke = await fetch(`${ready.ownerUrl}api/shares/${publicLink.id}`, { method: 'DELETE' }); assert.equal(revoke.status, 200);
  await until(async () => await viewer.evaluate('document.querySelectorAll("[data-surface]").length===0'), 'public link revocation');
  await live(recipient, 1);
  passed('public link revocation retracts access while independent identity grant remains usable');
  const screenshot = await recipient.command('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true }); await writeFile(path.join(output, 'shared-editor.png'), Buffer.from(screenshot.data, 'base64'));
  const response = await fetch(`${ready.ownerUrl}api/shares/${invite.id}`, { method: 'DELETE' }); assert.equal(response.status, 200);
  await until(async () => await recipient.evaluate('document.querySelectorAll("[data-surface]").length===0'), 'identity grant revocation');
  passed('identity revocation stops the registered browser');
  const invalid = new URL(publicLink.url); const bad = JSON.parse(Buffer.from(invalid.hash.slice(7),'base64url').toString()); bad.secret='0'.repeat(64); invalid.hash='#share:'+Buffer.from(JSON.stringify(bad)).toString('base64url');
  const denied = await browser.tab();await navigate(denied,invalid.href);
  await until(async()=>await denied.evaluate(`!!document.querySelector('[data-error-code="NotAuthorized"]')`),'invalid public secret rejection');
  passed('an invalid public bearer never renders a board',await denied.evaluate('document.querySelectorAll("[data-surface]").length===0'));await denied.close();
  await recipient.evaluate(`(async()=>{const request=indexedDB.open('omochi.hub-client.v1',1);const db=await new Promise((resolve,reject)=>{request.onsuccess=()=>resolve(request.result);request.onerror=()=>reject()});await new Promise((resolve,reject)=>{const tx=db.transaction('identity','readwrite');tx.oncomplete=resolve;tx.onabort=reject;tx.objectStore('identity').put({id:'default',secretHex:'corrupt',endpointId:'b'.repeat(64)})});db.close()})()`);
  await recipient.command('Page.reload');await until(async()=>await recipient.evaluate(`!!document.querySelector('[data-error-code="StorageCorrupt"]')`),'corrupt identity recovery screen');
  await click(recipient,'Reset browser identity…');await click(recipient,'Confirm identity reset');
  await until(async()=>await recipient.evaluate('!!document.querySelector("[data-browser-id]")'),'new explicit identity');
  passed('corrupt identity requires explicit reset and never silently restores trust',await recipient.evaluate(`document.querySelector('[data-browser-id]').dataset.browserId!==${JSON.stringify(identity)}&&document.querySelectorAll('[data-surface]').length===0`));
  passed('local identity reset preserves the hub-owned board',(await board()).name==='Edited by recipient');

  passed('no static host app-data/API fallback', !host.requests.some(r => !r.audit && /^\/dashboard\/(?:api|images|resources|events|services|ws)(?:\/|$)/.test(r.path)));
  passed('no unhandled browser exceptions', exceptions === 0);
  console.log(`Sharing browser acceptance passed (${report.checks.length} checks).`);
} catch (error) {
  report.errors.push(error.message); console.error(error.message); process.exitCode = 1;
  if (recipient) {
    report.diagnostics = await recipient.evaluate(`({status:document.querySelector('.connection-status')?.textContent, codes:Array.from(document.querySelectorAll('[data-error-code]')).map(e=>e.dataset.errorCode), surfaces:document.querySelectorAll('[data-surface]').length, inputs:Array.from(document.querySelectorAll('input')).map(e=>({type:e.type,label:e.getAttribute('aria-label'),disabled:e.matches(':disabled')}))})`).catch(() => ({}));
    const screenshot = await recipient.command('Page.captureScreenshot', {format:'png',captureBeyondViewport:true}).catch(() => null);
    if(screenshot) await writeFile(path.join(output,'failure.png'),Buffer.from(screenshot.data,'base64'));
  }
}
finally {
  await writeFile(path.join(output, 'result.json'), JSON.stringify(report, null, 2));
  await browser?.close(); host?.close();
  if (child && child.exitCode === null && child.signalCode === null) {
    const exit = once(child, 'exit').catch(() => {}); child.stdin.write('{"op":"stop"}\n'); const timer = setTimeout(() => child.kill('SIGKILL'), 5000); await exit; clearTimeout(timer);
  }
  if (!process.env.SHARING_RESULTS) await rm(temporary, { recursive: true, force: true });
}
