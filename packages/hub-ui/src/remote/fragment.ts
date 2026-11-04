/** Keep only the latest navigation while the hosted UI is loading. Never persist it. */
export function createFragmentInbox() {
  let pending: string | null = null;
  let listener: ((fragment: string) => void) | null = null;
  return {
    receive(fragment: string) {
      if (listener) listener(fragment);
      else pending = fragment;
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
