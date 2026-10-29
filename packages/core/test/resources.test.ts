import assert from 'node:assert/strict'
import { test } from '../src/testing.js'
import { ResourceClient, ResourceError, resourceHello, type ResourceFrame } from '../src/resources.js'

test('resource brokers isolate responses, bound concurrency and cancel disconnected work', async () => {
  const sent: ResourceFrame[] = []
  const a = new ResourceClient(frame => sent.push(frame)), b = new ResourceClient(() => {})
  await assert.rejects(a.request({ op: 'stat', resource: 'a'.repeat(32) }), (e: ResourceError) => e.status === 501)
  a.receive(resourceHello()); b.receive(resourceHello())
  const controller = new AbortController()
  const request = a.request({ op: 'stat', resource: 'a'.repeat(32) }, controller.signal)
  b.receive({ ...sent[0], kind: 'response', status: 200 })
  controller.abort()
  await assert.rejects(request, (e: ResourceError) => e.status === 499)
  const pending = Array.from({ length: 8 }, () => a.request({ op: 'stat', resource: 'a'.repeat(32) }).catch(error => error))
  await assert.rejects(a.request({ op: 'stat', resource: 'a'.repeat(32) }), (e: ResourceError) => e.status === 503)
  a.close()
  for (const error of await Promise.all(pending)) assert.equal(error.status, 503)
  assert.equal(a.supported, false)
})
