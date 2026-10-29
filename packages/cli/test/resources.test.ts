import assert from 'node:assert/strict'
import { WebSocket } from 'ws'
import { actionMessage, opened, test, waitFor } from '@omochi-ui/core/testing'
import { App } from '@omochi-ui/sdk'
import { UnixSocketServerTransport } from '@omochi-ui/sdk/node'
import { createStaticLocator, startGateway } from '@omochi-ui/discovery'
import { tempRuntimeRoot } from '@omochi-ui/discovery/testing'
import { startHub } from '../src/hub/hub.js'

test('resources cross gateway and two hubs without entering A2UI snapshots', async t => {
  const root = await tempRuntimeRoot(t)
  const app = new App({})
  let holding = false
  let releaseHold!: () => void
  const hold = new Promise<void>(resolve => { releaseHold = resolve })
  app.action('run', async () => { holding = true; await hold })
  const content = Buffer.alloc(100_000, 123)
  const uri = app.resources.bytes(content, 'video/mp4')
  app.surface('main', ui => ui.video(uri))
  const transport = new UnixSocketServerTransport({ serviceId: 'media', runtimeDir: root })
  const running = app.start(transport)
  await transport.ready
  t.after(async () => { releaseHold(); await transport.close(); await running })
  const gateway = await startGateway({ root, nodeId: 'aaaaaaaaaaaaaaaa' })
  t.after(() => gateway.close())
  const hubs = await Promise.all([0, 1].map(() => startHub({ port: 0, autoGateway: false,
    gateways: [createStaticLocator([{ kind: 'unix', socketPath: gateway.socketPath }])] })))
  for (const hub of hubs) t.after(() => hub.close())
  await waitFor(async () => (await Promise.all(hubs.map(async hub => {
    const rows = await (await fetch(`${hub.httpUrl}/services`)).json() as Array<{ status: string }>
    return rows[0]?.status === 'connected'
  }))).every(Boolean))
  const socket = new WebSocket(hubs[0]!.url)
  const observed: Array<Record<string, unknown>> = []
  socket.on('message', data => observed.push(JSON.parse(data.toString())))
  t.after(() => socket.close())
  await opened(socket)
  socket.send(JSON.stringify(actionMessage('aaaaaaaaaaaaaaaa~media/main', {})))
  await waitFor(() => holding)
  const urls = hubs.map(hub => `${hub.httpUrl}/resources/aaaaaaaaaaaaaaaa~media/${uri.split('/').at(-1)}`)
  const results = await Promise.all(urls.map(url => fetch(url, { headers: { Range: 'bytes=23000-78000' } })))
  for (const result of results) {
    assert.equal(result.status, 206)
    assert.deepEqual(Buffer.from(await result.arrayBuffer()), content.subarray(23000, 78001))
  }
  assert.ok(observed.every(message => message.omochi === undefined), 'resource responses must not be broadcast')
  releaseHold()
  app.resources.release(uri)
  assert.equal((await fetch(urls[0]!)).status, 404)
  await gateway.close()
  assert.equal((await fetch(urls[0]!)).status, 503)
})

test('resource retrieval uses pinned mTLS and stops after trust forget', async t => {
  const { certificateFingerprint, loadNodeIdentity, openTrustStore, gatewayEndpointKey } = await import('@omochi-ui/discovery')
  const root = await tempRuntimeRoot(t), hubConfig = await tempRuntimeRoot(t), gatewayConfig = await tempRuntimeRoot(t)
  const app = new App({})
  const uri = app.resources.bytes(Buffer.from('private media'), 'audio/wav')
  app.surface('main', ui => ui.audioPlayer(uri))
  const transport = new UnixSocketServerTransport({ serviceId: 'media', runtimeDir: root })
  const running = app.start(transport); await transport.ready
  t.after(async () => { await transport.close(); await running })
  const identity = await loadNodeIdentity({ root: hubConfig }), remoteIdentity = await loadNodeIdentity({ root: gatewayConfig })
  const trust = await openTrustStore(hubConfig), remoteTrust = await openTrustStore(gatewayConfig)
  for (const [store, peer, role] of [[trust, remoteIdentity, 'gateway'], [remoteTrust, identity, 'hub']] as const) {
    await store.put({ nodeId: peer.nodeId, name: peer.name, certificate: peer.certificate, fingerprint: certificateFingerprint(peer.certificate), role, pairedAt: new Date().toISOString() })
  }
  const gateway = await startGateway({ root, identity: remoteIdentity, trust: remoteTrust, listen: { host: '127.0.0.1', port: 0 } })
  t.after(() => gateway.close())
  const endpoint = { kind: 'tcp' as const, ...gateway.address! }
  const hub = await startHub({ port: 0, autoGateway: false, identity, trust, configRoot: hubConfig,
    gateways: [createStaticLocator([endpoint], new Map([[gatewayEndpointKey(endpoint), remoteIdentity.nodeId]]))] })
  t.after(() => hub.close())
  await waitFor(async () => ((await (await fetch(`${hub.httpUrl}/services`)).json()) as Array<{ status: string }>)[0]?.status === 'connected')
  const url = `${hub.httpUrl}/resources/${remoteIdentity.nodeId}~media/${uri.split('/').at(-1)}`
  const response = await fetch(url); assert.equal(response.status, 200); assert.equal(await response.text(), 'private media')
  await trust.forget(remoteIdentity.nodeId)
  assert.equal((await fetch(url)).status, 503)
})
