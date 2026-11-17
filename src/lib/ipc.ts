// Seul point d'appel au backend Rust. Chaque commande a sa fonction typée ;
// les composants n'appellent jamais `invoke` directement (règle ESLint).
import { invoke } from '@tauri-apps/api/core';

import type { AppError, AppInfo } from './types';

/** Code utilisé quand une erreur ne vient pas du backend sous la forme attendue. */
export const UNKNOWN_ERROR_CODE = 'unknown';

/** Vérifie qu'une valeur reçue est bien une `AppError` (le backend reste la seule source des codes). */
export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === 'object' &&
    value !== null &&
    'code' in value &&
    typeof value.code === 'string' &&
    /^[a-z0-9_]+(\.[a-z0-9_]+)*$/.test(value.code)
  );
}

/**
 * Normalise toute erreur en `AppError`. Le message brut éventuel n'est jamais propagé
 * vers l'interface : il pourrait contenir des données non maîtrisées.
 */
export function toAppError(error: unknown): AppError {
  return isAppError(error) ? { code: error.code } : { code: UNKNOWN_ERROR_CODE };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    // L'appelant reçoit une AppError normalisée (objet simple, volontairement pas une instance d'Error).
    // eslint-disable-next-line @typescript-eslint/only-throw-error
    throw toAppError(error);
  }
}

/** Nom et version de l'application. */
export function appInfo(): Promise<AppInfo> {
  return call<AppInfo>('app_info');
}
