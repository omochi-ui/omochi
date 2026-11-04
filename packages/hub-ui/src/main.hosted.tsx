// Capture bearer data before loading any UI/transport module.
let fragment: string | null = location.hash;
if ((fragment.startsWith('#share:') || fragment.startsWith('#owner:') || fragment.startsWith('#g:'))) history.replaceState(null, '', location.pathname + location.search);
window.addEventListener('pageshow', event => { if (event.persisted) location.reload(); });
async function main() {
  await import('./i18n.js');
  const [{ createRoot }, { createElement }, { ConnectionScreen }, { parseShareLink }, { loadBrowserTransport }, { SessionFault }] = await Promise.all([
    import('react-dom/client'), import('react'), import('./remote/ConnectionScreen.js'), import('./remote/link.js'), import('@omochi-ui/hub-transport'), import('@omochi-ui/core/browser'), import('./styles.css'), import('@omochi-ui/renderer/styles.css'),
  ]);
  let gatewayFragment = fragment?.startsWith('#g:') ? fragment : null;
  if (gatewayFragment) fragment = null;
  let link: import('./remote/link.js').ShareLink | null = null;
  let bootstrapError: import('@omochi-ui/core/browser').SessionFault | null = null;
  try {
    if (!fragment) { try { const saved = sessionStorage.getItem('omochi.shared.public'); if (saved) { const value = JSON.parse(saved); fragment = '#share:' + btoa(String.fromCharCode(...new TextEncoder().encode(JSON.stringify(value)))).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', ''); } } catch { /* No stored authority. */ } }
    if ((fragment?.startsWith('#share:') || fragment?.startsWith('#owner:'))) link = parseShareLink(fragment);
    else if (fragment) throw new SessionFault('InvalidInvitation');
  } catch { bootstrapError = new SessionFault('InvalidInvitation'); }
  fragment = null;
  const transport = await loadBrowserTransport(new URL('./iroh/', document.baseURI));
  const takeLink = () => { const current = link; link = null; return current; };
  const takeGateway = () => { const current = gatewayFragment; gatewayFragment = null; return current; };
  createRoot(document.getElementById('root')!).render(createElement(ConnectionScreen, { transport, takeLink, bootstrapError, takeGateway }));
}
void main().catch(async () => {
  fragment = null;
  const { default: i18n } = await import('./i18n.js');
  document.getElementById('root')!.textContent = i18n.t('connection.bootstrapError');
});
