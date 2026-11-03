import assert from 'node:assert/strict'
import { test } from '@omochi-ui/core/testing'
import { resolveResourceUrl } from '../src/resources.js'

test('resolves resource URIs using the connection and service, without changing ordinary URLs', () => {
  const id = 'a'.repeat(32), uri = `omochi://resources/${id}`
  assert.equal(resolveResourceUrl(uri, 'ws://localhost:5555/ws', 'main'), `http://localhost:5555/resources/_/${id}`)
  const node = 'a'.repeat(64)
  assert.equal(resolveResourceUrl(uri, 'wss://host/ws', `${node}~media/main`), `https://host/resources/${node}~media/${id}`)
  assert.equal(resolveResourceUrl('https://example.com/photo.png', 'ws://localhost/ws', 'main'), 'https://example.com/photo.png')
  assert.equal(resolveResourceUrl('omochi://resources/../../secret', 'ws://localhost/ws', 'main'), '')
})

test('routes full iroh EndpointIds without truncating the resource namespace', () => {
  const id = 'b'.repeat(32), node = '0123456789abcdef'.repeat(4)
  assert.equal(resolveResourceUrl(`omochi://resources/${id}`, 'ws://localhost:5555/ws', `${node}~media/main`), `http://localhost:5555/resources/${node}~media/${id}`)
  for (const invalid of ['a'.repeat(16), node.slice(1), node + '0', 'g'.repeat(64)]) {
    assert.equal(resolveResourceUrl(`omochi://resources/${id}`, 'ws://localhost:5555/ws', `${invalid}~media/main`), `http://localhost:5555/resources/_/${id}`)
  }
})
