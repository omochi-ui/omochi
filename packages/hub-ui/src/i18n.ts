import { createInstance, type InitOptions, type ParseKeys } from 'i18next';
import LanguageDetector from 'i18next-browser-languagedetector';
import { initReactI18next } from 'react-i18next';
import en from './locales/en.js';
import ja from './locales/ja.js';

type Strings<T> = {
  [K in keyof T]: T[K] extends string ? string : Strings<T[K]>;
} & {
  [K in keyof T as K extends `${infer Base}_other` ? `${Base}_${Intl.LDMLPluralRule}` : never]?: string;
};
export type LocaleResource = Strings<typeof en>;
/** Store message keys so pending feedback also follows a language change. */
export interface TranslationMessage {
  key: ParseKeys;
  values?: Record<string, string | number>;
}

export const languages = [
  { code: 'en', name: 'English' },
  { code: 'ja', name: '日本語' },
] as const;
export const languageStorageKey = 'omochi.dashboard.language';

declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'translation';
    resources: { translation: LocaleResource };
    strictKeyChecks: true;
  }
}

/** Bundled resources keep both local and hosted dashboards usable offline. */
export function createDashboardI18n(options: InitOptions = {}) {
  const instance = createInstance();
  void instance.use(LanguageDetector).use(initReactI18next).init({
    resources: { en: { translation: en }, ja: { translation: ja } },
    supportedLngs: languages.map(language => language.code),
    fallbackLng: 'en',
    load: 'languageOnly',
    initAsync: false,
    detection: {
      order: ['localStorage', 'navigator'],
      lookupLocalStorage: languageStorageKey,
      caches: ['localStorage'],
    },
    interpolation: { escapeValue: false }, // React escapes interpolated text.
    ...options,
  });
  return instance;
}

const i18n = createDashboardI18n();
function updateDocumentLanguage() {
  if (typeof document === 'undefined') return;
  document.documentElement.lang = i18n.resolvedLanguage ?? 'en';
  document.documentElement.dir = i18n.dir();
}
i18n.on('languageChanged', updateDocumentLanguage);
updateDocumentLanguage();
export default i18n;
