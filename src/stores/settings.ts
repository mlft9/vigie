import { create } from 'zustand';

import { SUPPORTED_LANGUAGES, type Language } from '@/i18n/languages';
import { settingsGet, settingsSet } from '@/lib/ipc';
import type { AppSettings } from '@/lib/types';

export const THEMES = ['system', 'light', 'dark'] as const;
export type ThemePreference = (typeof THEMES)[number];

/** `system` : suivre le système ; sinon, langue forcée par l'utilisateur. */
export type LanguagePreference = 'system' | Language;

/** Bornes de la taille de police du terminal (revérifiées par le cœur). */
export const MIN_FONT_SIZE = 8;
export const MAX_FONT_SIZE = 32;
export const DEFAULT_FONT_SIZE = 14;

export interface SettingsState {
  theme: ThemePreference;
  language: LanguagePreference;
  terminalFontSize: number;
  setTheme: (theme: ThemePreference) => void;
  setLanguage: (language: LanguagePreference) => void;
  setTerminalFontSize: (size: number) => void;
}

/**
 * Ancienne clé de stockage local (étape 1). Ses valeurs sont reprises une fois dans
 * `settings.json`, géré par le cœur, puis la clé est supprimée.
 */
export const LEGACY_STORAGE_KEY = 'younji.settings';

function isTheme(value: unknown): value is ThemePreference {
  return typeof value === 'string' && (THEMES as readonly string[]).includes(value);
}

function isLanguagePreference(value: unknown): value is LanguagePreference {
  return (
    value === 'system' ||
    (typeof value === 'string' && (SUPPORTED_LANGUAGES as readonly string[]).includes(value))
  );
}

function isFontSize(value: unknown): value is number {
  return (
    typeof value === 'number' &&
    Number.isInteger(value) &&
    value >= MIN_FONT_SIZE &&
    value <= MAX_FONT_SIZE
  );
}

function snapshot(state: SettingsState): AppSettings {
  return {
    language: state.language,
    theme: state.theme,
    terminalFontSize: state.terminalFontSize,
  };
}

/** Enregistre dans le cœur ; un échec est sans conséquence (préférence non mémorisée). */
function persist(state: SettingsState): void {
  void settingsSet(snapshot(state)).catch(() => undefined);
}

export const useSettings = create<SettingsState>()((set, get) => ({
  theme: 'system',
  language: 'system',
  terminalFontSize: DEFAULT_FONT_SIZE,
  setTheme: (theme) => {
    set({ theme });
    persist(get());
  },
  setLanguage: (language) => {
    set({ language });
    persist(get());
  },
  setTerminalFontSize: (size) => {
    if (!isFontSize(size)) return;
    set({ terminalFontSize: size });
    persist(get());
  },
}));

/** Applique des valeurs non fiables : toute valeur inattendue est ignorée. */
function applyUntrusted(values: Record<string, unknown>): void {
  const current = useSettings.getState();
  useSettings.setState({
    theme: isTheme(values.theme) ? values.theme : current.theme,
    language: isLanguagePreference(values.language) ? values.language : current.language,
    terminalFontSize: isFontSize(values.terminalFontSize)
      ? values.terminalFontSize
      : current.terminalFontSize,
  });
}

function readLegacy(): Record<string, unknown> | null {
  try {
    const raw = localStorage.getItem(LEGACY_STORAGE_KEY);
    if (raw === null) return null;
    const parsed: unknown = JSON.parse(raw);
    const state =
      typeof parsed === 'object' && parsed !== null && 'state' in parsed ? parsed.state : null;
    return typeof state === 'object' && state !== null ? (state as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

function removeLegacy(): void {
  try {
    localStorage.removeItem(LEGACY_STORAGE_KEY);
  } catch {
    // Stockage indisponible : rien à supprimer.
  }
}

/**
 * Charge les paramètres depuis le cœur, avant le premier rendu. Au premier lancement après
 * l'étape 1, reprend les préférences du `localStorage`. Hors de Tauri (tests, navigateur), les
 * valeurs par défaut restent en place.
 */
export async function loadSettings(): Promise<void> {
  try {
    const { settings, firstRun } = await settingsGet();
    applyUntrusted({ ...settings });
    if (firstRun) {
      const legacy = readLegacy();
      if (legacy) {
        applyUntrusted(legacy);
        persist(useSettings.getState());
      }
    }
    removeLegacy();
  } catch {
    // Cœur indisponible : valeurs par défaut.
  }
}
