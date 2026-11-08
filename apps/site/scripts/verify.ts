import { readdir, readFile, stat } from 'node:fs/promises';
import { join, relative } from 'node:path';
import assert from 'node:assert/strict';
const dist = new URL('../dist/', import.meta.url).pathname;
const content = new URL('../src/content/docs/', import.meta.url).pathname;
async function files(dir: string): Promise<string[]> {
  const entries = await readdir(dir, { withFileTypes: true });
  return (await Promise.all(entries.map(e => e.isDirectory() ? files(join(dir, e.name)) : [join(dir, e.name)]))).flat();
}
const htmlFiles = (await files(dist)).filter(p => p.endsWith('.html'));
const pages = new Map<string, { ids: Set<string>; refs: string[]; html: string }>();
for (const file of htmlFiles) {
  const html = await readFile(file, 'utf8');
  const url = '/' + relative(dist, file).replace(/index\.html$/, '');
  pages.set(url, {
    html,
    ids: new Set([...html.matchAll(/\bid="([^"]+)"/g)].map(m => m[1]!)),
    refs: [...html.matchAll(/\b(?:href|src)="([^"]+)"/g)].map(m => m[1]!.replaceAll('&amp;', '&')),
  });
  assert.equal((html.match(/<h1(?:\s|>)/g) || []).length, 1, `${url}: one h1`);
  assert(!html.includes('Welcome to Starlight'), `${url}: no starter content`);
}
assert(pages.get('/')?.html.includes('lang="ja"'), 'Japanese LP at root');
assert(pages.get('/en/')?.html.includes('lang="en"'), 'English LP under /en/');
assert(![...pages.keys()].some(path => path.startsWith('/ja/')), 'No legacy Japanese prefix');
const errors: string[] = [];
for (const [path, page] of pages) {
  for (const ref of page.refs) {
    const url = new URL(ref, `https://omochi.fyi${path}`);
    if (url.origin !== 'https://omochi.fyi') continue;
    const target = pages.get(url.pathname);
    if (target) {
      if (url.hash && !target.ids.has(decodeURIComponent(url.hash.slice(1)))) errors.push(`${path}: missing anchor ${ref}`);
    } else {
      try { assert((await stat(join(dist, decodeURIComponent(url.pathname)))).isFile()); }
      catch { errors.push(`${path}: missing target ${ref}`); }
    }
  }
  if (path.startsWith('/docs/')) {
    assert(page.html.includes('lang="ja"'), `${path}: Japanese is the default`);
    const translated = pages.get('/en' + path);
    assert(translated, `${path}: English translation exists`);
    assert(page.html.includes(`https://omochi.fyi/en${path}`), `${path}: translated alternate`);
    assert(translated.html.includes(`https://omochi.fyi${path}`), `${path}: Japanese alternate`);
    assert(translated.html.includes('lang="en"'), `${path}: English language`);
  }
  if (path.startsWith('/en/docs/')) {
    assert(pages.has(path.slice(3)), `${path}: Japanese translation exists`);
  }
  if (path !== '/404.html') assert(page.html.includes(`href="https://omochi.fyi${path}"`), `${path}: canonical`);
}
assert.equal(errors.length, 0, errors.join('\n'));
const index = JSON.parse(await readFile(join(dist, 'pagefind/pagefind-entry.json'), 'utf8'));
assert(index.languages.en, 'English search index');
assert(index.languages.ja, 'Japanese search index');
const docCount = (await files(content)).filter(p => /\.mdx?$/.test(p)).length;
assert.equal(htmlFiles.length, docCount + 4, 'Each doc, two LPs, the talk page, and a 404 are built');
console.log(`Verified ${htmlFiles.length} pages: local links, anchors, assets, translations, canonical URLs, and en/ja search.`);
