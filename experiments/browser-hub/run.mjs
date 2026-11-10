// Run with Bun after building the fixture, WASM wrapper, and static browser app.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, rm, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { gzipSync } from 'node:zlib';

const base = fileURLToPath(new URL('.', import.meta.url));
const results = path.join(base, 'results');
await mkdir(results, { recursive: true });
const temporary = await mkdtemp(path.join(tmpdir(), 'obh-browser-'));
const children = [];
let socket, server, evaluate;
const report = {
  startedAt: new Date().toISOString(),
  checks: {},
  errors: [],
  requests: [],
  console: [],
};
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function until(check, description, timeout = 45000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await check()) return;
    await delay(100);
  }
  throw new Error(`Timed out: ${description}`);
}
const check = (name, value) => {
  assert(value, name);
  report.checks[name] = true;
};

try {
  const fixture = spawn(
    path.join(base, 'target/debug/omochi-browser-hub-fixture'),
    [],
    {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, OMOCHI_BROWSER_SPIKE_TMPDIR: temporary },
    },
  );
  children.push(fixture);
  let fixtureError = '',
    ready;
  const messages = [];
  let buffer = '';
  fixture.stderr.on('data', (data) => {
    fixtureError += data;
  });
  fixture.stdout.on('data', (data) => {
    buffer += data;
    let index;
    while ((index = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, index);
      buffer = buffer.slice(index + 1);
      const value = JSON.parse(line);
      if (value.kind === 'ready') ready = value;
      else messages.push(value);
    }
  });
  await until(() => {
    if (fixture.exitCode !== null)
      throw new Error(`Fixture exited: ${fixtureError}`);
    return ready;
  }, 'gateway fixture and TLS relay readiness');
  report.gatewayId = ready.gatewayId;
  report.relay = ready.relay;

  // A static file host only. No native hub, WebSocket upgrade, or app-data API.
  const assets = path.join(base, 'dist');
  server = Bun.serve({
    hostname: '127.0.0.1',
    port: 0,
    async fetch(request) {
      const url = new URL(request.url);
      const pathname = decodeURIComponent(
        url.pathname === '/' ? '/index.html' : url.pathname,
      );
      const filename = path.resolve(assets, `.${pathname}`);
      if (!filename.startsWith(`${assets}/`))
        return new Response('Forbidden', { status: 403 });
      const file = Bun.file(filename);
      if (!(await file.exists()))
        return new Response('Not found', { status: 404 });
      return new Response(file);
    },
  });
  const origin = `http://127.0.0.1:${server.port}`;
  report.pageOrigin = origin;

  const chrome = spawn(
    process.env.CHROMIUM_BIN ?? 'chromium',
    [
      '--headless',
      '--no-sandbox',
      '--disable-gpu',
      '--disable-dev-shm-usage',
      '--no-first-run',
      '--no-default-browser-check',
      '--remote-debugging-port=0',
      `--user-data-dir=${temporary}/chrome`,
      'about:blank',
    ],
    { stdio: ['ignore', 'ignore', 'pipe'] },
  );
  children.push(chrome);
  let debug;
  chrome.stderr.on('data', (data) => {
    const match = data.toString().match(/DevTools listening on (ws:\/\/\S+)/);
    if (match) debug = match[1];
  });
  await until(() => debug, 'Chromium DevTools');
  const targets = await (
    await fetch(`http://${new URL(debug).host}/json/list`)
  ).json();
  socket = new WebSocket(
    targets.find((target) => target.type === 'page').webSocketDebuggerUrl,
  );
  await new Promise((resolve, reject) => {
    socket.onopen = resolve;
    socket.onerror = reject;
  });
  let sequence = 0;
  const pending = new Map();
  socket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.id) {
      const job = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) job.reject(new Error(JSON.stringify(message.error)));
      else job.resolve(message.result);
    } else if (message.method === 'Runtime.exceptionThrown') {
      report.errors.push(message.params.exceptionDetails.text);
    } else if (message.method === 'Runtime.consoleAPICalled') {
      report.console.push({
        type: message.params.type,
        text: message.params.args
          .map((arg) => arg.value ?? arg.description ?? '')
          .join(' '),
      });
    } else if (message.method === 'Network.requestWillBeSent') {
      const url = new URL(message.params.request.url);
      // Deliberately omit fragments and query strings, including bootstrap secrets.
      report.requests.push({
        type: message.params.type,
        url: `${url.origin}${url.pathname}`,
      });
    } else if (message.method === 'Network.webSocketCreated') {
      const url = new URL(message.params.url);
      report.requests.push({
        type: 'WebSocket',
        url: `${url.origin}${url.pathname}`,
      });
    }
  };
  const cdp = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = ++sequence;
      pending.set(id, { resolve, reject });
      socket.send(JSON.stringify({ id, method, params }));
    });
  evaluate = async (expression) => {
    const response = await cdp('Runtime.evaluate', {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (response.exceptionDetails)
      throw new Error(JSON.stringify(response.exceptionDetails));
    return response.result.value;
  };
  const evidence = () => evaluate('window.spike?.evidence ?? {}');
  const body = () => evaluate('document.body.innerText');
  const healthy = async () => {
    const state = await evidence();
    if (state.error) throw new Error(state.error);
    return state;
  };
  await cdp('Runtime.enable');
  await cdp('Page.enable');
  await cdp('Network.enable');
  report.browser = await cdp('Browser.getVersion');
  await cdp('Emulation.setDeviceMetricsOverride', {
    width: 1000,
    height: 800,
    deviceScaleFactor: 1,
    mobile: false,
  });
  const payload = Buffer.from(JSON.stringify(ready)).toString('base64url');
  await cdp('Page.navigate', { url: `${origin}/#connect:${payload}` });
  await until(
    async () =>
      (await healthy()).snapshotEnd &&
      (await body()).includes('Browser gateway count: 0'),
    'enrollment and live A2UI Surface',
  );
  check(
    'unauthorizedIdentityRejected',
    (await evidence()).unauthorizedRejected,
  );
  check('enrollmentAccepted', (await evidence()).enrolled);
  check('exactGatewayAuthenticated', (await evidence()).gatewayAuthenticated);
  check('relayOnlyPath', (await evidence()).relayOnly);
  check('invitationRemovedFromUrl', await evaluate('location.hash === ""'));
  check(
    'noInvitationPersisted',
    await evaluate(
      'Object.values(localStorage).every(value => !value.includes("omochi://invite/") && !value.includes("\\\"secret\\\""))',
    ),
  );
  await until(
    () =>
      evaluate('document.querySelector("canvas")?.style.display === "block"'),
    'SDK image drawn by existing renderer',
  );
  check(
    'sdkResourceImageDecoded',
    await evaluate(
      'document.querySelector("canvas").width === 1 && document.querySelector("canvas").height === 1',
    ),
  );
  check(
    'sdkResourceImagePixels',
    await evaluate(
      'Array.from(document.querySelector("canvas").getContext("2d").getImageData(0,0,1,1).data).some(value => value !== 0)',
    ),
  );
  await evaluate(
    'Array.from(document.querySelectorAll("button")).find(button => button.textContent.trim() === "Increment").click()',
  );
  await until(async () => {
    await healthy();
    return (await body()).includes('Browser gateway count: 1');
  }, 'real SDK action and resulting update');
  check('actionUpdatedSdkState', true);
  await until(async () => (await healthy()).heartbeatPong, 'gateway heartbeat');
  check('controlHeartbeat', true);
  report.firstSession = await evidence();
  const browserId = report.firstSession.browserId;
  await evaluate('window.spike.reconnect()');
  await until(
    async () =>
      (await healthy()).reconnected &&
      (await body()).includes('Browser gateway count: 1') &&
      (await evaluate(
        'document.querySelector("canvas")?.style.display === "block"',
      )),
    'fresh snapshot after connection replacement',
  );
  check('reconnectFreshSnapshot', true);
  const screenshot = await cdp('Page.captureScreenshot', { format: 'png' });
  await writeFile(
    path.join(results, 'browser.png'),
    Buffer.from(screenshot.data, 'base64'),
  );
  await cdp('Page.reload', { ignoreCache: true });
  await until(
    async () =>
      (await healthy()).restoredIdentity &&
      (await healthy()).snapshotEnd &&
      (await body()).includes('Browser gateway count: 1'),
    'reload with persistent browser identity and no invitation',
  );
  check('reloadWithoutInvitation', (await evidence()).browserId === browserId);
  report.restoredSession = await evidence();
  fixture.stdin.write(`${JSON.stringify({ op: 'forget', peer: browserId })}\n`);
  await until(
    () => messages.some((message) => message.kind === 'forgotten'),
    'gateway revocation acknowledgement',
  );
  await until(
    async () => (await evidence()).connectionClosed,
    'revoked browser connection closed',
  );
  check('gatewayRevocationClosesBrowser', true);
  report.revocation = await evidence();
  check(
    'noHubApplicationHttpRequests',
    report.requests
      .filter((request) => request.url.startsWith(origin))
      .every(
        (request) =>
          !/^\/(api|images|resources|services|events|ws)(\/|$)/.test(
            new URL(request.url).pathname,
          ),
      ),
  );
  check(
    'relayWebSocketObserved',
    report.requests.some(
      (request) =>
        request.type === 'WebSocket' && request.url.startsWith('wss://'),
    ),
  );
  check('noBrowserExceptions', report.errors.length === 0);
  const wasm = Bun.file(
    path.join(base, 'web/pkg/omochi_browser_hub_spike_bg.wasm'),
  );
  report.wasmBytes = (await stat(wasm.name)).size;
  report.wasmGzipBytes = gzipSync(await wasm.arrayBuffer()).length;
  report.result = 'PASS';
  console.log(
    JSON.stringify(
      {
        result: report.result,
        checks: report.checks,
        relay: report.relay,
        wasmBytes: report.wasmBytes,
        wasmGzipBytes: report.wasmGzipBytes,
      },
      null,
      2,
    ),
  );
  fixture.stdin.write(`${JSON.stringify({ op: 'stop' })}\n`);
  await until(
    () => fixture.exitCode !== null,
    'fixture graceful shutdown',
    10000,
  );
  assert.equal(fixture.exitCode, 0, fixtureError);
} catch (error) {
  report.result = 'FAIL';
  report.failure = String(error);
  if (socket?.readyState === WebSocket.OPEN) {
    // Final browser diagnostics are sanitized by the page's evidence interface.
    try {
      report.finalEvidence = await evaluate('window.spike?.evidence ?? {}');
      report.finalBody = await evaluate('document.body.innerText');
    } catch {}
  }
  console.error(JSON.stringify(report, null, 2));
  process.exitCode = 1;
} finally {
  report.finishedAt = new Date().toISOString();
  await writeFile(
    path.join(results, 'result.json'),
    JSON.stringify(report, null, 2),
  );
  socket?.close();
  server?.stop(true);
  for (const child of children.reverse())
    if (child.exitCode === null) child.kill('SIGTERM');
  await delay(200);
  for (const child of children)
    if (child.exitCode === null) child.kill('SIGKILL');
  await rm(temporary, { recursive: true, force: true });
}
