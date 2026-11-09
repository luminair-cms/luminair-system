import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import { useUiStore } from '@/store/use-ui-store';
import en from './locales/en.json';
import ru from './locales/ru.json';
import uk from './locales/uk.json';

// ---------------------------------------------------------------------------
// Primary Export (i18n Instance)
// ---------------------------------------------------------------------------

export const defaultNS = 'translation';

export const resources = {
  en: { translation: en },
  uk: { translation: uk },
  ru: { translation: ru },
} as const;

void i18n.use(initReactI18next).init({
  resources,
  lng: useUiStore.getState().activeLocale || 'en',
  fallbackLng: 'en',
  defaultNS,
  interpolation: {
    escapeValue: false,
  },
});

useUiStore.subscribe((state) => {
  if (state.activeLocale && state.activeLocale !== i18n.language) {
    void i18n.changeLanguage(state.activeLocale);
  }
});

export { i18n };
export default i18n;
