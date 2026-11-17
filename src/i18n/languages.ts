/** Langues prises en charge par l'interface. */
export const SUPPORTED_LANGUAGES = ['fr', 'en'] as const;

export type Language = (typeof SUPPORTED_LANGUAGES)[number];

/** Langue utilisée quand aucune préférence du système n'est reconnue. */
export const FALLBACK_LANGUAGE: Language = 'en';

function isLanguage(value: string): value is Language {
  return (SUPPORTED_LANGUAGES as readonly string[]).includes(value);
}

/**
 * Choisit la langue de l'interface à partir des préférences du système
 * (`navigator.languages`, qui reflète la langue d'affichage du système dans la WebView).
 * La première langue prise en charge l'emporte, quelle que soit sa variante régionale.
 */
export function detectLanguage(preferred: readonly string[]): Language {
  for (const tag of preferred) {
    const base = tag.toLowerCase().split('-')[0] ?? '';
    if (isLanguage(base)) {
      return base;
    }
  }
  return FALLBACK_LANGUAGE;
}

/** Langues du système, avec repli si l'environnement n'expose rien. */
export function systemLanguages(): readonly string[] {
  if (typeof navigator === 'undefined') {
    return [];
  }
  return navigator.languages.length > 0 ? navigator.languages : [navigator.language];
}
