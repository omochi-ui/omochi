// Capture/remove bearer material before importing React, renderer, transport or UI modules.
// Keep this entry free of static imports and JSX (which adds a static JSX-runtime import).
let fragment: string | null = window.location.hash;
if (fragment.startsWith('#connect:'))
  history.replaceState(null, '', location.pathname + location.search);
// A cached document owns stopped repositories/sessions after pagehide. Reboot the
// document so storage and the endpoint lock are acquired before creating fresh scopes.
window.addEventListener('pageshow', (event) => {
  if (event.persisted) location.reload();
});

async function main() {
  const [
    { createRoot },
    { createElement },
    hub,
    transportModule,
    { ConnectionScreen },
    { SessionFault },
    { LazyBrowserBoardRepository },
  ] = await Promise.all([
    import('react-dom/client'),
    import('react'),
    import('@omochi-ui/browser-hub'),
    import('@omochi-ui/browser-transport'),
    import('./ConnectionScreen.js'),
    import('@omochi-ui/core/browser'),
    import('./browser-boards.js'),
    import('../../../../packages/hub-ui/src/styles.css'),
    import('./connections.css'),
    import('@omochi-ui/renderer/styles.css'),
  ]);
  let invitation: string | null = null,
    bootstrapError = null;
  if (fragment?.startsWith('#connect:')) {
    try {
      invitation = hub.parseConnectFragment(fragment);
    } catch {
      bootstrapError = new SessionFault('InvalidInvitation');
    }
  }
  fragment = null;
  const transport = await transportModule.loadBrowserTransport(
    new URL('./iroh/', document.baseURI),
  );
  const boards = new LazyBrowserBoardRepository(hub.openProfileDatabase);
  window.addEventListener('pagehide', () => boards.close(), { once: true });
  const root = createRoot(document.getElementById('root')!);
  const takeInvitation = () => {
    const captured = invitation;
    invitation = null;
    return captured;
  };
  root.render(
    createElement(ConnectionScreen, {
      transport,
      boards,
      takeInvitation,
      bootstrapError,
    }),
  );
}
void main().catch(() => {
  fragment = null;
  document.getElementById('root')!.textContent =
    'Direct connections are unavailable. Use a supported Chromium browser and allow browser storage, then reload.';
});
