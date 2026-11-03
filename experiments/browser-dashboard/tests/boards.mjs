import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { nativeBoardHub } from './fixtures.mjs';
import { chooseBoardFile, fixtureBoard, importBoardFile, previewBoardFile } from './boards-ui.mjs';

export async function boardsChecks({ browser, host, fixture, until, passed, output, temporary }) {
  const document = fixtureBoard(fixture), node = fixture.ready.gateways[0].nodeId;
  document.sources[0].name = '<b>Advisory only</b>';
  let page = browser;
  const button = (text, scope = 'document') => `Array.from((${scope})?.querySelectorAll('button')??[]).find(button=>button.textContent.trim()===${JSON.stringify(text)})`;
  const click = async (text, scope) => {
    await until(async () => await page.evaluate(`!!(${button(text, scope)})&&!(${button(text, scope)}).disabled`), `enabled ${text}`);
    await page.evaluate(`${button(text, scope)}.click()`);
  };
  const input = async (selector, value) => page.evaluate(`(()=>{const el=document.querySelector(${JSON.stringify(selector)});Object.getOwnPropertyDescriptor(el.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype,'value').set.call(el,${JSON.stringify(value)});el.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  const boardId = () => page.evaluate('new URLSearchParams(location.search).get("dashboard")');
  const rows = () => browser.evaluate(`new Promise((resolve,reject)=>{const request=indexedDB.open('omochi.browser.v1',1);request.onsuccess=()=>{const db=request.result;const tx=db.transaction('boards','readonly');const get=tx.objectStore('boards').getAll();get.onsuccess=()=>{db.close();resolve(get.result)};get.onerror=reject}})`);
  const profile = () => browser.evaluate(`new Promise((resolve,reject)=>{const request=indexedDB.open('omochi.browser.v1',1);request.onsuccess=()=>{const db=request.result;const tx=db.transaction(['identity','gateways','preferences'],'readonly');const result={};for(const name of ['identity','gateways','preferences']){const get=tx.objectStore(name).getAll();get.onsuccess=()=>{result[name]=name==='identity'?get.result.map(value=>({endpointId:value.endpointId})):get.result}}tx.oncomplete=()=>{db.close();resolve(result)};tx.onabort=reject}})`);
  const download = async label => {
    const directory = await mkdtemp(path.join(output, `download-${label}-`));
    await browser.command('Browser.setDownloadBehavior', { behavior: 'allow', downloadPath: directory });
    await click('Export board file'); const filename = path.join(directory, 'omochi-board.json');
    await until(async () => await Bun.file(filename).exists(), 'board JSON download');
    return await Bun.file(filename).json();
  };
  await browser.command('Page.navigate', { url: host.url });
  await until(async () => await browser.evaluate('!!document.querySelector("[data-browser-id]")&&!Array.from(document.querySelectorAll("button")).find(button=>button.textContent==="Import board file")?.disabled'), 'board-only hosted UI');
  await browser.evaluate(`(async()=>{const wasm=await import(new URL('./iroh/iroh.js',document.baseURI).href);const counters=window.__boardNet={bind:0,pair:0,action:0,service:0};
    const bind=wasm.RawEndpoint.create,connect=wasm.RawEndpoint.prototype.connect,open=wasm.RawConnection.prototype.open,send=wasm.RawStream.prototype.send;
    wasm.RawEndpoint.create=function(...args){counters.bind++;return bind.apply(this,args)};
    wasm.RawEndpoint.prototype.connect=function(address,alpn,...rest){if(alpn==='omochi/pair/1')counters.pair++;return connect.call(this,address,alpn,...rest)};
    wasm.RawConnection.prototype.open=function(header,...rest){if(JSON.parse(header).kind==='service')counters.service++;return open.call(this,header,...rest)};
    wasm.RawStream.prototype.send=function(value){if(Object.hasOwn(JSON.parse(value),'action'))counters.action++;return send.call(this,value)};})()`);
  const originalProfile = await profile();
  await previewBoardFile(browser, output, document, until);
  passed('import previews source names as advisory text without interpreting HTML', await browser.evaluate('document.querySelector("dialog[open] ul").textContent.includes("<b>Advisory only</b>")&&!document.querySelector("dialog[open] ul b")'));
  await click('Import as new dashboard', 'document.querySelector("dialog[open]")');
  await until(async () => (await rows()).length === 1 && await boardId(), 'browser import commit');
  const first = await boardId();
  passed('browser import allocates a local UUID and revision one', /^[a-f0-9-]{36}$/.test(first) && (await rows())[0].revision === 1);
  passed('import retains exact panel/reference IDs and geometry', assert.deepEqual((await rows())[0].panels, document.board.panels) === undefined);
  passed('missing sources remain unavailable without network or trust mutation', (await browser.evaluate('JSON.stringify(window.__boardNet)')) === '{"bind":0,"pair":0,"action":0,"service":0}' &&
    JSON.stringify(await profile()) === JSON.stringify(originalProfile) && await browser.evaluate('document.querySelectorAll(".unavailable").length===8&&document.querySelectorAll("[data-surface]").length===0'));
  await importBoardFile(browser, output, document, until, 'same-board-copy.json');
  const second = await boardId();
  passed('importing the same file again creates an independent copy', second !== first && (await rows()).length === 2);
  const exported = await download('browser');
  assert.deepEqual(exported.board, document.board);
  passed('export excludes IDs, revisions, timestamps, profiles, addresses and secrets', Object.keys(exported).sort().join(',') === 'board,format,sources,version' &&
    Object.keys(exported.board).sort().join(',') === 'name,panels' && exported.sources.every(source => Object.keys(source).join(',') === 'nodeId'));

  const native = await nativeBoardHub(temporary);
  let nativeTab;
  try {
    nativeTab = await browser.tab(); await nativeTab.command('Page.enable'); await nativeTab.command('Runtime.enable');
    await nativeTab.command('Page.navigate', { url: native.url }); page = nativeTab;
    await until(async () => await page.evaluate('!!document.querySelector("#import-board-file")&&!document.querySelector(".empty-state")?.textContent.includes("Loading")'), 'native HTTP repository UI');
    await importBoardFile(page, output, exported, until, 'browser-to-native.json'); const nativeId = await boardId();
    let response = await fetch(`${native.url}api/dashboards/${nativeId}`); const saved = await response.json();
    passed('native HTTP repository imports browser exchange as a new board', nativeId !== first && nativeId !== second && saved.revision === 1);
    assert.deepEqual(saved.panels, document.board.panels);
    const nativeExport = await download('native');
    passed('native and browser exchange round-trip the same configuration', assert.deepEqual(nativeExport, exported) === undefined);
    page = browser; await importBoardFile(page, output, nativeExport, until, 'native-to-browser.json'); const third = await boardId();
    passed('native export imports as another browser-local copy', third !== nativeId && (await rows()).length === 3);
    page = nativeTab; await click('Edit dashboard'); await input('#dashboard-name', 'Native edited'); await click('Save dashboard');
    await until(async () => await page.evaluate('document.querySelector(".announcement")?.textContent.includes("Dashboard saved")'), 'native revision two');
    response = await fetch(`${native.url}api/dashboards/${nativeId}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ ...document.board, revision: 1 }) });
    passed('real native HTTP update rejects stale revisions', response.status === 409);
    response = await fetch(`${native.url}api/dashboards/${nativeId}`, { method: 'DELETE', headers: { 'Content-Type': 'application/json' }, body: '{"revision":1}' });
    passed('real native HTTP delete rejects stale revisions', response.status === 409);
    await click('Delete dashboard'); await click('Delete dashboard', 'document.querySelector("dialog[open]")');
    await until(async () => (await fetch(`${native.url}api/dashboards/${nativeId}`)).status === 404, 'native board deletion');
    passed('sender edits/deletion do not change browser copies or source profiles', (await rows()).every(row => row.name === document.board.name) && JSON.stringify(await profile()) === JSON.stringify(originalProfile));
  } finally { await nativeTab?.close(); await native.close(); page = browser; }

  await click('Edit dashboard'); await input('#dashboard-name', 'Draft survives quota');
  await browser.evaluate(`(()=>{const put=IDBObjectStore.prototype.put;window.__restoreBoardPut=()=>{IDBObjectStore.prototype.put=put;delete window.__restoreBoardPut};IDBObjectStore.prototype.put=function(value,...rest){if(this.name==='boards')throw new DOMException('quota','QuotaExceededError');return put.call(this,value,...rest)}})()`);
  await click('Save dashboard');
  await until(async () => await browser.evaluate('document.querySelector(".notice.error")?.textContent.includes("storage is unavailable")'), 'board quota failure');
  passed('quota failure keeps edits and does not advance stored revision', await browser.evaluate('document.querySelector("#dashboard-name").value==="Draft survives quota"') && (await rows()).every(row => row.revision === 1));
  await browser.evaluate('window.__restoreBoardPut()'); await click('Retry save');
  await until(async () => (await rows()).some(row => row.revision === 2), 'quota retry commit');
  await click('Edit dashboard'); await input('#dashboard-name', 'Draft survives abort');
  await browser.evaluate(`(()=>{const put=IDBObjectStore.prototype.put;window.__restoreBoardPut=()=>{IDBObjectStore.prototype.put=put;delete window.__restoreBoardPut};IDBObjectStore.prototype.put=function(...args){const request=put.apply(this,args);if(this.name==='boards')request.addEventListener('success',()=>this.transaction.abort(),{once:true});return request}})()`);
  await click('Save dashboard'); await until(async () => await browser.evaluate('!!document.querySelector(".notice.error")'), 'post-put board transaction abort');
  passed('transaction abort after request success keeps edits and the prior committed board', await browser.evaluate('document.querySelector("#dashboard-name").value==="Draft survives abort"') && (await rows()).every(row => row.revision <= 2));
  await browser.evaluate('window.__restoreBoardPut()'); await click('Retry save');
  await until(async () => (await rows()).some(row => row.revision === 3), 'abort retry commit');
  const current = await boardId();
  const other = await browser.tab();
  try {
    await other.command('Page.enable'); await other.command('Runtime.enable'); await other.command('Page.navigate', { url: `${host.url}?dashboard=${current}` });
    await until(async () => await other.evaluate('!!document.querySelector("[data-error-code=OwnerBusy]")&&!!Array.from(document.querySelectorAll("button")).find(button=>button.textContent==="Edit dashboard")'), 'board-only tab with active endpoint owner');
    page = other; await click('Edit dashboard'); await input('#dashboard-name', 'Other tab draft');
    page = browser; await click('Edit dashboard'); await input('#dashboard-name', 'First tab wins'); await click('Save dashboard');
    await until(async () => (await rows()).some(row => row.revision === 4), 'first tab CAS write');
    page = other; await click('Save dashboard');
    await until(async () => await other.evaluate('document.querySelector(".notice.error")?.textContent.includes("changed elsewhere")'), 'cross-tab stale revision');
    passed('a board-only tab edits independently and preserves its draft on CAS conflict', await other.evaluate('document.querySelector("#dashboard-name").value==="Other tab draft"'));
    await click('Discard and reload'); await click('Discard changes', 'document.querySelector("dialog[open]")');
    await until(async () => await other.evaluate('document.querySelector(".page-header h1")?.textContent==="First tab wins"'), 'post-commit broadcast reload');
    passed('discard/reload observes the committed revision from another tab');
  } finally { await other.close(); page = browser; }
  passed('board CRUD has no pairing, endpoint binding or action side effects', (await browser.evaluate('JSON.stringify(window.__boardNet)')) === '{"bind":0,"pair":0,"action":0,"service":0}');
  const beforeInvalid = JSON.stringify(await rows()), invalid = structuredClone(document);
  invalid.sources[0].relayUrls = ['https://untrusted.invalid/'];
  await chooseBoardFile(browser, output, invalid, until, 'invalid-board.json');
  await until(async () => await browser.evaluate('document.querySelector(".notice.error")?.textContent.includes("board is invalid")'), 'invalid board rejection');
  passed('unknown nested source fields are rejected before writes or connection changes', JSON.stringify(await rows()) === beforeInvalid &&
    JSON.stringify(await profile()) === JSON.stringify(originalProfile) && await browser.evaluate('window.__boardNet.bind===0&&!document.querySelector("dialog[open]")'));
  for (let index = 0; index < fixture.ready.gateways.length; index++) {
    await input('#gateway-invitation', fixture.ready.gateways[index].invitation); await click('Accept or refresh invitation');
    await until(async () => await browser.evaluate(`!!${button('Connect', `document.querySelector('[data-gateway="${fixture.ready.gateways[index].nodeId}"]')`)}`), 'explicit board source enrollment');
    await click('Connect', `document.querySelector('[data-gateway="${fixture.ready.gateways[index].nodeId}"]')`);
  }
  await until(async () => await browser.evaluate('document.querySelectorAll("[data-surface]").length===8'), 'explicitly connected board Surfaces');
  passed('only explicit Connect activates imported board sources');
  await click('Create dashboard', 'document.querySelector("aside")');
  await until(async () => await browser.evaluate('!!document.querySelector("#dashboard-name")&&document.querySelectorAll("[data-surface]").length===0'), 'new board releases prior subscriptions');
  await input('#dashboard-name', 'Picker subscription'); const beforePicker = await browser.evaluate('window.__boardNet.service');
  await click('Add view');
  passed('opening the picker alone subscribes to no services', await browser.evaluate(`window.__boardNet.service===${beforePicker}`));
  await browser.evaluate('document.querySelector(".service-options button").click()');
  await until(async () => await browser.evaluate('document.querySelectorAll(".app-options button").length===1'), 'explicit picker service snapshot');
  passed('picker selection subscribes to exactly one source service', await browser.evaluate(`window.__boardNet.service===${beforePicker + 1}`));
  await browser.evaluate('document.querySelector(".app-options button").click()'); await click('Save dashboard');
  await until(async () => await browser.evaluate('document.querySelector(".page-header h1")?.textContent==="Picker subscription"&&document.querySelectorAll("[data-surface]").length===1'), 'picker board commit');
  const live = await download('connected');
  passed('export labels come from authenticated sources rather than import hints', live.sources.length === 1 && live.sources[0].name === 'Fixture 0' && live.sources[0].nodeId === node);
  await browser.evaluate('document.querySelector(".connection-settings").open=false');
  await browser.command('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1000, deviceScaleFactor: 1, mobile: false });
  const screenshot = await browser.command('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true });
  await writeFile(path.join(output, 'boards.png'), Buffer.from(screenshot.data, 'base64'));
  const profileBeforeDelete = await profile(); await click('Delete dashboard'); await click('Delete dashboard', 'document.querySelector("dialog[open]")');
  await until(async () => await browser.evaluate('document.querySelector(".announcement")?.textContent.includes("Dashboard deleted")'), 'browser board deletion');
  passed('deleting a board preserves independent gateway profiles and enrollment', JSON.stringify(await profile()) === JSON.stringify(profileBeforeDelete));
}
