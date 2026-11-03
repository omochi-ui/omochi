import { writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fixtureBoard, importBoardFile } from './boards-ui.mjs';

/** Real SDK bytes, production hosted source/renderer, and fault gates only at the test FFI boundary. */
export async function mediaChecks({ browser, host, fixture, until, passed, output }) {
  const nodes = fixture.ready.gateways.map(gateway => gateway.nodeId);
  const first = `${nodes[0]}~s/main/nested`, second = `${nodes[1]}~s/main/nested`;
  const panel = `Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(first)})`;
  const target = `${panel}?.querySelector('canvas[aria-label="SDK PNG case"]')`;
  const markdown = `${panel}?.querySelector('canvas[aria-label="SDK Markdown PNG"]')`;
  const button = (text, scope = 'document') => `Array.from((${scope})?.querySelectorAll('button')??[]).find(button=>button.textContent===${JSON.stringify(text)})`;
  const click = async (text, scope) => browser.evaluate(`${button(text, scope)}.click()`);
  const setInvitation = async text => browser.evaluate(`(()=>{const el=document.querySelector('#gateway-invitation');Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(el,${JSON.stringify(text)});el.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  const pixel = canvas => `(()=>{const canvas=${canvas};return canvas&&getComputedStyle(canvas).display!=='none'?Array.from(canvas.getContext('2d').getImageData(0,0,1,1).data):null;})()`;
  const rgba = async (canvas, expected) => until(async () => JSON.stringify(await browser.evaluate(pixel(canvas))) === JSON.stringify(expected), 'actual PNG canvas pixels');
  const forbidden = [];
  await browser.command('Network.enable');
  const unsubscribe = browser.onEvent(message => {
    if (message.method === 'Network.requestWillBeSent' && /^https:\/\/media\.invalid\//.test(message.params.request.url)) forbidden.push('external-media');
  });
  const fragment = '#connect:' + Buffer.from(JSON.stringify({ version: 1, invitation: fixture.ready.gateways[0].invitation })).toString('base64url');
  await browser.command('Page.navigate', { url: host.url + fragment });
  await until(async () => await browser.evaluate('!!document.querySelector("[data-browser-id]")'), 'hosted media profile');
  await importBoardFile(browser, output, fixtureBoard(fixture), until);
  await click('Accept or refresh invitation');
  await until(async () => await browser.evaluate(`!!${button('Connect', `document.querySelector('[data-gateway="${nodes[0]}"]')`)}`), 'media gateway saved');
  await click('Connect', `document.querySelector('[data-gateway="${nodes[0]}"]')`);
  await rgba(target, [255, 0, 0, 255]); await rgba(markdown, [0, 255, 0, 255]);
  passed('SDK RGBA PNG renders actual red canvas pixels'); passed('Markdown SDK RGB PNG renders actual green canvas pixels');
  await setInvitation(fixture.ready.gateways[1].invitation); await click('Accept or refresh invitation');
  await until(async () => await browser.evaluate(`!!${button('Connect', `document.querySelector('[data-gateway="${nodes[1]}"]')`)}`), 'second media gateway saved');
  await click('Connect', `document.querySelector('[data-gateway="${nodes[1]}"]')`);
  await until(async () => await browser.evaluate('document.querySelectorAll("[data-surface]").length===8'), 'two gateways for media isolation');
  await rgba(target, [255, 0, 0, 255]); await rgba(markdown, [0, 255, 0, 255]);
  passed('direct media never assigns external image/audio/video src', await browser.evaluate(`document.querySelectorAll('img,video,audio,source').length===0 && ${panel}.textContent.includes('Video unavailable in direct mode') && ${panel}.textContent.includes('Audio unavailable in direct mode')`));
  passed('Markdown keeps safe explicit user links', await browser.evaluate(`${panel}.querySelector('a').href==='https://example.com/'`));
  const screenshot = await browser.command('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true });
  await writeFile(path.join(output, 'media.png'), Buffer.from(screenshot.data, 'base64'));
  const cases = ['RGB green', 'wide multi-chunk PNG', 'bad signature', 'bad CRC', 'invalid IHDR', 'bad IDAT', 'inflation excess', 'inflation deficit', 'APNG', '16-bit PNG', 'side limit', 'chunk limit', 'encoded limit'];
  for (const name of cases) {
    await click('Next PNG case', panel);
    await until(async () => await browser.evaluate(`${panel}?.textContent.includes(${JSON.stringify(`PNG case: ${name}`)})`), 'SDK PNG case update');
    if (name === 'RGB green' || name === 'wide multi-chunk PNG') {
      await rgba(target, [0, 255, 0, 255]);
      if (name === 'wide multi-chunk PNG') passed('maximum-side PNG reads multiple exact sequential chunks', await browser.evaluate(`(${target}).width===8192`));
    }
    else await until(async () => await browser.evaluate(`Array.from((${panel})?.querySelectorAll('[aria-label="SDK PNG case"]')??[]).some(node=>node.tagName==='SPAN'&&node.textContent==='Image unavailable')`), `unavailable ${name}`);
    passed(`real PNG validation: ${name}`);
  }
  await click('Next PNG case', panel); await rgba(target, [255, 0, 0, 255]);
  passed('failed PNG validation releases capacity for later valid PNGs');

  await browser.evaluate(`(async()=>{
    const wasm=await import(new URL('./iroh/iroh.js',document.baseURI).href);
    const gates=new Map(), decoders=[];const counters={cancelledHolds:0,stat:0,read:0,lateClosed:0};
    window.__media={mode:'off',gates,decoders,counters,decode:false,releaseDecode(){for(const resolve of decoders.splice(0))resolve();}};
    const receive=wasm.RawStream.prototype.receive,cancel=wasm.RawStream.prototype.cancel,send=wasm.RawStream.prototype.send;
    wasm.RawStream.prototype.send=function(value){const request=JSON.parse(value);if(request.omochi==='resources/1'&&['stat','read'].includes(request.op))counters[request.op]++;return send.call(this,value);};
    wasm.RawStream.prototype.receive=function(){return receive.call(this).then(async value=>{if(!value)return value;const reply=JSON.parse(JSON.parse(value).json);const mode=reply.omochi==='resources/1'?(Object.hasOwn(reply,'size')?'stat':'read'):'other';
      if(window.__media.mode===mode)await new Promise((resolve,reject)=>gates.set(this,{resolve,reject}));return value;});};
    wasm.RawStream.prototype.cancel=function(){const gate=gates.get(this);if(gate){gates.delete(this);counters.cancelledHolds++;gate.reject('Cancelled');}return cancel.call(this);};
    const decode=window.createImageBitmap;const close=ImageBitmap.prototype.close;
    ImageBitmap.prototype.close=function(){counters.lateClosed++;return close.call(this);};
    window.createImageBitmap=async(...args)=>{const bitmap=await decode(...args);if(window.__media.decode)await new Promise(resolve=>decoders.push(resolve));return bitmap;};
  })()`);
  const retry = () => click('Retry connection', `document.querySelector('[data-gateway="${nodes[0]}"]')`);
  for (const mode of ['stat', 'read']) {
    await browser.evaluate(`window.__media.mode=${JSON.stringify(mode)}`); await retry();
    await until(async () => await browser.evaluate('window.__media.gates.size===2'), `held real resource ${mode} replies`);
    const before = await browser.evaluate('window.__media.counters.cancelledHolds');
    await browser.evaluate('window.__media.mode="off"'); await retry();
    await until(async () => await browser.evaluate(`window.__media.gates.size===0&&window.__media.counters.cancelledHolds>=${before + 2}`), 'captured old resource streams cancelled');
    await rgba(target, [255, 0, 0, 255]); await rgba(markdown, [0, 255, 0, 255]);
    passed(`lease replacement aborts ${mode} and permits a fresh scoped image`);
  }
  await browser.evaluate('window.__media.decode=true'); await retry();
  await until(async () => await browser.evaluate('window.__media.decoders.length===2'), 'real ImageBitmaps awaiting late completion');
  const before = await browser.evaluate('window.__media.counters.lateClosed');
  await browser.evaluate('window.__media.decode=false'); await retry();
  await until(async () => await browser.evaluate(`!!(${target})?.nextElementSibling?.querySelector('button')&&!!(${markdown})?.nextElementSibling?.querySelector('button')`), 'replacement image reads refused while old decoders remain active');
  await browser.evaluate('window.__media.releaseDecode()');
  await until(async () => await browser.evaluate(`window.__media.counters.lateClosed>=${before + 2}`), 'late bitmap disposal');
  for (const canvas of [target, markdown]) {
    await until(async () => await browser.evaluate(`!!(${canvas})?.nextElementSibling?.querySelector('button')`), 'explicit image retry after busy decode');
    await browser.evaluate(`(${canvas}).nextElementSibling.querySelector('button').click()`);
  }
  await rgba(target, [255, 0, 0, 255]); await rgba(markdown, [0, 255, 0, 255]);
  passed('late decoded bitmaps are closed and cannot populate a replacement lease');

  await browser.evaluate('window.__media.mode="stat"'); await retry();
  await until(async () => await browser.evaluate('window.__media.gates.size===2'), 'stat pending at revocation');
  const reads = await browser.evaluate('window.__media.counters.read');
  const identity = await browser.evaluate('document.querySelector("[data-browser-id]").dataset.browserId');
  fixture.send({ op: 'forget', gateway: 0, peer: identity });
  await fixture.until(() => fixture.messages.some(message => message.kind === 'forgotten' && message.gateway === 0), 'media revocation ACK');
  await until(async () => await browser.evaluate('document.querySelectorAll("[data-surface]").length===4&&window.__media.gates.size===0'), 'revoked media scope retired');
  passed('revoked stat responses cause no further chunk requests', await browser.evaluate(`window.__media.counters.read===${reads}`));
  await browser.evaluate(`Array.from(Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(second)}).querySelectorAll('button')).find(button=>button.textContent==='Increment 1').click()`);
  await until(async () => await browser.evaluate(`Array.from(document.querySelectorAll('[data-surface]')).find(node=>node.dataset.surface===${JSON.stringify(second)})?.textContent.includes('count: 1')`), 'B action after media revocation');
  passed('revoked A media leaves B actions usable');
  passed('unsupported direct media makes zero external HTTP requests', forbidden.length === 0); unsubscribe();
}
