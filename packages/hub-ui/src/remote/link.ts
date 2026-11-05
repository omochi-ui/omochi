import { SessionFault, isValidNodeId } from '@omochi-ui/core/browser';
export interface ShareLink { version: 1 | 2; hubId: string; relayUrls: string[]; grantId: string; mode: 'share' | 'owner'; secret: string }
export interface SavedShare { owner?: true; hubId: string; relayUrls: string[]; grantId: string; name: string }
const uuid = (id: unknown): id is string => typeof id === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(id);
export function relays(value: unknown): value is string[] {
  return Array.isArray(value) && value.length >= 1 && value.length <= 4 && new Set(value).size === value.length && value.every(item => {
    if (typeof item !== 'string' || item.length > 2048) return false;
    try { const url = new URL(item); return url.protocol === 'https:' && !!url.hostname && !url.username && !url.password && !url.search && !url.hash; } catch { return false; }
  });
}
export function validSavedShare(value: unknown): value is SavedShare {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const v = value as Record<string, unknown>;
  return (Object.keys(v).sort().join(',') === 'grantId,hubId,name,relayUrls' || (v.owner === true && Object.keys(v).sort().join(',') === 'grantId,hubId,name,owner,relayUrls')) && typeof v.hubId === 'string' && isValidNodeId(v.hubId) && uuid(v.grantId) && relays(v.relayUrls) && typeof v.name === 'string' && v.name.length <= 200;
}
export function parseShareLink(fragment: string): ShareLink {
  try {
    const owner = fragment.startsWith('#owner:');
    if (!(owner || fragment.startsWith('#share:')) || fragment.length > 32768) throw 0;
    const encoded = fragment.slice(7);
    if (!/^[A-Za-z0-9_-]+$/.test(encoded) || encoded.length % 4 === 1) throw 0;
    const bytes = Uint8Array.from(atob(encoded.replaceAll('-', '+').replaceAll('_', '/')), c => c.charCodeAt(0));
    const value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)) as ShareLink;
    if (!value || typeof value !== 'object' || Object.keys(value).sort().join(',') !== 'grantId,hubId,mode,relayUrls,secret,version' || value.version !== (owner ? 1 : 2) || !isValidNodeId(value.hubId) || !uuid(value.grantId) || !relays(value.relayUrls) || !(owner ? value.mode === 'owner' : value.mode === 'share') || typeof value.secret !== 'string' || !/^[0-9a-f]{64}$/.test(value.secret)) throw 0;
    return structuredClone(value);
  } catch { throw new SessionFault('InvalidInvitation'); }
}
export function address(source: Pick<ShareLink, 'hubId' | 'relayUrls'>): string {
  return JSON.stringify({ id: source.hubId, addrs: source.relayUrls.map(Relay => ({ Relay })) });
}
