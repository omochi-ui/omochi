import type { ShareLink } from './link.js';

export function nativeConnectionLink(link: ShareLink): string | null {
  const payload = btoa(String.fromCharCode(...new TextEncoder().encode(JSON.stringify(link))))
    .replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '');
  return nativeInvitationLink(`#${link.mode}:${payload}`);
}

/** Bearer payloads stay in the fragment or custom URI, never query parameters. */
export function nativeInvitationLink(fragment: string): string | null {
  const match = /^#(g|share|owner):([A-Za-z0-9_-]+)$/.exec(fragment);
  if (!match || fragment.length > 32768) return null;
  return `omochi://${match[1] === 'g' ? 'invite' : match[1]}/${match[2]}`;
}

/** Installed before WASM initialization; native handoff must survive bootstrap failure. */
export function installNativeInvitationButton(document: Document, fragment: string): void {
  document.getElementById('native-invitation-link')?.remove();
  const href = nativeInvitationLink(fragment);
  if (!href) return;
  const container = document.createElement('aside');
  container.id = 'native-invitation-link';
  container.dataset.invitationKind = fragment.startsWith('#share:') ? 'share' : 'invitation';
  let language = navigator.language;
  try { language = localStorage.getItem('omochi.dashboard.language') || language; } catch { /* Use the browser preference before UI initialization. */ }
  const japanese = language.startsWith('ja');
  const link = document.createElement('a');
  link.href = href;
  link.className = 'hub-button';
  link.textContent = japanese ? 'Omochiアプリで開く' : 'Open in Omochi app';
  link.style.cssText = 'display:block;margin:16px;padding:12px;text-align:center';
  container.append(link);
  const note = document.createElement('p');
  note.style.cssText = 'margin:0 16px 16px;text-align:center';
  note.textContent = fragment.startsWith('#share:')
    ? (japanese ? 'この共有リンクはアプリでも同じ権限で利用できます。' : 'This sharing link grants the same permissions in the app.')
    : japanese
    ? 'ブラウザーで登録済みの招待は引き継げません。アプリには新しい招待を発行してください。'
    : 'An invitation already registered in a browser cannot be transferred. Issue a new invitation for the app.';
  container.append(note);
  document.body.prepend(container);
}

/** Once React can present invitations, it owns native handoff and dismissal. */
export function removeNativeInvitationButton(document: Document): void {
  document.getElementById('native-invitation-link')?.remove();
}

export function translateNativeInvitationButton(document: Document, labels: { open: string; shareHelp: string; invitationHelp: string }): void {
  const container = document.getElementById('native-invitation-link');
  const link = container?.querySelector('a'), note = container?.querySelector('p');
  if (link) link.textContent = labels.open;
  if (note) note.textContent = container?.dataset.invitationKind === 'share' ? labels.shareHelp : labels.invitationHelp;
}
