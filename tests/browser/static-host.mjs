import path from 'node:path';
import { fileURLToPath } from 'node:url';

const contract = await Bun.file(
  fileURLToPath(new URL('../../apps/dashboard/headers.json', import.meta.url)),
).json();
/** Static HTTPS assets only: no API fallback, routing redirects or proxy. */
export function staticHost({ assets, cert, key, base = '/', gzip = false }) {
  const root = path.resolve(assets);
  if (!base.startsWith('/') || !base.endsWith('/'))
    throw new Error('Static base must start and end with /.');
  const requests = [];
  const server = Bun.serve({
    hostname: '127.0.0.1',
    port: 0,
    tls: { cert: Bun.file(cert), key: Bun.file(key) },
    async fetch(request) {
      const url = new URL(request.url);
      requests.push({
        method: request.method,
        path: url.pathname,
        ...(request.headers.get('X-Omochi-Asset-Audit') === '1'
          ? { audit: true }
          : {}),
      });
      if (
        !url.pathname.startsWith(base) ||
        !['GET', 'HEAD'].includes(request.method)
      )
        return new Response('Not found', {
          status: 404,
          headers: contract.headers,
        });
      let relative;
      try {
        relative = decodeURIComponent(
          url.pathname.slice(base.length) || 'index.html',
        );
      } catch {
        return new Response('Not found', {
          status: 404,
          headers: contract.headers,
        });
      }
      const file = path.resolve(root, relative);
      if (!file.startsWith(`${root}/`))
        return new Response('Not found', {
          status: 404,
          headers: contract.headers,
        });
      const asset = Bun.file(file);
      if (!(await asset.exists()))
        return new Response('Not found', {
          status: 404,
          headers: contract.headers,
        });
      const headers = {
        ...contract.headers,
        'Content-Type':
          contract.mime[path.extname(file)] ?? 'application/octet-stream',
        'Cache-Control': relative.startsWith('assets/')
          ? contract.cache.assets
          : relative.startsWith('iroh/')
            ? contract.cache.iroh
            : contract.cache.html,
      };
      let body = asset;
      if (
        gzip &&
        /\.(?:js|wasm|css)$/.test(relative) &&
        /\bgzip\b/.test(request.headers.get('Accept-Encoding') ?? '')
      ) {
        body = Bun.gzipSync(await asset.arrayBuffer());
        headers['Content-Encoding'] = 'gzip';
        headers.Vary = 'Accept-Encoding';
      }
      return new Response(request.method === 'HEAD' ? null : body, { headers });
    },
  });
  return {
    url: `https://127.0.0.1:${server.port}${base}`,
    requests,
    close: () => server.stop(true),
    contract,
  };
}
