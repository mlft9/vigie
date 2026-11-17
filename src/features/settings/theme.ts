import type { ThemePreference } from '@/stores/settings';

export type ResolvedTheme = 'light' | 'dark';

const DARK_QUERY = '(prefers-color-scheme: dark)';

/** Résout la préférence en thème effectif, en interrogeant le système si besoin. */
export function resolveTheme(
  preference: ThemePreference,
  systemPrefersDark: boolean,
): ResolvedTheme {
  if (preference === 'system') {
    return systemPrefersDark ? 'dark' : 'light';
  }
  return preference;
}

export function systemPrefersDark(): boolean {
  return typeof window.matchMedia === 'function' && window.matchMedia(DARK_QUERY).matches;
}

/** Applique le thème effectif sur <html> (classe exclusive `light` / `dark`). */
export function applyTheme(theme: ResolvedTheme, root: HTMLElement = document.documentElement) {
  root.classList.toggle('dark', theme === 'dark');
  root.classList.toggle('light', theme === 'light');
}

/** S'abonne aux changements de thème du système. Renvoie la fonction de désabonnement. */
export function watchSystemTheme(onChange: (prefersDark: boolean) => void): () => void {
  if (typeof window.matchMedia !== 'function') {
    return () => undefined;
  }
  const query = window.matchMedia(DARK_QUERY);
  const listener = (event: MediaQueryListEvent) => {
    onChange(event.matches);
  };
  query.addEventListener('change', listener);
  return () => {
    query.removeEventListener('change', listener);
  };
}
