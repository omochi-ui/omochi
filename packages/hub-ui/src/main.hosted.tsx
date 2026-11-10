import {
  installNativeInvitationButton,
  translateNativeInvitationButton,
} from './remote/native-link.js';
import { createHostedFragmentInbox } from './remote/fragment.js';
// Capture bearer data before loading any UI/transport module.
const fragments = createHostedFragmentInbox(window, (fragment) =>
  installNativeInvitationButton(document, fragment),
);
window.addEventListener('pageshow', (event) => {
  if (event.persisted) location.reload();
});
async function main() {
  const { default: i18n } = await import('./i18n.js');
  translateNativeInvitationButton(document, {
    open: i18n.t('native.open'),
    shareHelp: i18n.t('native.shareHelp'),
    invitationHelp: i18n.t('native.invitationHelp'),
  });
  const [
    { createRoot },
    { createElement },
    { ConnectionScreen },
    { parseShareLink },
    { loadBrowserTransport },
    { SessionFault },
  ] = await Promise.all([
    import('react-dom/client'),
    import('react'),
    import('./remote/ConnectionScreen.js'),
    import('./remote/link.js'),
    import('@omochi-ui/hub-transport'),
    import('@omochi-ui/core/browser'),
    import('./styles.css'),
    import('@omochi-ui/renderer/styles.css'),
  ]);
  const transport = await loadBrowserTransport(
    new URL('./iroh/', document.baseURI),
  );
  let fragment = fragments.take();
  if (fragment?.startsWith('#g:')) {
    fragments.receive(fragment);
    fragment = null;
  }
  let link: import('./remote/link.js').ShareLink | null = null;
  let bootstrapError: import('@omochi-ui/core/browser').SessionFault | null =
    null;
  try {
    if (!fragment) {
      try {
        const saved = sessionStorage.getItem('omochi.shared.public');
        if (saved) {
          const value = JSON.parse(saved);
          fragment =
            '#share:' +
            btoa(
              String.fromCharCode(
                ...new TextEncoder().encode(JSON.stringify(value)),
              ),
            )
              .replaceAll('+', '-')
              .replaceAll('/', '_')
              .replaceAll('=', '');
        }
      } catch {
        /* No stored authority. */
      }
    }
    if (fragment?.startsWith('#share:') || fragment?.startsWith('#owner:'))
      link = parseShareLink(fragment);
    else if (fragment) throw new SessionFault('InvalidInvitation');
  } catch {
    bootstrapError = new SessionFault('InvalidInvitation');
  }
  fragment = null;
  const takeLink = () => {
    const current = link;
    link = null;
    return current;
  };
  createRoot(document.getElementById('root')!).render(
    createElement(ConnectionScreen, {
      transport,
      takeLink,
      bootstrapError,
      subscribeFragment: fragments.subscribe,
    }),
  );
}
void main().catch(async () => {
  fragments.take();
  const { default: i18n } = await import('./i18n.js');
  document.getElementById('root')!.textContent = i18n.t(
    'connection.bootstrapError',
  );
});
