import { ja } from './ja';
import { en } from './en';

export type Locale = 'ja' | 'en';
const messages = { ja, en };

export function getLocale(locale: string | undefined): Locale {
  return locale === 'en' ? 'en' : 'ja';
}

export function getMessages(locale: Locale) {
  return messages[locale];
}
