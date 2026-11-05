/** Keep only the latest navigation while the hosted UI is loading. Never persist it. */
export function createFragmentInbox() {
  let pending: string | null = null;
  let listener: ((fragment: string) => void) | null = null;
  return {
    receive(fragment: string) {
      if (listener) listener(fragment);
      else pending = fragment;
    },
    take() {
      const current = pending;
      pending = null;
      return current;
    },
    subscribe(receive: (fragment: string) => void) {
      listener = receive;
      const initial = pending;
      pending = null;
      if (initial !== null) receive(initial);
      return () => { if (listener === receive) listener = null; };
    },
  };
}

/** Install before importing UI/transport modules so bearer fragments never wait for them. */
export function createHostedFragmentInbox(browser: Pick<Window, 'location' | 'history' | 'addEventListener'>, onInvitation?: (fragment: string) => void) {
  const inbox = createFragmentInbox();
  const capture = () => {
    const fragment = browser.location.hash;
    if (!fragment.startsWith('#share:') && !fragment.startsWith('#owner:') && !fragment.startsWith('#g:')) return;
    browser.history.replaceState(null, '', browser.location.pathname + browser.location.search);
    onInvitation?.(fragment);
    inbox.receive(fragment);
  };
  // Retain unknown initial fragments for the normal invalid-invitation error.
  if (browser.location.hash) inbox.receive(browser.location.hash);
  capture();
  browser.addEventListener('hashchange', capture);
  return {
    receive: inbox.receive,
    subscribe: inbox.subscribe,
    take() {
      // Navigation can precede its queued hashchange event when bootstrap finishes.
      capture();
      return inbox.take();
    },
  };
}
