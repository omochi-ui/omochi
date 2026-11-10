import { useCallback, useEffect, useRef, useState } from 'react';
import type { SessionFault } from '@omochi-ui/core/browser';
import type { BrowserTransport } from '@omochi-ui/hub-transport';
import { useDashboardNavigation } from '../DashboardNavigation.js';
import {
  ConnectionController,
  initialConnectionState,
  type ConnectionAction,
} from './connection-controller.js';
import { createFragmentInbox } from './fragment.js';
import type { ShareLink } from './link.js';
import { removeNativeInvitationButton } from './native-link.js';

export interface ConnectionScreenProps {
  transport: BrowserTransport;
  takeLink(): ShareLink | null;
  bootstrapError: SessionFault | null;
  subscribeFragment?: (receive: (fragment: string) => void) => () => void;
}
const noFragments = () => () => {};

/** React owns subscriptions; the controller owns all connection state and actions. */
export function useConnections({
  transport,
  takeLink,
  bootstrapError,
  subscribeFragment = noFragments,
}: ConnectionScreenProps) {
  const navigation = useDashboardNavigation();
  const [initialLink] = useState(takeLink);
  const [gatewayFragments] = useState(createFragmentInbox);
  const [state, setState] = useState(() =>
    initialConnectionState(initialLink, bootstrapError),
  );
  const current = useRef<ConnectionController | null>(null);
  const closing = useRef(Promise.resolve());

  useEffect(() => {
    const controller = new ConnectionController({
      transport,
      initialLink,
      bootstrapError,
      requestLeave: navigation.requestLeave,
    });
    current.current = controller;
    setState(controller.getSnapshot());
    const unsubscribeState = controller.subscribe(setState);
    // A replacement must wait for the previous profile's endpoint lock to release.
    void closing.current.then(() => controller.start());
    removeNativeInvitationButton(document);
    const unsubscribeFragment = subscribeFragment((fragment) => {
      removeNativeInvitationButton(document);
      if (fragment.startsWith('#g:')) gatewayFragments.receive(fragment);
      else controller.receive(fragment);
    });
    const close = () => {
      closing.current = controller.dispose();
    };
    window.addEventListener('pagehide', close, { once: true });
    return () => {
      current.current = null;
      unsubscribeState();
      unsubscribeFragment();
      window.removeEventListener('pagehide', close);
      close();
    };
  }, [
    transport,
    initialLink,
    bootstrapError,
    subscribeFragment,
    gatewayFragments,
    navigation,
  ]);

  const dispatch = useCallback(
    (action: ConnectionAction) =>
      current.current?.dispatch(action) ?? Promise.resolve(),
    [],
  );
  return { state, dispatch, subscribeGateway: gatewayFragments.subscribe };
}
