import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';

import { detectLanguage, systemLanguages } from '@/i18n/languages';
import { useSettings } from '@/stores/settings';

import { applyTheme, resolveTheme, systemPrefersDark, watchSystemTheme } from './theme';

/**
 * Répercute les préférences (thème, langue) sur le document et sur i18next.
 * Monté une seule fois, à la racine de l'application.
 */
export function useApplySettings() {
  const theme = useSettings((s) => s.theme);
  const language = useSettings((s) => s.language);
  const { i18n } = useTranslation();

  useEffect(() => {
    applyTheme(resolveTheme(theme, systemPrefersDark()));
    if (theme !== 'system') {
      return undefined;
    }
    // En mode « système », on suit les changements à chaud (ex. passage en mode sombre le soir).
    return watchSystemTheme((prefersDark) => {
      applyTheme(resolveTheme('system', prefersDark));
    });
  }, [theme]);

  useEffect(() => {
    const effective = language === 'system' ? detectLanguage(systemLanguages()) : language;
    if (i18n.language !== effective) {
      void i18n.changeLanguage(effective);
    }
    document.documentElement.lang = effective;
  }, [language, i18n]);
}
