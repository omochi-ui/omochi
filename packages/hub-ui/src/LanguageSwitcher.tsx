import { useTranslation } from 'react-i18next';
import { languages } from './i18n.js';

export function LanguageSwitcher() {
  const { t, i18n } = useTranslation();
  return (
    <label className="language-switcher">
      <span>{t('language.label')}</span>
      <select
        value={i18n.resolvedLanguage ?? 'en'}
        onChange={(event) => {
          void i18n.changeLanguage(event.target.value);
        }}
      >
        {languages.map((language) => (
          <option
            key={language.code}
            value={language.code}
            lang={language.code}
          >
            {language.name}
          </option>
        ))}
      </select>
    </label>
  );
}
