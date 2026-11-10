import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';

/** The production native remote listener, with only the SDK Unix bytes held by a transparent proxy. */
export async function revocationChecks({
  browser,
  host,
  fixture,
  until,
  passed,
  output,
}) {
  await browser.command('Page.addScriptToEvaluateOnNewDocument', {
    source: `window.__BROWSER_FIXTURE__=${JSON.stringify({ gateways: fixture.ready.gateways })};window.__revocationRejected=0;addEventListener('unhandledrejection',()=>window.__revocationRejected++);`,
  });
  await browser.command('Page.navigate', { url: host.url });
  await until(
    async () =>
      await browser.evaluate(
        'window.__browserTest?.controller.getSnapshot().surfaceIds.length===8',
      ),
    'native gateway SDK proxy initial snapshots',
  );
  const a = fixture.ready.gateways[0].nodeId,
    b = fixture.ready.gateways[1].nodeId;
  const first = `${a}~s/main/nested`,
    second = `${b}~s/main/nested`;
  const identity = await browser.evaluate('window.__browserTest.endpointId');
  const lease = await browser.evaluate(
    `window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(first)}).token`,
  );
  const bLease = await browser.evaluate(
    `window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(second)}).token`,
  );
  const send = (tag, padding) => {
    const message = {
      version: 'v0.9',
      action: {
        surfaceId: first,
        name: 'increment',
        sourceComponentId: 'button',
        timestamp: new Date().toISOString(),
        context: { input: { draft: tag, tag, padding: '' } },
      },
    };
    return browser.evaluate(
      `(async()=>{const message=${JSON.stringify(message)};message.action.context.input.padding='x'.repeat(${padding});try{await window.__browserTest.source.action(${JSON.stringify(lease)},message);return {ok:true}}catch(error){return {ok:false,code:error.code}}})()`,
    );
  };
  fixture.send({ op: 'sdk-arm' });
  await fixture.until(
    () => fixture.messages.some((message) => message.kind === 'sdk-armed'),
    'SDK gate armed',
  );
  assert.deepEqual(await send('dispatched', 2 * 1024 * 1024), { ok: true });
  await fixture.until(
    () => fixture.messages.some((message) => message.kind === 'sdk-write-held'),
    'actual partial shared SDK write',
  );
  passed('real remote SDK command is dispatched before the Unix writer stalls');
  assert.deepEqual(await send('revoked', 0), { ok: true });
  fixture.send({ op: 'local-action' });
  await fixture.until(
    () => fixture.messages.some((message) => message.kind === 'local-enqueued'),
    'Local command queued behind remote work',
  );
  fixture.send({ op: 'forget', gateway: 0, peer: identity });
  await until(
    async () => {
      const offset = fixture.messages.length;
      fixture.send({ op: 'trust-status', peer: identity });
      await fixture.until(
        () =>
          fixture.messages
            .slice(offset)
            .some((message) => message.kind === 'trust-status'),
        'durable revocation read',
      );
      return fixture.messages
        .slice(offset)
        .some(
          (message) => message.kind === 'trust-status' && !message.registered,
        );
    },
    'durable trust removed while shared SDK write remains blocked',
    3000,
  );
  // Allow the real authorization poll to reach its drain barrier. This is shorter than
  // both the five-second SDK dispatch bound and the live acknowledgement deadline.
  await Bun.sleep(200);
  passed(
    'trust forget cannot acknowledge an in-flight SDK dispatch',
    !fixture.messages.some(
      (message) =>
        message.kind === 'forgotten' || message.kind === 'forget-failed',
    ),
  );
  fixture.send({ op: 'sdk-release' });
  await fixture.until(
    () => fixture.messages.some((message) => message.kind === 'forgotten'),
    'live revocation ACK after shared write joins',
    5000,
  );
  await fixture.until(
    () =>
      fixture.messages.some(
        (message) =>
          message.kind === 'action-observed' && message.tag === 'local',
      ),
    'Local command survives revoked remote queue',
    5000,
  );
  const observed = fixture.messages
    .filter(
      (message) =>
        message.kind === 'action-observed' &&
        message.gateway === 0 &&
        message.tag,
    )
    .map((message) => message.tag);
  assert.deepEqual(observed, ['dispatched', 'local']);
  passed(
    'only already-dispatched remote work and Local work reach the real SDK after revocation',
  );
  await until(
    async () =>
      await browser.evaluate(
        'window.__browserTest.controller.getSnapshot().surfaceIds.length===4',
      ),
    'A removed while native B lives',
  );
  assert.equal(
    await browser.evaluate(
      `window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(second)}).token`,
    ),
    bLease,
  );
  assert.equal(
    await browser.evaluate(
      `(async()=>{try{await window.__browserTest.source.action(${JSON.stringify(lease)},{version:'v0.9',action:{surfaceId:${JSON.stringify(first)},name:'increment'}});return false}catch(error){return error.code==='ScopeExpired'}})()`,
    ),
    true,
  );
  await browser.evaluate(
    `Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(second)}).querySelector('button').click()`,
  );
  await until(
    async () =>
      await browser.evaluate(
        `Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(second)})?.textContent.includes('count: 1')`,
      ),
    'B remains actionable after blocked A revocation',
  );
  passed(
    'A revocation expires old actions without replay and preserves B lease and actions',
  );
  passed(
    'no unhandled rejections while native revocation joins',
    await browser.evaluate('window.__revocationRejected===0'),
  );
  const screenshot = await browser.command('Page.captureScreenshot', {
    format: 'png',
  });
  await writeFile(
    path.join(output, 'revocation.png'),
    Buffer.from(screenshot.data, 'base64'),
  );
  await browser.evaluate('window.__browserTest.close()');
  const stopped = await browser.evaluate('window.__browserTest.diagnostics()');
  for (const name of [
    'frameBytes',
    'snapshotBytes',
    'deliveryBytes',
    'renderBytes',
    'actionBytes',
    'leases',
    'leaseBytes',
    'activeReads',
    'encodedBytes',
  ])
    assert.equal(stopped.session[name], 0);
  passed('blocked-write revocation session closes with zero retained budgets');
}
