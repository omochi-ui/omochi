export interface SharingRepository { request(path: string, method?: string, body?: unknown): Promise<unknown> }
export class SharingFault extends Error {
  readonly code: 'updateError' | 'infoUnavailable' | 'unavailable' | 'relayUnavailable';
  constructor(code: SharingFault['code'] = 'updateError') {
    super('Could not update sharing. Refresh and retry.'); this.name = 'SharingFault'; this.code = code;
  }
}
export class HttpSharingRepository implements SharingRepository {
  async request(path: string, method = 'GET', body?: unknown): Promise<unknown> {
    const response = await fetch(path, { method, headers: { 'Content-Type': 'application/json' }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
    if (!response.ok) { await response.body?.cancel(); throw new SharingFault(); }
    return response.json();
  }
}
