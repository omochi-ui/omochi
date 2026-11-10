import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';

/** Full ten-minute workload: actual SDK reconciliation, gateway frames and installed renderer. */
export async function loadChecks({
  browser,
  host,
  fixture,
  until,
  passed,
  output,
  report,
}) {
  await browser.command('Page.addScriptToEvaluateOnNewDocument', {
    source: `window.__BROWSER_FIXTURE__=${JSON.stringify({ gateways: fixture.ready.gateways })};window.__loadRejected=0;addEventListener('unhandledrejection',()=>window.__loadRejected++);`,
  });
  await browser.command('Page.navigate', { url: host.url });
  await until(async () => {
    if (await browser.evaluate('!!window.__browserTestError'))
      throw new Error('Load entry failed to enroll its memory identity.');
    return await browser.evaluate(
      'window.__browserTest?.controller.getSnapshot().surfaceIds.length===8',
    );
  }, 'load eight initial SDK snapshots');
  await until(
    async () =>
      await browser.evaluate(
        'document.querySelectorAll("canvas").length===2 && Array.from(document.querySelectorAll("canvas")).every(canvas=>canvas.width===1)',
      ),
    'load initial PNG pair',
  );
  passed('two gateways eight services and two decoded PNG readers');
  const initial = await browser.evaluate('window.__browserTest.diagnostics()');
  const memorySamples = [
    { elapsedMilliseconds: 0, ...(await browser.memoryUsage()) },
  ];
  const bounds = {
    frameBytes: 8 * 1024 * 1024,
    snapshotBytes: 32 * 1024 * 1024,
    peakSnapshotBytes: 32 * 1024 * 1024,
    largestServiceSnapshot: 16 * 1024 * 1024,
    deliveryBytes: 16 * 1024 * 1024,
    peakDeliveryBytes: 16 * 1024 * 1024,
    renderBytes: 16 * 1024 * 1024,
    peakRenderBytes: 16 * 1024 * 1024,
    renderCount: 32,
    actionBytes: 8 * 1024 * 1024,
    peakActionBytes: 8 * 1024 * 1024,
    actionCount: 8,
    leases: 4096,
    leaseBytes: 8 * 1024 * 1024,
    activeReads: 2,
    encodedBytes: 16 * 1024 * 1024,
    peakEncodedBytes: 16 * 1024 * 1024,
    peakReads: 2,
  };
  const maxima = {},
    imageMaxima = {};
  let previous = 0,
    samples = 0;
  fixture.send({ op: 'load' });
  await fixture.until(
    () => fixture.messages.some((message) => message.kind === 'load-started'),
    'load start',
  );
  const started = Date.now();
  while (Date.now() - started < 620000) {
    const sample = await browser.evaluate(
      `(()=>{const diagnostics=window.__browserTest.diagnostics();return {...diagnostics,rejected:window.__loadRejected,errors:window.__browserTest.errors.length,surfaces:window.__browserTest.controller.getSnapshot().surfaceIds.length,dom:Array.from(document.querySelectorAll('[data-surface]')).map(node=>({id:node.dataset.surface,sequence:Number(node.textContent.match(/LOAD (\\d{5}) /)?.[1])}))}})()`,
    );
    samples++;
    assert.equal(sample.rejected, 0, 'No rejected promises');
    assert.equal(sample.errors, 0, 'No source failures under load');
    assert.equal(sample.surfaces, 8);
    assert.equal(
      sample.session.leases,
      initial.session.leases,
      'Retained leases must remain constant',
    );
    assert.equal(
      sample.images.scopes,
      initial.images.scopes,
      'Retained renderer scopes must remain constant',
    );
    assert.equal(
      sample.load.invalid,
      0,
      'Payloads must be exact 1 KiB and ordered per service',
    );
    assert(sample.load.updates >= previous);
    previous = sample.load.updates;
    for (const [name, bound] of Object.entries(bounds)) {
      const value = sample.session[name];
      assert(
        Number.isSafeInteger(value) && value >= 0 && value <= bound,
        `Session budget exceeded: ${name}`,
      );
      maxima[name] = Math.max(maxima[name] ?? 0, value);
    }
    for (const [name, bound] of Object.entries({
      scopes: 8,
      loading: 2,
      decodedBytes: 32 * 1024 * 1024,
    })) {
      const value = sample.images[name];
      assert(value >= 0 && value <= bound, `Renderer budget exceeded: ${name}`);
      imageMaxima[name] = Math.max(imageMaxima[name] ?? 0, value);
    }
    if (samples % 30 === 0) {
      memorySamples.push({
        elapsedMilliseconds: Date.now() - started,
        ...(await browser.memoryUsage()),
      });
      console.log(
        `Load: ${Math.floor((Date.now() - started) / 1000)}s, ${sample.load.updates}/10000 updates.`,
      );
    }
    if (
      fixture.messages.some((message) => message.kind === 'load-queued') &&
      sample.load.updates === 10000
    )
      break;
    await Bun.sleep(1000);
  }
  const elapsed = Date.now() - started;
  passed(
    'ten full minutes of real incremental SDK traffic',
    elapsed >= 599000 &&
      fixture.messages.some((message) => message.kind === 'load-queued'),
  );
  const final = await browser.evaluate('window.__browserTest.diagnostics()');
  assert.equal(final.load.updates, 10000);
  assert.equal(final.load.last.length, 8);
  passed(
    'all 10000 exact 1 KiB updates applied in order across eight services',
  );
  await fixture.until(
    () =>
      fixture.messages.some(
        (message) =>
          message.kind === 'load-progress' && message.emitted === 10000,
      ),
    'all SDK updates emitted',
  );
  const expected = new Map(
    fixture.ready.gateways.flatMap((gateway, node) =>
      [1, 16, 17, 64].map((length, index) => [
        `${gateway.nodeId}~${'s'.repeat(length)}`,
        9993 + node * 4 + index,
      ]),
    ),
  );
  for (const [service, sequence] of final.load.last)
    assert.equal(sequence, expected.get(service));
  await until(
    async () =>
      await browser.evaluate(
        `Array.from(document.querySelectorAll('[data-surface]')).every(node=>{const text=node.textContent.match(/LOAD (\\d{5}) /)?.[1];return Number(text)===Object.fromEntries(${JSON.stringify([...expected].map(([key, value]) => [`${key}/main/nested`, value]))})[node.dataset.surface]})`,
      ),
    'latest load values on real DOM',
  );
  passed('final rendered values match every service without stale renders');
  passed(
    'PNG transfers repeat with two simultaneous admitted readers',
    final.session.readsStarted >= 100 && final.session.peakReads === 2,
  );
  passed(
    'all sampled counters and reservation high-water marks stay within budgets',
  );
  report.load = {
    elapsedMilliseconds: elapsed,
    samples,
    updates: final.load.updates,
    pngReads: final.session.readsStarted,
    maxima,
    imageMaxima,
    memorySamples,
  };
  report.chromium = await browser.command('Browser.getVersion');
  report.memory = await browser.command('Runtime.getHeapUsage');
  const screenshot = await browser.command('Page.captureScreenshot', {
    format: 'png',
  });
  await writeFile(
    path.join(output, 'load.png'),
    Buffer.from(screenshot.data, 'base64'),
  );
  await browser.evaluate('window.__browserTest.close()');
  const stopped = await browser.evaluate('window.__browserTest.diagnostics()');
  for (const name of [
    'frameBytes',
    'snapshotBytes',
    'deliveryBytes',
    'renderBytes',
    'renderCount',
    'actionBytes',
    'actionCount',
    'leases',
    'leaseBytes',
    'activeReads',
    'encodedBytes',
  ])
    assert.equal(stopped.session[name], 0, `Stopped budget retained: ${name}`);
  assert.deepEqual(stopped.images, { scopes: 0, loading: 0, decodedBytes: 0 });
  passed(
    'all retained session and renderer budgets return to zero after joined stop',
  );
  report.load.stopped = stopped;
}
