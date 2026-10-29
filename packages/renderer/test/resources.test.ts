import assert from 'node:assert/strict'
import { test } from '@omochi-ui/core/testing'
import { resolveResourceUrl } from '../src/resources.js'

test('resolves resource URIs using the connection and service, without changing ordinary URLs', () => {
  const id = 'a'.repeat(32), uri = `omochi://resources/${id}`
  assert.equal(resolveResourceUrl(uri, 'ws://localhost:5555/ws', 'main'), `http://localhost:5555/resources/_/${id}`)
  assert.equal(resolveResourceUrl(uri, 'wss://host/ws', 'aaaaaaaaaaaaaaaa~media/main'), `https://host/resources/aaaaaaaaaaaaaaaa~media/${id}`)
  assert.equal(resolveResourceUrl('https://example.com/photo.png', 'ws://localhost/ws', 'main'), 'https://example.com/photo.png')
  assert.equal(resolveResourceUrl('omochi://resources/../../secret', 'ws://localhost/ws', 'main'), '')
})
