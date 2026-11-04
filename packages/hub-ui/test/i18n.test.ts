import assert from 'node:assert/strict';
import { expect } from 'bun:test';
import { test, type TestContext } from '@omochi-ui/core/testing';
import { SessionFault } from '@omochi-ui/core/browser';
import { BoardFault } from '../src/board-repository.js';
import { SharingFault } from '../src/sharing-repository.js';
import { createDashboardI18n, languageStorageKey } from '../src/i18n.js';
import { localizedError } from '../src/localized-error.js';
import en from '../src/locales/en.js';
import ja from '../src/locales/ja.js';

function browserLanguage(context: TestContext, languages: string[], saved?: string) {
  for (const [key, value] of Object.entries({ languages, language: languages[0] })) {
    const descriptor = Object.getOwnPropertyDescriptor(navigator, key);
    Object.defineProperty(navigator, key, { configurable: true, value });
    context.after(() => {
      if (descriptor) Object.defineProperty(navigator, key, descriptor);
      else Reflect.deleteProperty(navigator, key);
    });
  }
  const previous = localStorage.getItem(languageStorageKey);
  localStorage.removeItem(languageStorageKey);
  if (saved) localStorage.setItem(languageStorageKey, saved);
  context.after(() => {
    if (previous === null) localStorage.removeItem(languageStorageKey);
    else localStorage.setItem(languageStorageKey, previous);
  });
}

function flatten(value: object, prefix = ''): Record<string, string> {
  return Object.fromEntries(Object.entries(value).flatMap(([key, text]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    return typeof text === 'string' ? [[path, text]] : Object.entries(flatten(text, path));
  }));
}

test('Japanese resources cover every English key and preserve interpolation and rich text', () => {
  const english = flatten(en), japanese = flatten(ja);
  const baseKey = (key: string) => key.replace(/_(zero|one|two|few|many|other)$/, '');
  expect([...new Set(Object.keys(japanese).map(baseKey))].sort()).toEqual([...new Set(Object.keys(english).map(baseKey))].sort());
  const tokens = (value: string) => [...value.matchAll(/{{\s*([^}]+?)\s*}}|<\/?([\w]+)>/g)].map(match => match[0]).sort();
  for (const [key, value] of Object.entries(japanese)) {
    const source = english[key] ?? english[`${baseKey(key)}_other`];
    expect(value.trim().length).toBeGreaterThan(0);
    expect(tokens(value)).toEqual(tokens(source));
  }
});

test('regional browser languages select Japanese and unsupported languages fall back to English', context => {
  browserLanguage(context, ['ja-JP', 'en-US']);
  const japanese = createDashboardI18n();
  assert.equal(japanese.isInitialized, true);
  assert.equal(japanese.resolvedLanguage, 'ja');
  assert.equal(japanese.t('workspace.title'), 'ダッシュボード');
  localStorage.removeItem(languageStorageKey);
  Object.defineProperty(navigator, 'languages', { configurable: true, value: ['fr-FR'] });
  Object.defineProperty(navigator, 'language', { configurable: true, value: 'fr-FR' });
  assert.equal(createDashboardI18n().resolvedLanguage, 'en');
});

test('saved preferences override browser languages and language changes survive a new instance', async context => {
  browserLanguage(context, ['ja-JP'], 'en');
  const instance = createDashboardI18n();
  assert.equal(instance.resolvedLanguage, 'en');
  await instance.changeLanguage('ja');
  assert.equal(localStorage.getItem(languageStorageKey), 'ja');
  assert.equal(createDashboardI18n().resolvedLanguage, 'ja');
});

test('counts use complete localized messages with English plurals', () => {
  const instance = createDashboardI18n({ lng: 'en', detection: { caches: [] } });
  assert.equal(instance.t('workspace.apps', { count: 0 }), '0 apps available');
  assert.equal(instance.t('workspace.apps', { count: 1 }), '1 app available');
  assert.equal(instance.t('workspace.apps', { count: 2 }), '2 apps available');
  assert.equal(instance.t('workspace.views', { count: 1 }), '1 view');
  const japanese = instance.getFixedT('ja');
  assert.equal(japanese('workspace.apps', { count: 1 }), '1個のアプリが利用可能');
  assert.equal(japanese('workspace.views', { count: 2 }), '2個のビュー');
  assert.equal(japanese('confirmation.deleteTitle', { name: 'Daily overview' }), '「Daily overview」を削除しますか？');
});

test('missing translations fall back to English', () => {
  const instance = createDashboardI18n({ lng: 'ja', detection: { caches: [] }, resources: {
    en: { translation: en }, ja: { translation: { workspace: { title: 'ダッシュボード' } } },
  } });
  assert.equal(instance.t('workspace.title'), 'ダッシュボード');
  assert.equal(instance.t('workspace.save'), 'Save dashboard');
});

test('stored faults are translated at presentation time without changing their public codes', async () => {
  const instance = createDashboardI18n({ lng: 'en', detection: { caches: [] } });
  const fault = new BoardFault('Conflict');
  assert.equal(localizedError(fault, instance.t), en.boardError.Conflict);
  await instance.changeLanguage('ja');
  assert.equal(localizedError(fault, instance.t), ja.boardError.Conflict);
  assert.equal(fault.code, 'Conflict');
  assert.equal(localizedError(new SessionFault('InvalidInvitation'), instance.t), ja.sessionError.InvalidInvitation);
  assert.equal(localizedError(new SharingFault(), instance.t), ja.share.updateError);
});
