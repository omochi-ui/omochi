import { describe, expect, test } from 'bun:test';
import worker from '../src/index.ts';
import contract from '../headers.json';

describe('dashboard asset boundary', () => {
  test('serves root, JS, CSS and WASM with policy headers, retaining asset validators', async () => {
    for (const [path, type, cache] of [
      ['/', 'text/html', 'no-cache'],
      ['/index.html', 'text/html', 'no-cache'],
      ['/assets/main-hash.js', 'text/javascript', contract.cache.assets],
      ['/assets/main-hash.css', 'text/css', contract.cache.assets],
      ['/iroh/iroh.js', 'text/javascript', 'no-cache'],
      ['/iroh/iroh_bg.wasm', 'application/wasm', 'no-cache'],
    ]) {
      const response = await worker.fetch(new Request(`https://app.omochi.fyi${path}`), {
        ASSETS: { async fetch(request) {
          expect(new URL(request.url).pathname).toBe(path === '/' ? '/index.html' : path);
          return new Response('asset', { headers: { ETag: 'fixture', 'Content-Encoding': 'gzip' } });
        } },
      });
      expect(response.status).toBe(200);
      for (const [name, value] of Object.entries(contract.headers)) expect(response.headers.get(name)).toBe(value);
      expect(response.headers.get('Cache-Control')).toBe(cache);
      expect(response.headers.get('Content-Type')).toBe(type);
      expect(response.headers.get('ETag')).toBe('fixture');
      expect(response.headers.get('Content-Encoding')).toBe('gzip');
    }
  });

  test('no API, websocket upgrade, unknown route, or write can reach the asset binding', async () => {
    for (const [path, method] of [
      ['/api/dashboards', 'GET'], ['/ws', 'GET'], ['/images/id', 'GET'],
      ['/resources/id', 'GET'], ['/missing.js', 'GET'], ['/other/', 'GET'],
      ['/', 'POST'], ['/assets/main.js', 'PUT'], ['/iroh/iroh.js', 'OPTIONS'],
    ]) {
      const response = await worker.fetch(new Request(`https://app.omochi.fyi${path}`, { method }), {
        ASSETS: { async fetch() { throw new Error('Unexpected asset read'); } },
      });
      expect(response.status).toBe(404);
      expect(response.headers.get('Content-Security-Policy')).toBe(contract.headers['Content-Security-Policy']);
    }
  });

  test('missing assets stay 404 and conditional/HEAD responses retain HTTP semantics', async () => {
    const missing = await worker.fetch(new Request('https://app.omochi.fyi/assets/missing.js'), {
      ASSETS: { async fetch() { return new Response('missing', { status: 404 }); } },
    });
    expect(missing.status).toBe(404);
    expect(missing.headers.get('Cache-Control')).toBe('no-cache');
    for (const status of [200, 304]) {
      const response = await worker.fetch(new Request('https://app.omochi.fyi/iroh/iroh.js', {
        method: 'HEAD', headers: { 'If-None-Match': 'fixture' },
      }), { ASSETS: { async fetch(request) {
        expect(request.method).toBe('HEAD');
        expect(request.headers.get('If-None-Match')).toBe('fixture');
        return new Response(null, { status, headers: { ETag: 'fixture' } });
      } } });
      expect(response.status).toBe(status);
      expect(await response.text()).toBe('');
      expect(response.headers.get('ETag')).toBe('fixture');
      expect(response.headers.get('Cache-Control')).toBe('no-cache');
    }
  });
});
