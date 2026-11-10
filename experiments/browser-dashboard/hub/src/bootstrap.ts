import { isValidNodeId, SessionFault } from '@omochi-ui/core/browser';

export interface BrowserInvitation {
  readonly nodeId: string;
  readonly relayUrls: readonly string[];
  readonly secret: string;
  readonly expires: number;
}
const prefix = 'omochi://invite/';
const invalid = (): never => {
  throw new SessionFault('InvalidInvitation');
};
function object(
  value: unknown,
  keys: readonly string[],
): Record<string, unknown> {
  if (
    !value ||
    typeof value !== 'object' ||
    Array.isArray(value) ||
    Object.keys(value).length !== keys.length ||
    keys.some((key) => !Object.hasOwn(value, key))
  )
    return invalid();
  return value as Record<string, unknown>;
}
export function validRelayUrl(value: unknown): value is string {
  if (
    typeof value !== 'string' ||
    value.length > 2048 ||
    !value ||
    value !== value.trim()
  )
    return false;
  try {
    const url = new URL(value);
    return (
      /^https:\/\//i.test(value) &&
      url.protocol === 'https:' &&
      !!url.hostname &&
      !url.username &&
      !url.password &&
      !value.split('/')[2]?.includes('@') &&
      !url.search &&
      !url.hash &&
      !value.includes('#') &&
      !value.includes('?')
    );
  } catch {
    return false;
  }
}
function validIpHint(value: unknown): boolean {
  if (typeof value !== 'string') return false;
  const match = /^(\[[0-9a-fA-F:.]+\]|[0-9.]+):(0|[1-9]\d*)$/.exec(value);
  if (!match || Number(match[2]) > 65535) return false;
  if (!match[1]!.startsWith('['))
    return (
      match[1]!.split('.').length === 4 &&
      match[1]!
        .split('.')
        .every((part) => /^(0|[1-9]\d{0,2})$/.test(part) && Number(part) <= 255)
    );
  try {
    return new URL(`http://${value}/`).hostname.startsWith('[');
  } catch {
    return false;
  }
}
function decodeJson(bytes: Uint8Array): unknown {
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  } catch {
    return invalid();
  }
}

function decodeBase64urlJson(encoded: string): unknown {
  if (!/^[A-Za-z0-9_-]+$/.test(encoded) || encoded.length % 4 === 1)
    return invalid();
  let binary: string;
  try {
    binary = atob(encoded.replace(/-/g, '+').replace(/_/g, '/'));
  } catch {
    return invalid();
  }
  // Reject noncanonical trailing bits as well as padding and alternative alphabets.
  if (
    btoa(binary).replace(/=/g, '').replace(/\+/g, '-').replace(/\//g, '_') !==
    encoded
  )
    return invalid();
  return decodeJson(Uint8Array.from(binary, (byte) => byte.charCodeAt(0)));
}

/** Secret-bearing return value: retain only in owner-private memory. IP hints are ignored. */
export function parseInvitation(text: string): Readonly<BrowserInvitation> {
  if (typeof text !== 'string') return invalid();
  text = text.trim();
  if (text.length > 16384 || !text.startsWith(prefix)) return invalid();
  const invite = object(decodeBase64urlJson(text.slice(prefix.length)), [
    'version',
    'address',
    'secret',
    'expires',
  ]);
  const address = object(invite.address, ['id', 'addrs']);
  if (
    invite.version !== 4 ||
    typeof invite.secret !== 'string' ||
    !/^[0-9a-fA-F]{64}$/.test(invite.secret) ||
    typeof address.id !== 'string' ||
    !isValidNodeId(address.id) ||
    !Array.isArray(address.addrs) ||
    !Number.isSafeInteger(invite.expires) ||
    Number(invite.expires) <= Math.floor(Date.now() / 1000) ||
    !Number.isSafeInteger(Number(invite.expires) * 1000)
  )
    return invalid();
  const relays: string[] = [];
  for (const entry of address.addrs) {
    if (
      !entry ||
      typeof entry !== 'object' ||
      Array.isArray(entry) ||
      Object.keys(entry).length !== 1
    )
      return invalid();
    if (Object.hasOwn(entry, 'Relay')) {
      if (!validRelayUrl(entry.Relay)) return invalid();
      if (!relays.includes(entry.Relay)) relays.push(entry.Relay);
    } else if (!Object.hasOwn(entry, 'Ip') || !validIpHint(entry.Ip))
      return invalid();
  }
  if (!relays.length || relays.length > 4) return invalid();
  return Object.freeze({
    nodeId: address.id,
    secret: invite.secret,
    expires: invite.expires as number,
    relayUrls: Object.freeze(relays),
  });
}

/** Validate the navigation wrapper without treating it as another authority. */
export function parseConnectFragment(fragment: string): string {
  if (
    typeof fragment !== 'string' ||
    fragment.length > 32768 ||
    !fragment.startsWith('#connect:')
  )
    return invalid();
  const wrapper = object(decodeBase64urlJson(fragment.slice(9)), [
    'version',
    'invitation',
  ]);
  if (wrapper.version !== 1 || typeof wrapper.invitation !== 'string')
    return invalid();
  parseInvitation(wrapper.invitation);
  return wrapper.invitation;
}

/** Call before loading the hosted UI. Removal also happens for malformed links. */
export function captureConnectFragment(): string | null {
  const fragment = globalThis.location.hash;
  if (!fragment.startsWith('#connect:')) return null;
  globalThis.history.replaceState(
    null,
    '',
    globalThis.location.pathname + globalThis.location.search,
  );
  return parseConnectFragment(fragment);
}

export function invitationAddress(
  invite: Pick<BrowserInvitation, 'nodeId' | 'relayUrls'>,
): string {
  return JSON.stringify({
    id: invite.nodeId,
    addrs: invite.relayUrls.map((Relay) => ({ Relay })),
  });
}
