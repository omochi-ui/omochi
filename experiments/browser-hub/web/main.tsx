import React from '../../../packages/hub-ui/node_modules/react';
import { createRoot } from '../../../packages/hub-ui/node_modules/react-dom/client.js';
import { MessageProcessor } from '../../../packages/renderer/node_modules/@a2ui/web_core/src/v0_9/index.js';
import { SurfaceView } from '../../../packages/renderer/src/surface';
import { resourceCatalog } from '../../../packages/renderer/src/resources';
import { BrowserImages } from '../../../packages/renderer/src/images';
import { createActionMessage } from '../../../packages/renderer/src/client';
import '../../../packages/renderer/src/styles.css';
import { parseInvitation } from '../../browser-dashboard/hub/src/bootstrap';
import init, { BrowserEndpoint } from './pkg/omochi_browser_hub_spike.js';

const root = createRoot(document.getElementById('root')!);
const status = (value: string) => { document.getElementById('status')!.textContent = value; };
const evidence: Record<string, any> = { events: [], frames: 0, resources: 0, actions: 0 };
const record = (name: string, value: unknown = true) => {
  evidence[name] = value;
  evidence.events.push({ name, at: Math.round(performance.now()) });
};
const deadline = <T,>(promise: Promise<T>, ms = 20000): Promise<T> => Promise.race([
  promise, new Promise<never>((_, reject) => setTimeout(() => reject(new Error('operation timed out')), ms)),
]);
const send = (stream: any, value: unknown) => deadline(stream.send(JSON.stringify(value)));
const recv = async (stream: any, timeout = 20000) => {
  const raw = await deadline(stream.recv(), timeout);
  if (raw === undefined) throw new Error('stream ended');
  return JSON.parse(raw);
};

class DirectImages extends BrowserImages {
  constructor(private readonly client: Client) { super('ws://unused.invalid', () => {}); }
  override async fetch(_handle: string, uri: string, signal: AbortSignal): Promise<Response> {
    try { return await this.readImage(uri, signal); }
    catch (error) { record('imageError', String(error)); throw error; }
  }
  private async readImage(uri: string, signal: AbortSignal): Promise<Response> {
    signal.throwIfAborted();
    const id = /^omochi:\/\/resources\/([a-f0-9]{32})$/.exec(uri)?.[1];
    if (!id) throw new Error('Spike supports only SDK resource images');
    const metadata = await this.client.resource({ op: 'stat', resource: id });
    record('imageMetadata', { status: metadata.status, size: metadata.size, mime: metadata.mime });
    if (metadata.status !== 200 || metadata.mime !== 'image/png' || metadata.size > 8 * 1024 * 1024) throw new Error('unsupported image');
    const bytes = new Uint8Array(metadata.size);
    for (let offset = 0; offset < bytes.length;) {
      signal.throwIfAborted();
      const length = Math.min(24576, bytes.length - offset);
      const part = await this.client.resource({ op: 'read', resource: id, offset, length });
      if (part.status !== 200) throw new Error('resource read rejected');
      const decoded = Uint8Array.from(atob(part.data), c => c.charCodeAt(0));
      if (decoded.length !== length) throw new Error('incorrect resource chunk size');
      bytes.set(decoded, offset); offset += length;
    }
    signal.throwIfAborted();
    // PNG-only adapter for this experiment; reuse existing renderer decode/pixel budgets.
    const view = new DataView(bytes.buffer);
    if (view.getUint32(0) !== 0x89504e47 || view.getUint32(4) !== 0x0d0a1a0a) throw new Error('invalid PNG');
    record('resourceImageBytes', bytes.length);
    return new Response(bytes, { headers: {
      'content-type': metadata.mime,
      'x-omochi-image-width': String(view.getUint32(16)),
      'x-omochi-image-height': String(view.getUint32(20)),
    } });
  }
}

