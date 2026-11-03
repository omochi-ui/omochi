export interface SharingRepository { request(path: string, method?: string, body?: unknown): Promise<unknown> }
export class HttpSharingRepository implements SharingRepository {
  async request(path: string, method = 'GET', body?: unknown): Promise<unknown> {
    const response = await fetch(path, { method, headers: { 'Content-Type': 'application/json' }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
    if (!response.ok) { await response.body?.cancel(); throw new Error('Could not update sharing. Refresh and retry.'); }
    return response.json();
  }
}
