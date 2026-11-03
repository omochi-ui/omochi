import assert from 'node:assert/strict';
import { readdir, readFile, writeFile } from 'node:fs/promises';
import { builtinModules } from 'node:module';
import path from 'node:path';
import { fixtureBoard, importBoardFile } from './boards-ui.mjs';

/** Production artifact, nested HTTPS base, exact headers and real relay recovery. */
export async function hostedChecks({ browser, host, fixture, until, passed, output, report, assets, publicRelay }) {
  report.chromium = await browser.command('Browser.getVersion');
  assert(Number(report.chromium.product.match(/\/(\d+)/)?.[1]) >= 156, 'Chromium 156+ required');
  await browser.command('Page.navigate', { url: host.url });
  await until(async () => await browser.evaluate('!!document.querySelector("[data-browser-id]")'), 'nested-base hosted profile');
  passed('secure Chromium facilities', await browser.evaluate('isSecureContext && !!indexedDB && !!navigator.locks && !!WebAssembly && !!AbortController && !!DecompressionStream && !!createImageBitmap'));
  const capabilities = await browser.evaluate('({userAgent:navigator.userAgent,language:navigator.language})');
  report.browser = capabilities;
  const files = [];
  async function scan(directory, prefix = '') {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const name = prefix + entry.name;
      if (entry.isDirectory()) await scan(path.join(directory, entry.name), name + '/');
      else files.push(name);
    }
  }
  await scan(assets);
  const builtins = new Set(builtinModules.flatMap(name => [name, `node:${name}`]));
  for (const file of files.filter(name => name.endsWith('.js'))) {
    const text = await readFile(path.join(assets, file), 'utf8');
    assert(!text.includes('__vite-browser-external'), 'Browser bundle contains an externalized Node module');
    for (const match of text.matchAll(/(?:\bfrom\s*|\bimport\s*\(\s*|\bimport\s*|\brequire\s*\(\s*)["']([^"']+)["']/g)) {
      assert(!builtins.has(match[1]) && !match[1].startsWith('node:'), 'Browser bundle imports a Node builtin');
    }
  }
  passed('production JS contains no Node builtin imports');
  const audit = async relative => browser.evaluate(`(async()=>{const response=await fetch(new URL(${JSON.stringify(relative)},document.baseURI),{headers:{'X-Omochi-Asset-Audit':'1'}});const bytes=(await response.arrayBuffer()).byteLength;return {status:response.status,redirected:response.redirected,headers:Object.fromEntries(response.headers),bytes}})()`);
  report.assets = [];
  for (const file of ['index.html', ...files.filter(name => /\.(?:js|wasm|css)$/.test(name))]) {
    const response = await audit(file);
    assert.equal(response.status, 200); assert.equal(response.redirected, false);
    for (const [name, value] of Object.entries(host.contract.headers)) assert.equal(response.headers[name.toLowerCase()], value);
    assert.equal(response.headers['content-type'], host.contract.mime[path.extname(file)]);
    assert.equal(response.headers['cache-control'], file.startsWith('assets/') ? host.contract.cache.assets : file.startsWith('iroh/') ? host.contract.cache.iroh : host.contract.cache.html);
    const source = await readFile(path.join(assets, file));
    assert.equal(response.bytes, source.length, 'Compressed transfer must decode to the original asset');
    if (/\.(?:js|wasm|css)$/.test(file)) assert.equal(response.headers['content-encoding'], 'gzip');
    report.assets.push({ file, bytes: source.length, gzipBytes: Bun.gzipSync(source).length });
  }
  passed('nested assets preserve exact MIME CSP cache and gzip headers');
  for (const missing of ['missing.js', 'iroh/missing.wasm', 'deep/route', 'api/services', 'images/missing', 'resources/missing', 'events', 'ws']) {
    const response = await audit(missing);
    assert.equal(response.status, 404); assert.equal(response.redirected, false);
    for (const [name, value] of Object.entries(host.contract.headers)) assert.equal(response.headers[name.toLowerCase()], value);
  }
  passed('missing assets and application endpoints return 404 without index fallback');
  passed('hosted runtime has no fixture globals', await browser.evaluate('typeof window.__browserTest === "undefined" && typeof window.__BROWSER_FIXTURE__ === "undefined"'));
  await browser.evaluate(`(async()=>{const wasm=await import(new URL('./iroh/iroh.js',document.baseURI).href);const close=wasm.RawEndpoint.prototype.close;window.__hostedAudit={closed:0,remainingFrames:0};wasm.RawEndpoint.prototype.close=function(...args){return close.apply(this,args).then(()=>{window.__hostedAudit.closed++;window.__hostedAudit.remainingFrames+=this.frame_bytes()})};})()`);
  await importBoardFile(browser, output, fixtureBoard(fixture), until);
  const button = (text, scope = 'document') => `Array.from((${scope}).querySelectorAll('button')).find(button=>button.textContent===${JSON.stringify(text)})`;
  const click = async (text, scope) => {
    await until(async () => await browser.evaluate(`!!${button(text, scope)} && !${button(text, scope)}.disabled`), 'enabled hosted control');
    await browser.evaluate(`${button(text, scope)}.click()`);
  };
  for (const gateway of fixture.ready.gateways) {
    await browser.evaluate(`(()=>{const input=document.querySelector('#gateway-invitation');Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(input,${JSON.stringify(gateway.invitation)});input.dispatchEvent(new Event('input',{bubbles:true}))})()`);
    await click('Accept or refresh invitation');
    const scope = `document.querySelector('[data-gateway="${gateway.nodeId}"]')`;
    await until(async () => await browser.evaluate(`!!(${scope} && ${button('Connect', scope)})`), 'hosted enrollment');
    await click('Connect', scope);
  }
  await until(async () => await browser.evaluate('document.querySelectorAll("[data-surface]").length===8'), 'nested hosted eight SDK surfaces');
  passed('gzip nested-base artifact connects two real gateways and eight services');
  if (!publicRelay) {
    fixture.send({ op: 'stop-relay' });
    await fixture.until(() => fixture.messages.some(message => message.kind === 'relay-stopped'), 'relay shutdown');
    await until(async () => await browser.evaluate('document.querySelectorAll("[data-surface]").length===0'), 'control loss retracts all leases', 90000);
    passed('relay loss retracts surfaces before recovery');
    const resumedAt = Date.now(); fixture.send({ op: 'restart-relay' });
    await fixture.until(() => fixture.messages.some(message => message.kind === 'relay-restarted'), 'same-URL relay restart');
    await until(async () => await browser.evaluate('document.querySelectorAll("[data-surface]").length===8'), 'automatic fresh snapshots after relay recovery', Math.max(1, 90000 - (Date.now() - resumedAt)));
    report.recoveryMilliseconds = Date.now() - resumedAt;
    passed('controlled relay recovers eight fresh snapshots within 90 seconds', report.recoveryMilliseconds <= 90000);
    const reference = `${fixture.ready.gateways[0].nodeId}~s/main/nested`;
    await browser.evaluate(`Array.from(Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(reference)}).querySelectorAll('button')).find(button=>button.textContent==='Increment 0').click()`);
    await until(async () => await browser.evaluate(`Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(reference)})?.textContent.includes('count: 1')`), 'post-recovery real SDK action');
    passed('recovered SDK action executes once');
  } else report.controlledRelay = 'not run in public-relay smoke mode';
  const screenshot = await browser.command('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true });
  await writeFile(path.join(output, 'hosted.png'), Buffer.from(screenshot.data, 'base64'));
  for (const gateway of fixture.ready.gateways) await click('Disconnect', `document.querySelector('[data-gateway="${gateway.nodeId}"]')`);
  await until(async () => await browser.evaluate('document.querySelectorAll("[data-surface]").length===0'), 'explicit disconnect retraction');
  await until(async () => await browser.evaluate('window.__hostedAudit.closed>=2'), 'owned endpoints join on disconnect');
  passed('closed WASM endpoints retain zero frame bytes', await browser.evaluate('window.__hostedAudit.remainingFrames===0'));
  passed('all hosted application assets stay under the nested base', host.requests.every(request => request.audit || request.path.startsWith('/dashboard/')));
}
