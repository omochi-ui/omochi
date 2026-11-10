import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fixtureBoard, importBoardFile } from './boards-ui.mjs';

/** Drives only the production UI/ProfileOwner; no test globals are shipped in hosted assets. */
export async function profileChecks({
  browser,
  host,
  fixture,
  until,
  passed,
  output,
}) {
  const nodes = fixture.ready.gateways.map((gateway) => gateway.nodeId);
  let page = browser;
  const freshInvite = async () => {
    const offset = fixture.messages.length;
    fixture.send({ op: 'invite', gateway: 0 });
    await fixture.until(
      () =>
        fixture.messages
          .slice(offset)
          .some((message) => message.kind === 'invitation'),
      'fresh same-source invitation',
    );
    return fixture.messages
      .slice(offset)
      .find((message) => message.kind === 'invitation').invitation;
  };
  const button = (text, scope = 'document') =>
    `Array.from((${scope})?.querySelectorAll('button')??[]).find(button=>button.textContent===${JSON.stringify(text)})`;
  const click = async (text, scope) => {
    await page.evaluate(`${button(text, scope)}.click()`);
  };
  const ready = async () =>
    until(
      async () =>
        await page.evaluate('!!document.querySelector("[data-browser-id]")'),
      'durable browser profile',
    );
  const setInvitation = async (raw) => {
    await page.evaluate(`(() => {const input=document.querySelector('#gateway-invitation'); const setter=Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set;
      setter.call(input,${JSON.stringify(raw)});input.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  };
  const records = () =>
    page.evaluate(`new Promise((resolve,reject)=>{const request=indexedDB.open('omochi.browser.v1',1);request.onerror=()=>reject();request.onsuccess=()=>{
    const db=request.result;const tx=db.transaction(['identity','gateways','preferences'],'readonly');const result={};
    for(const name of ['identity','gateways','preferences']){const get=tx.objectStore(name).getAll();get.onsuccess=()=>{result[name]=get.result.map(value=>name==='identity'?{endpointId:value.endpointId}:value)}}
    tx.oncomplete=()=>{db.close();resolve(result)};tx.onabort=()=>{db.close();reject()};}})`);
  const fragment =
    '#connect:' +
    Buffer.from(
      JSON.stringify({
        version: 1,
        invitation: fixture.ready.gateways[0].invitation,
      }),
    ).toString('base64url');
  await page.command('Page.navigate', { url: host.url + fragment });
  await ready();
  passed(
    'bootstrap bearer fragment removed before connection',
    await page.evaluate(
      'location.hash === "" && document.querySelectorAll("[data-gateway]").length === 0',
    ),
  );
  const identity = await page.evaluate(
    'document.querySelector("[data-browser-id]").dataset.browserId',
  );
  assert.match(identity, /^[0-9a-f]{64}$/);
  passed(
    'identity committed before explicit invitation redemption',
    (await records()).identity[0].endpointId === identity,
  );
  await importBoardFile(page, output, fixtureBoard(fixture), until);
  passed(
    'imported board does not select or connect any gateway',
    (await records()).gateways.length === 0 &&
      (await page.evaluate(
        'document.querySelectorAll("[data-surface]").length===0',
      )),
  );
  await click('Accept or refresh invitation');
  await until(
    async () =>
      await page.evaluate(
        `!!${button('Connect', `document.querySelector('[data-gateway="${nodes[0]}"]')`)}`,
      ),
    'first acceptance save',
  );
  passed(
    'new enrollment saved and left unselected',
    (await records()).gateways[0].enrollment === 'accepted' &&
      (await page.evaluate(
        'document.querySelectorAll("[data-surface]").length===0',
      )),
  );
  await click(
    'Connect',
    `document.querySelector('[data-gateway="${nodes[0]}"]')`,
  );
  await until(
    async () =>
      await page.evaluate(
        'document.querySelectorAll("[data-surface]").length===4',
      ),
    'real hosted renderer',
  );
  passed('explicit Connect mounts four real SDK Surfaces');
  await setInvitation(fixture.ready.gateways[1].invitation);
  await click('Accept or refresh invitation');
  await until(
    async () =>
      await page.evaluate(
        `!!${button('Connect', `document.querySelector('[data-gateway="${nodes[1]}"]')`)}`,
      ),
    'second acceptance save',
  );
  await click(
    'Connect',
    `document.querySelector('[data-gateway="${nodes[1]}"]')`,
  );
  await until(
    async () =>
      await page.evaluate(
        'document.querySelectorAll("[data-surface]").length===8',
      ),
    'eight hosted Surfaces',
  );
  passed('two selected durable gateways share the production session');
  passed(
    'live source names use authenticated hello',
    await page.evaluate(
      `document.querySelector('[data-reference^="${nodes[0]}"] .panel-heading small')?.textContent.startsWith('Fixture 0 /') && document.querySelector('[data-reference^="${nodes[1]}"] .panel-heading small')?.textContent.startsWith('Fixture 1 /')`,
    ),
  );

  await page.command('Page.reload');
  await ready();
  await until(
    async () =>
      await page.evaluate(
        'document.querySelectorAll("[data-surface]").length===8',
      ),
    'reload selected profiles',
  );
  passed(
    'reload restores the same identity and both selected connections',
    await page.evaluate(
      `document.querySelector('[data-browser-id]').dataset.browserId===${JSON.stringify(identity)}`,
    ),
  );
  const previousDocument = await page.evaluate('performance.timeOrigin');
  await page.evaluate(
    "window.dispatchEvent(new PageTransitionEvent('pageshow',{persisted:true}))",
  );
  await until(
    async () =>
      await page.evaluate(
        `performance.timeOrigin!==${previousDocument}&&document.querySelector('[data-browser-id]')?.dataset.browserId===${JSON.stringify(identity)}`,
      ),
    'fresh document after persisted pageshow',
  );
  await until(
    async () =>
      await page.evaluate(
        'document.querySelectorAll("[data-surface]").length===8',
      ),
    'fresh owner and snapshots after cached-document restoration',
  );
  passed(
    'persisted pageshow reboots storage and the owner before restoring selected sources',
  );
  await setInvitation(await freshInvite());
  await click('Accept or refresh invitation');
  await until(
    async () =>
      await page.evaluate(
        '!!document.querySelector("[data-invitation-state=refreshed]")',
      ),
    'accepted invitation ordinary refresh',
  );
  await until(
    async () =>
      await page.evaluate(
        'document.querySelectorAll("[data-surface]").length===8',
      ),
    'selected union restored after refresh',
  );
  passed('accepted link refresh preserves selected sources');
  const connectedImage = await page.command('Page.captureScreenshot', {
    format: 'png',
    captureBeyondViewport: true,
  });
  await writeFile(
    path.join(output, 'profile-connected.png'),
    Buffer.from(connectedImage.data, 'base64'),
  );
  fixture.send({ op: 'forget', gateway: 0, peer: identity });
  await fixture.until(
    () =>
      fixture.messages.some(
        (message) => message.kind === 'forgotten' && message.gateway === 0,
      ),
    'profile revocation ACK',
  );
  await until(
    async () =>
      await page.evaluate(
        'document.querySelectorAll("[data-surface]").length===4',
      ),
    'revoked source removed',
  );
  await setInvitation(await freshInvite());
  await click('Accept or refresh invitation');
  await until(
    async () =>
      await page.evaluate(
        '!!document.querySelector("[data-invitation-state=can-redeem]") && !!document.querySelector("[data-error-code=NotAuthorized]")',
      ),
    'ordinary refresh rejected without automatic redemption',
  );
  await until(
    async () =>
      await page.evaluate(
        'document.querySelectorAll("[data-surface]").length===4',
      ),
    'B restored after rejected refresh',
  );
  passed(
    'revoked accepted link requires explicit redemption',
    await page.evaluate(
      'document.querySelectorAll("[data-surface]").length===4',
    ),
  );
  await click('Accept gateway-wide access');
  await until(
    async () =>
      await page.evaluate(
        'document.querySelectorAll("[data-surface]").length===8',
      ),
    'explicit same-key redemption',
  );
  passed('explicit redemption reuses the durable browser identity');

  // This second tab uses the same real IDB/lock manager. It must never open a second endpoint.
  const second = await browser.tab();
  await second.command('Page.enable');
  await second.command('Runtime.enable');
  await second.command('Page.navigate', { url: host.url });
  await until(
    async () =>
      await second.evaluate(
        '!!document.querySelector("[data-error-code=OwnerBusy]")',
      ),
    'owner contention',
  );
  passed('second tab reports OwnerBusy');
  // Renderer-process termination exercises automatic lock release without relying on pagehide.
  await page.command('Page.crash', {}, 2000).catch(() => {});
  page = second;
  await click('Retry opening profile');
  await ready();
  await until(
    async () =>
      await page.evaluate(
        'document.querySelectorAll("[data-surface]").length===8',
      ),
    'owner crash takeover',
  );
  passed(
    'owner crash releases lock and retains durable identity',
    await page.evaluate(
      `document.querySelector('[data-browser-id]').dataset.browserId===${JSON.stringify(identity)}`,
    ),
  );

  // Fail a transaction after put's success event, before transaction.complete.
  await page.evaluate(`(() => {const original=IDBObjectStore.prototype.put;window.__restoreProfilePut=()=>{IDBObjectStore.prototype.put=original;delete window.__restoreProfilePut};
    IDBObjectStore.prototype.put=function(value,...rest){const request=original.call(this,value,...rest);if(this.name==='gateways'&&value.enrollment==='accepted')request.addEventListener('success',()=>this.transaction.abort(),{once:true});return request;};})()`);
  await setInvitation(fixture.ready.gateways[0].invitation);
  await click('Accept or refresh invitation');
  await until(
    async () =>
      await page.evaluate(
        `document.querySelector('[data-gateway="${nodes[0]}"]')?.dataset.recovery==='unsaved' && document.querySelectorAll('[data-surface]').length===0`,
      ),
    'accepted local commit abort',
  );
  passed('local commit abort publishes unsaved and suspends all connections');
  await page.evaluate('window.__restoreProfilePut()');
  await click(
    'Retry save',
    `document.querySelector('[data-gateway="${nodes[0]}"]')`,
  );
  await until(
    async () =>
      await page.evaluate(
        `document.querySelector('[data-gateway="${nodes[0]}"]')?.dataset.recovery==='none' && !!${button('Resume selected connections')}`,
      ),
    'local-only save retry',
  );
  passed(
    'save retry retains suspension until explicit resume',
    await page.evaluate(
      'document.querySelectorAll("[data-surface]").length===0',
    ),
  );
  await click('Resume selected connections');
  await until(
    async () =>
      await page.evaluate(
        'document.querySelectorAll("[data-surface]").length===8',
      ),
    'explicit resume after save',
  );

  // A corrupt identity cannot silently generate a replacement or bind any source.
  const boardId = crypto.randomUUID();
  await page.evaluate(`new Promise((resolve,reject)=>{const request=indexedDB.open('omochi.browser.v1',1);request.onsuccess=()=>{const db=request.result;const tx=db.transaction(['identity','boards','preferences'],'readwrite');
    const get=tx.objectStore('identity').get('default');get.onsuccess=()=>tx.objectStore('identity').put({...get.result,endpointId:'f'.repeat(64)});
    tx.objectStore('boards').put({id:${JSON.stringify(boardId)},retained:'unchanged'});
    const prefs=tx.objectStore('preferences').get('ui');prefs.onsuccess=()=>tx.objectStore('preferences').put({...prefs.result,selectedBoardId:${JSON.stringify(boardId)}});
    tx.oncomplete=()=>{db.close();resolve()};tx.onabort=()=>{db.close();reject()};}})`);
  await page.command('Page.reload');
  await until(
    async () =>
      await page.evaluate(
        'document.body.textContent.includes("Saved profile needs recovery")',
      ),
    'corrupt identity recovery',
  );
  passed(
    'corrupt identity requires explicit recovery',
    await page.evaluate(
      'document.querySelectorAll("[data-surface]").length===0 && document.body.textContent.includes("f".repeat(64))',
    ),
  );
  await click('Reset browser identity…');
  await click('Confirm identity reset');
  await ready();
  const reset = await records();
  passed(
    'reset generates a fresh identity and clears source trust locally',
    reset.identity[0].endpointId !== identity &&
      reset.gateways.length === 0 &&
      reset.preferences[0].selectedGateways.length === 0,
  );
  passed(
    'reset retains boards and their selection',
    reset.preferences[0].selectedBoardId === boardId &&
      (await page.evaluate(
        `new Promise(resolve=>{const request=indexedDB.open('omochi.browser.v1',1);request.onsuccess=()=>{const db=request.result;const tx=db.transaction('boards','readonly');const get=tx.objectStore('boards').get(${JSON.stringify(boardId)});get.onsuccess=()=>{resolve(get.result?.retained==='unchanged');db.close()}}})`,
      )),
  );
  // Discard a real accepted reply at the FFI boundary. Remote enrollment has happened,
  // but ProfileOwner must retain pending/unknown and must not infer ordinary access.
  await page.evaluate(`(async()=>{const wasm=await import(new URL('./iroh/iroh.js',document.baseURI).href);const original=wasm.RawStream.prototype.receive;let discard=true;
    wasm.RawStream.prototype.receive=function(...args){return original.apply(this,args).then(value=>{if(discard&&value&&JSON.parse(value).json&&JSON.parse(JSON.parse(value).json).accepted===true){discard=false;throw 'NetworkUnavailable'}return value;});};})()`);
  await setInvitation(await freshInvite());
  await click('Accept or refresh invitation');
  await until(
    async () =>
      await page.evaluate(
        `document.querySelector('[data-gateway="${nodes[0]}"]')?.dataset.recovery==='unknown'`,
      ),
    'lost accepted reply',
  );
  passed(
    'lost real acceptance stays pending and opens no ordinary services',
    (await records()).gateways[0].enrollment === 'pending' &&
      (await page.evaluate(
        'document.querySelectorAll("[data-surface]").length===0',
      )),
  );
  const newIdentity = reset.identity[0].endpointId;
  await page.command('Page.reload');
  await ready();
  passed(
    'reload retains same-key pending recovery without probing',
    await page.evaluate(
      `document.querySelector('[data-browser-id]').dataset.browserId===${JSON.stringify(newIdentity)} && document.querySelector('[data-gateway="${nodes[0]}"]').dataset.recovery==='pending'`,
    ),
  );
  await setInvitation(await freshInvite());
  await click('Accept or refresh invitation');
  await until(
    async () =>
      await page.evaluate(
        `!!${button('Connect', `document.querySelector('[data-gateway="${nodes[0]}"]')`)}`,
      ),
    'new invitation same-key recovery',
  );
  passed(
    'new invitation completes same-key enrollment',
    await page.evaluate(
      `document.querySelector('[data-browser-id]').dataset.browserId===${JSON.stringify(newIdentity)}`,
    ),
  );
  const screenshot = await page.command('Page.captureScreenshot', {
    format: 'png',
    captureBeyondViewport: true,
  });
  await writeFile(
    path.join(output, 'profile.png'),
    Buffer.from(screenshot.data, 'base64'),
  );
  passed(
    'hosted runtime exposes no fixture or raw profile globals',
    await page.evaluate(
      'typeof window.__browserTest === "undefined" && typeof window.__BROWSER_FIXTURE__ === "undefined"',
    ),
  );
  await page.evaluate(
    `new Promise((resolve,reject)=>{const request=indexedDB.open('omochi.browser.v1',2);request.onsuccess=()=>{request.result.close();resolve()};request.onerror=()=>reject();})`,
  );
  await page.command('Page.reload');
  await until(
    async () =>
      await page.evaluate(
        '!!document.querySelector("[data-error-code=StorageVersion]")',
      ),
    'unsupported storage version',
  );
  passed('unsupported DB version rejects without identity migration');
  await second.close();
  const denied = await browser.tab();
  await denied.command('Page.enable');
  await denied.command('Runtime.enable');
  await denied.command('Page.addScriptToEvaluateOnNewDocument', {
    source: `IDBFactory.prototype.open=function(){throw new DOMException('denied','SecurityError')}`,
  });
  await denied.command('Page.navigate', { url: host.url });
  await until(
    async () =>
      await denied.evaluate(
        '!!document.querySelector("[data-error-code=StorageUnavailable]")',
      ),
    'denied storage',
  );
  passed('denied real browser storage fails closed');
  await denied.close();
}
