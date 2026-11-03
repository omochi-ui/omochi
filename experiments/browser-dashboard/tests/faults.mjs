import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';

/** Real QUIC framing faults from an explicitly adversarial fixture peer, with a normal native B gateway. */
export async function faultChecks({ browser, host, fixture, until, passed, output, report }) {
  await browser.command('Page.addScriptToEvaluateOnNewDocument', { source: `window.__BROWSER_FIXTURE__=${JSON.stringify({ gateways: fixture.ready.gateways })};window.__faultRejected=0;addEventListener('unhandledrejection',()=>window.__faultRejected++);` });
  await browser.command('Page.navigate', { url: host.url });
  const a = fixture.ready.gateways[0].nodeId, b = fixture.ready.gateways[1].nodeId;
  const service = `${a}~s`, first = `${service}/main/nested`, second = `${b}~s/main/nested`;
  const ready = () => until(async () => await browser.evaluate('window.__browserTest?.controller.getSnapshot().surfaceIds.length===8'), 'fault peer and native B ready');
  await ready();
  const bLease = await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(second)}).token`);
  await browser.evaluate(`window.__faultHistory=[];window.__browserTest.source.subscribe(event=>{if(event.kind==='services'){const view=event.services.find(view=>view.id===${JSON.stringify(service)});if(view?.detail)window.__faultHistory.push(view.detail)}})`);
  const configure = async kind => {
    const offset = fixture.messages.length; fixture.send({ op: 'fault', kind });
    await fixture.until(() => fixture.messages.slice(offset).some(message => message.kind === 'fault-configured'), 'protocol fault configuration');
    return offset;
  };
  const retry = () => browser.evaluate(`window.__browserTest.source.retry({service:${JSON.stringify(service)}})`);
  const retryGateway = () => browser.evaluate(`window.__browserTest.source.retry({gatewayId:${JSON.stringify(a)}})`);
  const bUsable = async () => {
    const count = await browser.evaluate(`Number(Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(second)}).textContent.match(/count: ([0-9]+)/)[1])`);
    await browser.evaluate(`Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(second)}).querySelector('button').click()`);
    await until(async () => await browser.evaluate(`Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(second)})?.textContent.includes('count: ${count+1}')`), 'B action during A fault');
    assert.equal(await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(second)}).token`), bLease);
  };
  report.faults = [];
  for (const kind of ['partial-header', 'partial-body', 'silent-snapshot', 'stalled-snapshot', 'interrupted-snapshot']) {
    await browser.evaluate('window.__faultHistory=[]');
    const oldLease = await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(first)}).token`);
    // Hold the public action lease ID, rather than replaying a queued action after reconnect.
    const offset = await configure(kind); const started = Date.now(); await retry();
    await fixture.until(() => fixture.messages.slice(offset).some(message => message.kind === 'fault-applied' && message.fault === kind), 'native protocol fault reached stream');
    assert.equal(await browser.evaluate(`window.__browserTest.controller.getSnapshot().surfaceIds.includes(${JSON.stringify(first)})`), false, 'Uncommitted snapshot must not become visible');
    assert.equal(await browser.evaluate(`(async()=>{try{await window.__browserTest.source.action(${JSON.stringify(oldLease)},{version:'v0.9',action:{surfaceId:${JSON.stringify(first)},name:'increment'}});return false}catch(error){return error.code==='ScopeExpired'}})()`), true);
    await bUsable();
    if (kind !== 'interrupted-snapshot') {
      const lower = kind.startsWith('partial-') ? 4500 : kind === 'silent-snapshot' ? 9500 : 29500;
      await until(async () => await browser.evaluate('window.__faultHistory.some(detail=>/timed out/i.test(detail))'), 'real read/snapshot timeout with healthy control', 40000);
      assert(Date.now() - started >= lower, 'Deadline must begin at the specified boundary');
    }
    await configure('release'); await ready();
    assert.notEqual(await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(first)}).token`), oldLease);
    await bUsable();
    const diagnostics = await browser.evaluate('window.__browserTest.diagnostics()');
    assert.equal(diagnostics.session.leases, 8); assert.equal(diagnostics.session.frameBytes, 0);
    report.faults.push({ kind, recoveredMilliseconds: Date.now() - started });
    passed(`real QUIC ${kind} retracts only A service and releases frame ownership`);
  }
  for (const kind of ['duplicate-roster', 'unknown-control', 'pong-mismatch']) {
    const offset = await configure(kind); await retryGateway();
    await fixture.until(() => fixture.messages.slice(offset).some(message => message.kind === 'fault-applied' && message.fault === kind), 'control fault reached stream');
    await until(async () => await browser.evaluate('window.__browserTest.controller.getSnapshot().surfaceIds.length===4'), 'invalid A control retracts all A services', 40000);
    await bUsable();
    await retryGateway(); await ready();
    passed(`real QUIC ${kind} fails A control while native B remains live`);
  }
  await browser.evaluate(`window.__deletedA=null;(()=>{let previous=window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(first)}).token;window.__browserTest.controller.subscribe(()=>{const current=window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(first)})?.token;if(current)previous=current;else if(previous)window.__deletedA=previous})})()`);
  const deletionOffset = await configure('delete-recreate');
  await retry();
  await fixture.until(() => fixture.messages.slice(deletionOffset).some(message => message.kind === 'fault-held'), 'delete/recreate gate');
  await until(async () => await browser.evaluate(`!window.__browserTest.controller.getSnapshot().surfaceIds.includes(${JSON.stringify(first)}) && window.__browserTest.controller.getSnapshot().surfaceIds.length===7`), 'same-ID surface deletion');
  await bUsable();
  const deletedLease = await browser.evaluate('window.__deletedA');
  await configure('release'); await ready();
  assert(deletedLease);
  assert.notEqual(await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(first)}).token`), deletedLease);
  passed('same-ID delete and recreate replaces only its surface lease');

  const oldA = await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(first)}).token`);
  const otherA = `${a}~${'s'.repeat(16)}/main/nested`;
  const otherLease = await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(otherA)}).token`);
  await browser.evaluate(`(()=>{const processor=window.__browserTest.controller.getSnapshot().processor;const original=processor.processMessages;let fail=true;processor.processMessages=function(messages){if(fail&&messages.some(message=>message.createSurface?.surfaceId===${JSON.stringify(first)})){fail=false;original.call(this,[messages.find(message=>message.createSurface)]);throw new Error('fixture partial apply')}return original.call(this,messages)};window.__restoreProcessor=()=>{processor.processMessages=original};window.__throwingObservers=[window.__browserTest.source.subscribe(()=>{throw new Error('fixture observer')}),window.__browserTest.source.subscribe(async()=>{throw new Error('fixture async observer')})]})()`);
  await retry();
  await until(async () => await browser.evaluate(`window.__browserTest.controller.getSnapshot().services.find(service=>service.id===${JSON.stringify(service)})?.status==='error'`), 'authoritative installed processor failure');
  assert.equal(await browser.evaluate(`window.__browserTest.controller.getSnapshot().services.find(service=>service.id===${JSON.stringify(service)}).detail`), 'This source could not be rendered.');
  assert.equal(await browser.evaluate(`window.__browserTest.controller.getSnapshot().surfaceIds.includes(${JSON.stringify(first)})`), false);
  assert.equal(await browser.evaluate(`window.__browserTest.controller.getSnapshot().processor.model.surfacesMap.has(${JSON.stringify(first)})`), false);
  assert.equal(await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(otherA)}).token`), otherLease);
  await bUsable();
  await browser.evaluate('window.__restoreProcessor();for(const unsubscribe of window.__throwingObservers)unsubscribe()');
  await retry(); await ready();
  assert.notEqual(await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(first)}).token`), oldA);
  passed('failed authoritative apply retracts partially created A surface while other services remain live');
  passed('independent synchronous and asynchronous observer throws cannot interrupt source dispatch');

  await configure('generation-replace'); await retryGateway(); await ready();
  const beforeGeneration = await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(first)}).token`);
  const siblingGeneration = await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(otherA)}).token`);
  await configure('release');
  await until(async () => await browser.evaluate(`!!window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(first)}) && window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(first)}).token!==${JSON.stringify(beforeGeneration)}`), 'same-connection generation replacement');
  await ready();
  assert.equal(await browser.evaluate(`window.__browserTest.controller.getSnapshot().images.scope(${JSON.stringify(otherA)}).token`), siblingGeneration);
  await bUsable();
  passed('roster generation replacement retires only the captured service instance');
  await configure('generation-max'); const offset = fixture.messages.length; await retryGateway(); await ready();
  await fixture.until(() => fixture.messages.slice(offset).some(message => message.kind === 'maximum-generation-open' && !message.resource), 'exact max-u64 native open header');
  const target = `Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(first)})`;
  await browser.evaluate(`${target}.querySelector('button').click()`);
  await until(async () => await browser.evaluate(`${target}?.textContent.includes('count: 1')`), 'max-u64 action reaches actual SDK');
  passed('maximum u64 generation survives actual QUIC roster open and SDK action without rounding');
  await fixture.until(() => fixture.messages.slice(offset).some(message => message.kind === 'maximum-generation-open' && message.resource), 'exact max-u64 resource open header');
  await until(async () => await browser.evaluate(`Array.from((${target}).querySelectorAll('canvas')).some(canvas=>canvas.getAttribute('aria-label')==='SDK PNG case'&&canvas.width===1&&canvas.getContext('2d').getImageData(0,0,1,1).data[0]===255)`), 'maximum-generation actual PNG pixels');
  passed('maximum u64 generation also scopes real PNG stat and chunk streams');
  await bUsable();
  passed('no rejected promises under real framing faults', await browser.evaluate('window.__faultRejected===0'));
  const screenshot = await browser.command('Page.captureScreenshot', { format: 'png' });
  await writeFile(path.join(output, 'faults.png'), Buffer.from(screenshot.data, 'base64'));
  await browser.evaluate('window.__browserTest.close()');
  const stopped = await browser.evaluate('window.__browserTest.diagnostics()');
  for (const name of ['frameBytes','snapshotBytes','deliveryBytes','renderBytes','actionBytes','leases','leaseBytes','activeReads','encodedBytes']) assert.equal(stopped.session[name], 0);
  passed('protocol fault session joins to zero retained bytes and leases');
}
