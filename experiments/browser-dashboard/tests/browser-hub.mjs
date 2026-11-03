import assert from 'node:assert/strict';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { chromium } from './cdp.mjs';
import { certificate, gatewayFixture } from './fixtures.mjs';
import { staticHost } from './static-host.mjs';
import { profileChecks } from './profile.mjs';
import { mediaChecks } from './media.mjs';
import { boardsChecks } from './boards.mjs';
import { hostedChecks } from './hosted.mjs';
import { loadChecks } from './load.mjs';
import { faultChecks } from './faults.mjs';
import { revocationChecks } from './revocation.mjs';

const args = process.argv.slice(2);
const modeIndex = args.indexOf('--mode');
const mode = modeIndex < 0 ? 'session' : args[modeIndex + 1];
if (!['session', 'profile', 'media', 'boards', 'hosted', 'load', 'faults', 'revocation', 'all'].includes(mode)) throw new Error('Use --mode session|profile|media|boards|hosted|load|faults|revocation|all.');
if (mode === 'all') {
  const output = path.resolve(process.env.BROWSER_HUB_RESULTS ?? 'test-results/browser-hub');
  await mkdir(output, { recursive: true, mode: 0o700 });
  const report = { mode, startedAt: new Date().toISOString(), modes: {}, errors: [] };
  try {
    for (const childMode of ['session', 'profile', 'media', 'boards', 'hosted', 'faults', 'revocation', 'load']) {
      const childOutput = path.join(output, childMode);
      const child = spawn(process.execPath, [fileURLToPath(import.meta.url), '--mode', childMode, ...(args.includes('--public-relay') ? ['--public-relay'] : [])],
        { stdio: 'inherit', env: { ...process.env, BROWSER_HUB_RESULTS: childOutput } });
      const status = await new Promise((resolve, reject) => { child.once('error', () => reject(new Error(`Could not start browser ${childMode} mode.`))); child.once('exit', resolve); });
      if (status !== 0) throw new Error(`Browser ${childMode} mode failed.`);
      report.modes[childMode] = await Bun.file(path.join(childOutput, 'result.json')).json();
    }
    console.log('All browser integration modes passed.');
  } catch (error) { report.errors.push(error.message); throw error; }
  finally { await writeFile(path.join(output, 'result.json'), JSON.stringify(report, null, 2)); }
  process.exit(0);
}
const assets = path.resolve(['session', 'load', 'faults', 'revocation'].includes(mode) ? 'experiments/browser-dashboard/app/dist-browser-test' : 'experiments/browser-dashboard/app/dist-hosted');
if (!await Bun.file(path.join(assets, 'index.html')).exists() || !await Bun.file(path.join(assets, 'iroh/iroh_bg.wasm')).exists()) throw new Error('Missing browser test assets. Run bun run experiment:browser-dashboard build.');
const temporary = await mkdtemp(path.join(tmpdir(), 'omochi-browser-hub-'));
const output = process.env.BROWSER_HUB_RESULTS ? path.resolve(process.env.BROWSER_HUB_RESULTS) : path.join(temporary, 'results');
await mkdir(output, { recursive: true, mode: 0o700 });
const report = { mode, startedAt: new Date().toISOString(), checks: {}, errors: [] };
let fixture, browser, host;
async function until(predicate, label, timeout = 45000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await Bun.sleep(30);
  }
  throw new Error(`Browser check timed out: ${label}`);
}
function passed(name, value = true) { assert(value, name); report.checks[name] = true; if (process.env.BROWSER_HUB_VERBOSE === '1') console.log(`Passed: ${name}`); }
try {
  const tls = await certificate(temporary);
  fixture = await gatewayFixture(temporary, tls, args.includes('--public-relay'), ['media', 'faults'].includes(mode), mode === 'load', mode === 'faults', mode === 'revocation');
  host = staticHost({ assets, ...tls, ...(mode === 'hosted' ? { base: '/dashboard/', gzip: true } : {}) });
  browser = await chromium(path.join(temporary, 'chrome'), [`--ignore-certificate-errors-spki-list=${tls.spki}`]);
  let pageExceptions = 0;
  browser.onEvent(message => { if (message.method === 'Runtime.exceptionThrown') pageExceptions++; });
  await browser.command('Page.enable'); await browser.command('Runtime.enable');
  if (mode !== 'session') {
    await ({ profile: profileChecks, media: mediaChecks, boards: boardsChecks, hosted: hostedChecks, load: loadChecks, faults: faultChecks, revocation: revocationChecks }[mode])({ browser, host, fixture, until, passed, output, temporary, report, assets, publicRelay: args.includes('--public-relay') });
    passed('no uncaught browser exceptions', pageExceptions === 0);
    passed('no static host application API calls', !host.requests.some(request => !request.audit && /^\/(?:dashboard\/)?(?:api|images|resources|events|services|ws)(?:\/|$)/.test(request.path)));
    console.log(`Browser ${mode} checks passed (${Object.keys(report.checks).length}).`);
  } else {
  await browser.command('Page.addScriptToEvaluateOnNewDocument', { source: `window.__BROWSER_FIXTURE__=${JSON.stringify({ gateways: fixture.ready.gateways })};` });
  await browser.command('Page.navigate', { url: host.url });
  await until(async () => {
    if (await browser.evaluate('!!window.__browserTestError')) throw new Error('Browser entry failed to enroll its memory identity.');
    return await browser.evaluate('window.__browserTest?.controller.getSnapshot().surfaceIds.length === 8');
  }, 'eight committed SDK Surfaces');
  passed('two authenticated gateways and eight services');
  const nodes = fixture.ready.gateways.map(gateway => gateway.nodeId);
  const first = `${nodes[0]}~s/main/nested`, second = `${nodes[1]}~s/main/nested`;
  const summary = await browser.evaluate('({ids:window.__browserTest.controller.getSnapshot().surfaceIds, endpoint:window.__browserTest.controller.getSnapshot().endpoint, status:window.__browserTest.controller.getSnapshot().status})');
  passed('full v4 namespaces with nested SDK IDs', summary.ids.includes(first) && summary.ids.includes(second) && nodes.every(id => /^[0-9a-f]{64}$/.test(id)));
  passed('direct renderer endpoint', summary.endpoint === 'browser:gateway' && summary.status === 'Connected');
  await until(async () => await browser.evaluate(`!!Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(first)})?.querySelector('input')`), 'bound form input');
  await browser.evaluate(`(() => {
    const panel=Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(first)});
    const input=panel.querySelector('input'); const setter=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set;
    setter.call(input,'unfinished draft'); input.dispatchEvent(new Event('input',{bubbles:true}));
    panel.querySelector('button').click();
  })()`);
  await until(async () => await browser.evaluate(`Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(first)})?.textContent.includes('count: 1')`), 'real SDK action update');
  passed('form action reaches the real SDK');
  passed('draft context reaches SDK', await browser.evaluate(`Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(first)}).textContent.includes('Received draft: unfinished draft')`));
  await fixture.until(() => fixture.messages.some(message => message.kind === 'action-observed' && message.gateway === 0), 'SDK client model observation');
  passed('raw form metadata is narrowed to the target SDK', fixture.messages.some(message => message.kind === 'action-observed' && message.gateway === 0 && message.service === 's' && message.scoped && message.bareTarget && message.draftPresent));
  await browser.evaluate(`window.__browserTest.source.retry({service:${JSON.stringify(`${nodes[0]}~s`)}})`);
  await until(async () => await browser.evaluate('window.__browserTest.controller.getSnapshot().surfaceIds.length === 8'), 'service Retry snapshot');
  passed('scoped service Retry');
  const otherLease = await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(second)}).token`);
  await browser.evaluate(`window.__browserTest.source.retry({gatewayId:${JSON.stringify(nodes[0])}})`);
  await until(async () => await browser.evaluate('window.__browserTest.controller.getSnapshot().surfaceIds.length === 8'), 'gateway Retry snapshots');
  passed('gateway Retry preserves B lease', await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(second)}).token`) === otherLease);
  const endpointId = await browser.evaluate('window.__browserTest.endpointId');
  fixture.send({ op: 'forget', gateway: 0, peer: endpointId });
  await fixture.until(() => fixture.messages.some(message => message.kind === 'forgotten' && message.gateway === 0), 'revocation ACK');
  await until(async () => await browser.evaluate('window.__browserTest.controller.getSnapshot().surfaceIds.length === 4'), 'A retracted while B lives');
  await browser.evaluate(`Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(second)}).querySelector('button').click()`);
  await until(async () => await browser.evaluate(`Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(second)}).textContent.includes('count: 1')`), 'B action after A revocation');
  passed('revocation A preserves live B actions');
  const screenshot = await browser.command('Page.captureScreenshot', { format: 'png' });
  await writeFile(path.join(output, 'session.png'), Buffer.from(screenshot.data, 'base64'));
  await browser.evaluate('window.__browserTest.close()');
  passed('orderly renderer and endpoint shutdown');
  passed('no static host application API calls', !host.requests.some(request => /^\/(?:api|images|resources|events|services|ws)(?:\/|$)/.test(request.path)));
  report.chromium = await browser.command('Browser.getVersion');
  report.gatewayIds = nodes; report.requests = host.requests;
  console.log(`Browser session checks passed (${Object.keys(report.checks).length}).`);
  }
} catch (error) {
  report.errors.push(error.message);
  await writeFile(path.join(output, 'result.json'), JSON.stringify(report, null, 2));
  throw error;
} finally {
  await writeFile(path.join(output, 'result.json'), JSON.stringify(report, null, 2));
  await browser?.close(); await host?.close(); await fixture?.close();
  await rm(temporary, { recursive: true, force: true });
}