class Client {
  endpoint: any;
  connection: any;
  service: any;
  serviceStream: any;
  processor: any;
  images: DirectImages;
  surface = '';
  heartbeat: any;
  request = 0;
  epoch = 0;
  target: any;
  constructor() { this.images = new DirectImages(this); }
  async initialize(target: any) {
    this.target = target;
    this.endpoint = await BrowserEndpoint.create(localStorage.getItem('spike-key') ?? undefined, target.relay);
    localStorage.setItem('spike-key', this.endpoint.secret());
    record('browserId', this.endpoint.id());
    if (localStorage.getItem(`gateway:${target.gatewayId}`)) {
      record('restoredIdentity');
    } else {
      // Actual gateway must reject this identity before enrollment.
      const rejected = await deadline(this.endpoint.connect(target.gatewayId, target.relay, 'omochi/gateway/4'));
      try {
        const stream = await rejected.open();
        await send(stream, { kind: 'control', protocol: 4 });
        await recv(stream);
        throw new Error('gateway leaked control data before enrollment');
      } catch (error) {
        if (String(error).includes('leaked') || String(error).includes('timed out')) throw error;
        record('unauthorizedRejectionReason', await deadline(rejected.closed(), 5000));
        record('unauthorizedRejected');
      } finally { rejected.close(); }
      const invite = parseInvitation(target.invitation);
      const pairing = await deadline(this.endpoint.connect(target.gatewayId, target.relay, 'omochi/pair/1'));
      const stream = await pairing.open();
      await send(stream, { version: 4, secret: invite.secret, name: 'Chromium browser hub spike' });
      if (!(await recv(stream)).accepted) throw new Error('enrollment rejected');
      localStorage.setItem(`gateway:${target.gatewayId}`, JSON.stringify({ gatewayId: target.gatewayId, relay: target.relay }));
      pairing.close();
      record('enrolled');
    }
    await this.connect();
  }
  async connect() {
    const epoch = ++this.epoch;
    this.connection = await deadline(this.endpoint.connect(this.target.gatewayId, this.target.relay, 'omochi/gateway/4'));
    if (this.connection.remote_id() !== this.target.gatewayId) throw new Error('wrong authenticated gateway');
    record('gatewayAuthenticated');
    record('relayOnly', this.connection.relay_only());
    const control = await this.connection.open();
    await send(control, { kind: 'control', protocol: 4 });
    const hello = await recv(control);
    if (hello.kind !== 'hello' || hello.protocol !== 4 || hello.nodeId !== this.target.gatewayId) throw new Error('invalid hello');
    const roster = await recv(control);
    if (roster.kind !== 'services') throw new Error('invalid service list');
    this.service = roster.services.find((service: any) => service.id === 'browser-spike');
    if (!this.service) throw new Error('fixture service missing');
    record('serviceDiscovered');
    let ping = 0;
    this.heartbeat = setInterval(() => send(control, { kind: 'ping', id: ++ping }).catch(() => {}), 5000);
    void (async () => {
      try { while (epoch === this.epoch) {
        const message = await recv(control);
        if (message.kind === 'pong') record('heartbeatPong');
      } } catch { /* connection.closed records the authenticated session loss */ }
    })();
    void this.connection.closed().then((reason: string) => {
      if (epoch !== this.epoch) return;
      clearInterval(this.heartbeat); this.images.close(); record('connectionClosed', reason);
      status('Gateway connection closed');
    });
    this.images = new DirectImages(this);
    this.processor = new MessageProcessor([resourceCatalog], (action: any) => {
      const local = { ...action, surfaceId: action.surfaceId.slice(action.surfaceId.indexOf('/') + 1) };
      const message = createActionMessage(local, null);
      void send(this.serviceStream, message).then(() => { evidence.actions++; }).catch(fail);
    });
    this.serviceStream = await this.connection.open();
    await send(this.serviceStream, { kind: 'service', service: this.service.id, generation: this.service.generation });
    void this.consume(epoch);
  }
  async consume(epoch: number) {
    try {
      while (epoch === this.epoch) {
        const frame = await recv(this.serviceStream, 120000);
        if (frame.kind === 'snapshotStart') { record('snapshotStart'); continue; }
        if (frame.kind === 'snapshotEnd') { record('snapshotEnd'); status('Connected directly to gateway'); continue; }
        if (frame.kind !== 'message') throw new Error('invalid service frame');
        const message = structuredClone(frame.message);
        const kind = ['createSurface', 'updateComponents', 'updateDataModel', 'deleteSurface'].find(k => message[k]);
        if (!kind) throw new Error('invalid A2UI message');
        const localId = message[kind].surfaceId;
        this.surface = `${this.target.gatewayId}~${this.service.id}/${localId}`;
        message[kind].surfaceId = this.surface;
        if (kind === 'createSurface') this.images.update(this.surface, `epoch-${epoch}`);
        if (kind === 'deleteSurface') this.images.update(this.surface, null);
        this.processor.processMessages([message]); evidence.frames++;
        root.render(<SurfaceView session={{ processor: this.processor, surfaceIds: [this.surface], status: 'Connected', error: null, images: this.images, endpoint: 'ws://unused.invalid', resourceBaseUrl: undefined }} surfaceId={this.surface} />);
      }
    } catch (error) { if (epoch === this.epoch && !evidence.connectionClosed) fail(error); }
  }
  async resource(request: any) {
    record('resourceRequested', request.op);
    const stream = await this.connection.open();
    await send(stream, { kind: 'resource', service: this.service.id, generation: this.service.generation });
    await send(stream, { ...request, omochi: 'resources/1', kind: 'request', id: `browser_${++this.request}` });
    const result = await recv(stream); evidence.resources++;
    return result;
  }
  async reconnect() {
    this.epoch++; clearInterval(this.heartbeat); this.images.close(); this.connection.close();
    await this.connect(); record('reconnected');
  }
}

function fail(error: unknown) { evidence.error = String(error); status(`Failed: ${error}`); console.error(error); }
const client = new Client();
Object.assign(window, { spike: { evidence, reconnect: () => client.reconnect(), browserId: () => client.endpoint.id() } });
try {
  const fragment = location.hash.slice('#connect:'.length);
  let target;
  if (location.hash.startsWith('#connect:')) {
    target = JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(fragment.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0))));
    history.replaceState(null, '', location.pathname);
    // Persist only addressing information, never the invitation secret.
    localStorage.setItem('spike-target', JSON.stringify({ gatewayId: target.gatewayId, relay: target.relay }));
  } else target = JSON.parse(localStorage.getItem('spike-target') ?? 'null');
  if (!target) throw new Error('provide #connect: bootstrap payload');
  await init(); record('wasmReady');
  await client.initialize(target);
} catch (error) { fail(error); }
