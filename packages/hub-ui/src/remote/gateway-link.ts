import { isValidNodeId } from '@omochi-ui/core/browser';
export interface GatewayInvitation {
  invitation: string;
  gatewayId: string;
  expires: number;
}
export function parseGatewayLink(fragment: string): GatewayInvitation {
  if (!fragment.startsWith('#g:') || fragment.length > 16384)
    throw new Error('Invalid gateway invitation');
  const payload = fragment.slice(3);
  if (!/^[A-Za-z0-9_-]+$/.test(payload))
    throw new Error('Invalid gateway invitation');
  const value = JSON.parse(
    new TextDecoder('utf-8', { fatal: true }).decode(
      Uint8Array.from(
        atob(payload.replaceAll('-', '+').replaceAll('_', '/')),
        (c) => c.charCodeAt(0),
      ),
    ),
  );
  if (
    !value ||
    Object.keys(value).sort().join(',') !== 'address,expires,secret,version' ||
    value.version !== 4 ||
    !isValidNodeId(value.address?.id) ||
    typeof value.secret !== 'string' ||
    !/^[0-9a-fA-F]{64}$/.test(value.secret) ||
    !Number.isSafeInteger(value.expires) ||
    value.expires * 1000 <= Date.now()
  )
    throw new Error('Invalid or expired gateway invitation');
  return {
    invitation: `omochi://invite/${payload}`,
    gatewayId: value.address.id,
    expires: value.expires,
  };
}
