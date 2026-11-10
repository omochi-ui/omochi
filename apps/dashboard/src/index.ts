import contract from '../headers.json';

interface Env {
  ASSETS: { fetch(request: Request): Promise<Response> };
}

/** Static client only. App data and authorization stay on the native hub. */
export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    const path = url.pathname;
    const allowed =
      path === '/' ||
      path === '/index.html' ||
      path.startsWith('/assets/') ||
      path.startsWith('/iroh/');
    if (!allowed || !['GET', 'HEAD'].includes(request.method)) {
      return new Response(request.method === 'HEAD' ? null : 'Not found', {
        status: 404,
        headers: {
          ...contract.headers,
          'Cache-Control': 'no-cache',
          'Content-Type': 'text/plain; charset=utf-8',
        },
      });
    }

    // Disable implicit HTML routing in the asset binding; only / maps to index.
    if (path === '/') url.pathname = '/index.html';
    const asset = await env.ASSETS.fetch(new Request(url, request));
    const headers = new Headers(asset.headers);
    for (const [name, value] of Object.entries(contract.headers))
      headers.set(name, value);
    headers.set(
      'Cache-Control',
      asset.ok || asset.status === 304
        ? path.startsWith('/assets/')
          ? contract.cache.assets
          : path.startsWith('/iroh/')
            ? contract.cache.iroh
            : contract.cache.html
        : 'no-cache',
    );
    const extension = url.pathname.match(/\.[a-z]+$/)?.[0];
    const mime = contract.mime[extension as keyof typeof contract.mime];
    if (asset.ok && mime) headers.set('Content-Type', mime);
    return new Response(request.method === 'HEAD' ? null : asset.body, {
      status: asset.status,
      statusText: asset.statusText,
      headers,
    });
  },
};
