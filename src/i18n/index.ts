import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

import en from '@/locales/en.json';
import fr from '@/locales/fr.json';

import { FALLBACK_LANGUAGE, type Language } from './languages';

export const resources = {
  fr: { translation: fr },
  en: { translation: en },
} as const;

/**
 * Initialise i18next une seule fois. Les traductions sont embarquées dans le bundle :
 * aucun chargement réseau, compatible avec la CSP stricte.
 */
export function initI18n(language: Language): typeof i18n {
  if (!i18n.isInitialized) {
    void i18n.use(initReactI18next).init({
      resources,
      lng: language,
      fallbackLng: FALLBACK_LANGUAGE,
      supportedLngs: ['fr', 'en'],
      interpolation: {
        // React échappe déjà le texte rendu ; aucune traduction n'est interprétée comme du HTML.
        escapeValue: false,
      },
      returnNull: false,
      initAsync: false,
    });
  }
  return i18n;
}

export { i18n };
