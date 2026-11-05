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
  const link = document.createElement('a');
  link.href = href;
  link.className = 'hub-button';
  link.textContent = document.documentElement.lang.startsWith('ja') || navigator.language.startsWith('ja') ? 'Omochiアプリで開く' : 'Open in Omochi app';
  link.style.cssText = 'display:block;margin:16px;padding:12px;text-align:center';
  container.append(link);
  const note = document.createElement('p');
  note.style.cssText = 'margin:0 16px 16px;text-align:center';
  note.textContent = fragment.startsWith('#share:')
    ? (navigator.language.startsWith('ja') ? 'この共有リンクはアプリでも同じ権限で利用できます。' : 'This sharing link grants the same permissions in the app.')
    : navigator.language.startsWith('ja')
    ? 'ブラウザーで登録済みの招待は引き継げません。アプリには新しい招待を発行してください。'
    : 'An invitation already registered in a browser cannot be transferred. Issue a new invitation for the app.';
  container.append(note);
  document.body.prepend(container);
}
