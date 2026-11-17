import { create } from 'zustand';
import { createJSONStorage, persist, type StateStorage } from 'zustand/middleware';

import { SUPPORTED_LANGUAGES, type Language } from '@/i18n/languages';

export const THEMES = ['system', 'light', 'dark'] as const;
export type ThemePreference = (typeof THEMES)[number];

/** `system` : suivre le système ; sinon, langue forcée par l'utilisateur. */
export type LanguagePreference = 'system' | Language;

export interface SettingsState {
  theme: ThemePreference;
  language: LanguagePreference;
  setTheme: (theme: ThemePreference) => void;
  setLanguage: (language: LanguagePreference) => void;
}

/** Clé de stockage local. Ces préférences ne contiennent aucune donnée sensible. */
export const SETTINGS_STORAGE_KEY = 'younji.settings';

/**
 * Accès à localStorage qui ne lève jamais d'exception (stockage désactivé ou plein) :
 * les préférences retombent alors simplement sur leurs valeurs par défaut.
 * Provisoire : migration vers settings.json, géré par le backend, à l'étape 2.
 */
const safeLocalStorage: StateStorage = {
  getItem: (name) => {
    try {
      return localStorage.getItem(name);
    } catch {
      return null;
    }
  },
  setItem: (name, value) => {
    try {
      localStorage.setItem(name, value);
    } catch {
      // Préférence non mémorisée : sans conséquence.
    }
  },
  removeItem: (name) => {
    try {
      localStorage.removeItem(name);
    } catch {
      // Idem.
    }
  },
};

function isTheme(value: unknown): value is ThemePreference {
  return typeof value === 'string' && (THEMES as readonly string[]).includes(value);
}

function isLanguagePreference(value: unknown): value is LanguagePreference {
  return (
    value === 'system' ||
    (typeof value === 'string' && (SUPPORTED_LANGUAGES as readonly string[]).includes(value))
  );
}

export const useSettings = create<SettingsState>()(
  persist(
    (set) => ({
      theme: 'system',
      language: 'system',
      setTheme: (theme) => {
        set({ theme });
      },
      setLanguage: (language) => {
        set({ language });
      },
    }),
    {
      name: SETTINGS_STORAGE_KEY,
      version: 1,
      storage: createJSONStorage(() => safeLocalStorage),
      partialize: (state) => ({ theme: state.theme, language: state.language }),
      // Le contenu du stockage n'est pas fiable : toute valeur inattendue est ignorée.
      merge: (persisted, current) => {
        const p = (persisted ?? {}) as Record<string, unknown>;
        return {
          ...current,
          theme: isTheme(p.theme) ? p.theme : current.theme,
          language: isLanguagePreference(p.language) ? p.language : current.language,
        };
      },
    },
  ),
);
