// Run after bun run build:native: bun packages/hub-ui/test/browser.mjs
// Native services only. SCREENSHOT_DIR and CHROMIUM_BIN may override defaults.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { once } from 'node:events';
const temporary = await mkdtemp(path.join(tmpdir(), 'ob-'));
const output = process.env.SCREENSHOT_DIR ?? '/tmp/omochi-hub-screenshots';
await mkdir(output, { recursive: true });
const children = [];
let socket;
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(check, description) {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) { if (await check()) return; await delay(50); }
  throw new Error(`Timed out: ${description}`);
}
async function start(binary, args, pattern) {
  const child = spawn(path.resolve('crates/target/debug', binary), args, { stdio: ['ignore', 'pipe', 'pipe'] });
  children.push(child);
  let log = '', match;
  child.stderr.on('data', chunk => { log += chunk; match = log.match(pattern); });
  child.on('error', error => { log += error.message; });
  await until(() => { if (child.exitCode !== null) throw new Error(log); return match; }, `${binary}: ${args.join(' ')}`);
  return { child, match };
}
try {
  const runtime = path.join(temporary, 'run');
  const { match } = await start('omochi-ui', ['--config-dir', path.join(temporary, 'config'), '--runtime-dir', runtime, 'hub', '--port', '0'], /dashboard (http:\/\/127\.0\.0\.1:\d+\/)/);
  const url = match[1];
  const utility = await start('omochi-utils', ['button', '--service-id', 'tasks', '--runtime-dir', runtime, '--', '/bin/echo', 'dashboard-action-complete'], /Published service/);
  await until(async () => (await (await fetch(`${url}services`)).json()).some(s => s.status === 'connected'), 'service discovery');
  const services = await (await fetch(`${url}services`)).json();
  const service = services.find(s => s.status === 'connected');
  const reference = { nodeId: service.node.id, serviceId: 'tasks', surfaceId: 'main' };
  const response = await fetch(`${url}api/dashboards`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name: 'Daily overview', panels: [{ id: 'one', title: 'Task runner', reference, x: 0, y: 0, w: 6, h: 6 }] }) });
  assert.equal(response.ok, true, await response.clone().text());
  const dashboard = await response.json();
  const chrome = spawn(process.env.CHROMIUM_BIN ?? 'chromium', ['--headless', '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage', '--no-first-run', '--no-default-browser-check', '--remote-debugging-port=0', `--user-data-dir=${temporary}/chrome`, 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
  children.push(chrome);
  let browserUrl = '';
  chrome.stderr.on('data', chunk => { const match = chunk.toString().match(/DevTools listening on (ws:\/\/\S+)/); if (match) browserUrl = match[1]; });
  await until(() => browserUrl, 'Chromium startup');
  const endpoint = new URL(browserUrl);
  const targets = await (await fetch(`http://${endpoint.host}/json/list`)).json();
  socket = new WebSocket(targets.find(t => t.type === 'page').webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
  let sequence = 0;
  const pending = new Map();
  const errors = [];
  socket.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    if (message.id) {
      const task = pending.get(message.id); pending.delete(message.id);
      if (message.error) task.reject(new Error(JSON.stringify(message.error))); else task.resolve(message.result);
    } else if (message.method === 'Runtime.exceptionThrown') errors.push(message.params.exceptionDetails);
  });
  const cdp = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++sequence; pending.set(id, { resolve, reject }); socket.send(JSON.stringify({ id, method, params }));
  });
  const evaluate = async expression => {
    const result = await cdp('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  const text = () => evaluate('document.body.innerText');
  const click = async label => {
    await until(() => evaluate(`Array.from(document.querySelectorAll('button')).some(b => b.textContent.replace('▧', '').trim() === ${JSON.stringify(label)} && !b.disabled)`), `button ${label}`);
    await evaluate(`(() => { const b = Array.from(document.querySelectorAll('button')).find(b => b.textContent.trim() === ${JSON.stringify(label)} && !b.disabled && b.getClientRects().length); b.focus(); b.click(); })()`);
  };
  const input = async (selector, value) => {
    await evaluate(`(() => { const el = document.querySelector(${JSON.stringify(selector)}); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, ${JSON.stringify(value)}); el.dispatchEvent(new Event('input', { bubbles: true })); })()`);
  };
  const screenshot = async name => {
    const image = await cdp('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true });
    await writeFile(path.join(output, name), Buffer.from(image.data, 'base64'));
  };
  await cdp('Runtime.enable'); await cdp('Page.enable');
  await cdp('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1000, deviceScaleFactor: 1, mobile: false });
  await cdp('Page.navigate', { url: `${url}?dashboard=${dashboard.id}` });
  await until(async () => (await text()).includes('Task runner') && (await text()).includes('Ready'), 'live dashboard');
  await screenshot('dashboard.png');
  if (process.env.CAPTURE_ONLY) { console.log(`Baseline: ${output}`); } else {
    await click('Board view');
    assert.equal(await evaluate("getComputedStyle(document.querySelector('.sidebar')).display"), 'none');
    assert.equal(await evaluate("new URL(location.href).searchParams.get('view')"), 'board');
    // The live surface DOM survives both shell transitions.
    await evaluate("void (window.savedSurface = document.querySelector('.omochi-surface'))");
    await click('Run');
    await until(async () => (await text()).includes('dashboard-action-complete'), 'action in board mode');
    await screenshot('board.png');
    await click('Exit board view');
    assert.equal(await evaluate("window.savedSurface === document.querySelector('.omochi-surface')"), true);
    const colors = await evaluate(`['.workspace', '.connection-status', '.primary', '.dashboard-link.selected'].map(selector => { const e=document.querySelector(selector); const c=getComputedStyle(e); let bg=c.backgroundColor, parent=e.parentElement; while(bg==='rgba(0, 0, 0, 0)' && parent) { bg=getComputedStyle(parent).backgroundColor; parent=parent.parentElement; } return {selector,color:c.color,background:bg}; })`);
    const luminance = color => color.match(/[\d.]+/g).slice(0,3).map(Number).map(v=>v/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4).reduce((sum,v,i)=>sum+v*[.2126,.7152,.0722][i],0);
    for(const pair of colors) { const a=luminance(pair.color), b=luminance(pair.background); pair.ratio=(Math.max(a,b)+.05)/(Math.min(a,b)+.05); assert.ok(pair.ratio>=4.5,JSON.stringify(pair)); }
    await writeFile(path.join(output,'contrast.json'),JSON.stringify(colors,null,2));
    await click('Edit dashboard');
    await input('[aria-label="Dashboard name"]', ''); await click('Save dashboard');
    await until(() => evaluate("document.activeElement.id === 'dashboard-name' && document.activeElement.getAttribute('aria-invalid') === 'true'"), 'name validation');
    await input('[aria-label="Dashboard name"]', 'Daily overview');
    assert.equal(await evaluate("document.querySelectorAll('.panel input[type=number]').length"), 0);
    assert.equal(await evaluate("document.querySelectorAll('.resize-edge').length"), 8);
    await click('Adjust layout'); await click('Wider');
    assert.equal(await evaluate("document.querySelector('.panel').style.gridColumn"), '1 / span 7');
    // Pointer moves are previews; escape returns the original rectangle.
    const bounds = await evaluate("(() => { const r = document.querySelector('.drag-handle').getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()");
    await screenshot('before-drag.png');
    await cdp('Input.dispatchMouseEvent', { type: 'mouseMoved', ...bounds });
    await cdp('Input.dispatchMouseEvent', { type: 'mousePressed', ...bounds, button: 'left', buttons: 1, clickCount: 1 });
    await cdp('Input.dispatchMouseEvent', { type: 'mouseMoved', x: bounds.x, y: bounds.y + 112, button: 'left', buttons: 1 });
    await until(() => evaluate("document.querySelector('.panel').style.gridRow === '3 / span 6'"), 'move preview');
    await cdp('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Escape', code: 'Escape' });
    await cdp('Input.dispatchMouseEvent', { type: 'mouseReleased', x: bounds.x, y: bounds.y + 112, button: 'left' });
    assert.equal(await evaluate("document.querySelector('.panel').style.gridRow"), '1 / span 6');
    const resizeBounds = await evaluate("(() => { const r=document.querySelector('.resize-se').getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()");
    await cdp('Input.dispatchMouseEvent', { type: 'mouseMoved', ...resizeBounds });
    await cdp('Input.dispatchMouseEvent', { type: 'mousePressed', ...resizeBounds, button: 'left', buttons: 1, clickCount: 1 });
    await cdp('Input.dispatchMouseEvent', { type: 'mouseMoved', x: resizeBounds.x, y: resizeBounds.y+56, button: 'left', buttons: 1 });
    await until(()=>evaluate("document.querySelector('.panel').style.gridRow==='1 / span 7'"),'resize preview');
    await cdp('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Escape', code: 'Escape' });
    await cdp('Input.dispatchMouseEvent', { type: 'mouseReleased', x: resizeBounds.x, y: resizeBounds.y+56, button: 'left' });
    await evaluate("document.querySelector('.drag-handle').focus()");
    await cdp('Input.dispatchKeyEvent', { type: 'keyDown', key: 'ArrowDown', code: 'ArrowDown' });
    await until(() => evaluate("document.querySelector('.panel').style.gridRow === '2 / span 6'"), 'keyboard movement');
    await click('Add view'); await input('#search-views', 'no-such-view');
    await until(async () => (await text()).includes('No views match'), 'empty search'); await screenshot('search-empty.png');
    await click('Clear search');
    await evaluate("document.querySelector('[aria-label=\"Close view picker\"]').click()");
    await until(() => evaluate("document.activeElement.id === 'add-view'"), 'picker focus restoration');
    await evaluate("document.querySelector('[aria-label=\"Remove Task runner\"]').click()");
    await until(() => evaluate("document.querySelectorAll('.panel').length === 0"), 'remove view');
    await click('Undo removal');
    await until(() => evaluate("document.querySelectorAll('.panel').length === 1"), 'undo view');
    await screenshot('editing.png');
    await click('Cancel');
    await until(() => evaluate("document.querySelector('#confirm-title')?.textContent === 'Discard unsaved changes?'"), 'discard confirmation');
    await evaluate("document.querySelector('dialog .hub-button').click()");
    assert.equal(await evaluate("!!document.querySelector('.dashboard-name')"), true);
    await click('Save dashboard');
    await until(async () => (await text()).includes('Dashboard saved.'), 'save');
    const ax = await cdp('Accessibility.getFullAXTree');
    const unnamedButtons=ax.nodes.filter(n=>!n.ignored && n.role?.value==='button' && !n.name?.value);
    assert.equal(unnamedButtons.length,0);
    await writeFile(path.join(output,'accessibility.json'), JSON.stringify(ax,null,2));
    const saved = await (await fetch(`${url}api/dashboards`)).json();
    assert.equal(saved[0].panels[0].w, 7); assert.equal(saved[0].panels[0].y, 1);
    await cdp('Page.reload'); await until(async () => (await text()).includes('Task runner'), 'reload saved');
    // Conflict retains the draft and offers recovery.
    await click('Edit dashboard'); await input('[aria-label="Dashboard name"]', 'Local changes');
    await fetch(`${url}api/dashboards/${dashboard.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ ...saved[0], name: 'Changed elsewhere' }) });
    await click('Save dashboard'); await until(async () => (await text()).includes('Your edits are kept'), 'conflict');
    assert.equal(await evaluate("document.querySelector('.dashboard-name').value"), 'Local changes');
    await screenshot('save-error.png');
    await click('Discard and reload'); await click('Discard changes');
    await until(async () => (await text()).includes('Changed elsewhere'), 'reload conflict');
    await click('Create dashboard');
    await input('.dashboard-name', 'Temporary dashboard');
    await click('Save dashboard');
    await until(async ()=>(await text()).includes('Dashboard saved.'),'create dashboard');
    await click('Delete dashboard');
    await until(()=>evaluate("document.querySelector('dialog[open] #confirm-title')?.textContent.includes('Temporary dashboard')"),'delete confirmation');
    await evaluate("document.querySelector('dialog .danger').focus()");
    await cdp('Input.dispatchKeyEvent', {type:'keyDown', key:'Enter', code:'Enter', windowsVirtualKeyCode:13, text:'\r', unmodifiedText:'\r'});
    await cdp('Input.dispatchKeyEvent', {type:'keyUp', key:'Enter', code:'Enter', windowsVirtualKeyCode:13});
    await until(async ()=>(await text()).includes('Dashboard deleted.'),'delete dashboard').catch(async error=>{ console.log(await text()); throw error; });
    await start('omochi-utils', ['button', '--service-id', 'second', '--runtime-dir', runtime, '--', '/bin/echo', 'second-action'], /Published service/);
    await click('Edit dashboard'); await click('Add view');
    await until(()=>evaluate("Array.from(document.querySelectorAll('.app-options button')).some(b=>!b.disabled)"),'second view');
    await evaluate("document.querySelector('.app-options button:not(:disabled)').click()");
    await until(()=>evaluate("document.querySelectorAll('.panel').length===2"),'added second view');
    await click('Save dashboard');
    await until(async () => (await text()).includes('Dashboard saved.'), 'save second view');
    await cdp('Emulation.setDeviceMetricsOverride', { width: 320, height: 844, deviceScaleFactor: 1, mobile: true });
    await until(() => evaluate("getComputedStyle(document.querySelector('.panel-grid')).display === 'flex'"), 'mobile layout');
    assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true);
    await screenshot('mobile.png');
    await click('Edit dashboard');
    assert.equal(await evaluate("document.querySelectorAll('.resize-edge').length"), 0);
    assert.equal((await text()).includes('Move up'), true);
    const firstId=await evaluate("document.querySelector('.panel').dataset.panelId");
    await click('Move down');
    await until(()=>evaluate("document.querySelector('.panel').dataset.panelId").then(id=>id!==firstId),'mobile reorder');
    await click('Save dashboard');
    await until(async () => (await text()).includes('Dashboard saved.'), 'save mobile order');
    await click('Board view'); await cdp('Page.reload');
    await until(async () => (await text()).includes('Exit board view'), 'direct board mode');
    assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true);
    await screenshot('mobile-board.png');
    await click('Exit board view');
    await cdp('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] });
    assert.equal(await evaluate("getComputedStyle(document.querySelector('.hub-button')).transitionDuration"), '0s');
    await cdp('Emulation.setEmulatedMedia', { features: [{ name: 'forced-colors', value: 'active' }] });
    await screenshot('forced-colors.png');
    await cdp('Emulation.setEmulatedMedia', { features: [] });
    await evaluate("document.documentElement.dir = 'rtl'");
    assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true);
    await screenshot('rtl.png');
    await evaluate("document.documentElement.dir = 'ltr'; document.documentElement.style.fontSize = '200%'");
    assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true);
    await screenshot('text-200.png');
    await click('Board view');
    assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true);
    await screenshot('board-text-200.png');
    utility.child.kill('SIGTERM');
    await until(async ()=>(await text()).includes('View unavailable'),'source app disconnected');
    await screenshot('disconnected.png');
    assert.deepEqual(errors, []);
    console.log(`Browser checks passed. Screenshots: ${output}`);
  }
} finally {
  socket?.close();
  for (const child of children.reverse()) {
    if (child.exitCode !== null || child.signalCode !== null) continue;
    const exited = once(child, 'exit'); child.kill('SIGTERM');
    await Promise.race([exited, delay(3000)]);
    if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
  }
  await rm(temporary, { recursive: true, force: true });
}
