// An uncaught request callback error must fail this process, not just one request.
import assert from 'node:assert/strict'
import { once } from 'node:events'
import { createServer } from 'node:http'
import { createConnection } from 'node:net'
import { createServicesEndpoints, serveResources } from '@omochi-ui/core'

const services = [{ id: 'test', name: 'Test', status: 'connected' }]
const endpoints = createServicesEndpoints({ list: () => services })
const resource = 'a'.repeat(32)
const content = 'media'
let fetches = 0
const fetchResource = async (route, request) => {
  fetches++
  assert.equal(route, 'node/service')
  assert.equal(request.resource, resource)
  return request.op === 'stat'
    ? { size: content.length, mime: 'video/mp4', etag: `"${resource}"` }
    : { data: Buffer.from(content).toString('base64') }
}
const server = createServer((request, response) => {
  const handled = process.argv[2] === 'resources'
    ? serveResources(request, response, fetchResource)
    : endpoints.handleRequest(request, response)
  if (!handled) response.writeHead(404).end('fallback')
})

function request(target, origin) {
  return new Promise((resolve, reject) => {
    const socket = createConnection({ host: '127.0.0.1', port: server.address().port })
    let response = ''
    socket.on('data', data => { response += data })
    socket.once('error', reject)
    socket.once('close', () => resolve(response))
    socket.once('connect', () => socket.end([
      `GET ${target} HTTP/1.1`, 'Host: localhost', 'Connection: close',
      ...(origin ? [`Origin: ${origin}`] : []), '', '',
    ].join('\r\n')))
  })
}

try {
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  for (const target of ['//[', 'http://[', '//localhost:65536', '//%zz']) {
    for (const origin of [undefined, 'https://untrusted.example']) {
      assert.match(await request(target, origin), /^HTTP\/1\.1 400 /, target)
    }
  }
  if (process.argv[2] === 'resources') {
    for (const route of ['%', '%GG', '%C3%28']) {
      assert.match(await request(`/resources/${route}/${resource}`), /^HTTP\/1\.1 400 /)
    }
    assert.equal(fetches, 0, 'invalid routes must never reach the resource provider')
    const target = `/resources/node%2Fservice/${resource}?download=1`
    assert.match(await request(target, 'https://untrusted.example'), /^HTTP\/1\.1 403 /)
    const response = await request(target, 'http://localhost:1234')
    assert.match(response, /^HTTP\/1\.1 200 /)
    assert.ok(response.endsWith(`\r\n\r\n${content}`))
    assert.equal(fetches, 2)
  } else {
    assert.match(await request('/services', 'https://untrusted.example'), /^HTTP\/1\.1 403 /)
    const response = await request('/services?detail=1', 'http://localhost:1234')
    assert.match(response, /^HTTP\/1\.1 200 /)
    assert.ok(response.includes(JSON.stringify(services)))
  }
  const fallback = await request('/unrelated/%?query=%')
  assert.match(fallback, /^HTTP\/1\.1 404 /)
  assert.ok(fallback.includes('fallback'), 'unmatched paths still return false')
} finally {
  endpoints.close()
  await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()))
}
console.log('request targets isolated')
