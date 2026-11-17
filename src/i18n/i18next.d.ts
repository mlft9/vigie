import 'i18next';

import type fr from '@/locales/fr.json';

// Typage des clés de traduction : une clé absente de fr.json ne compile pas.
// La parité avec en.json est vérifiée par un test (src/i18n/locales.test.ts).
declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'translation';
    resources: {
      translation: typeof fr;
    };
  }
}
