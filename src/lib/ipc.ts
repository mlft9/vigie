// Seul point d'appel au backend Rust. Chaque commande a sa fonction typée ;
// les composants n'appellent jamais `invoke` directement (règle ESLint).
import { Channel, invoke } from '@tauri-apps/api/core';

import type {
  AppError,
  AppInfo,
  ConnectRequest,
  KeyFileRef,
  SessionEvent,
  SessionId,
} from './types';

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

/** Destinataires des flux d'une session SSH. */
export interface SessionListeners {
  /** Changement d'état (clé d'hôte, connexion, fin). */
  onEvent: (event: SessionEvent) => void;
  /** Sortie du terminal, en octets bruts, à acquitter avec `sshAck` une fois affichée. */
  onOutput: (data: Uint8Array) => void;
}

/**
 * Ouvre une session SSH. Le secret éventuel de `request.auth` n'est transmis qu'ici, une seule
 * fois ; il ne revient jamais du cœur.
 */
export function sshConnect(
  request: ConnectRequest,
  listeners: SessionListeners,
): Promise<SessionId> {
  const onEvent = new Channel<SessionEvent>(listeners.onEvent);
  const onOutput = new Channel<ArrayBuffer>((buffer) => {
    listeners.onOutput(new Uint8Array(buffer));
  });
  return call<SessionId>('ssh_connect', { request, onEvent, onOutput });
}

/** Envoie une saisie. `binary` : chaque caractère représente un octet brut (0–255). */
export function sshWrite(sessionId: SessionId, data: string, binary = false): Promise<void> {
  return call<undefined>('ssh_write', { sessionId, data, binary });
}

/** Répercute la taille du terminal sur le serveur. */
export function sshResize(sessionId: SessionId, cols: number, rows: number): Promise<void> {
  return call<undefined>('ssh_resize', { sessionId, cols, rows });
}

/** Acquitte des octets de sortie affichés (contrôle de flux). */
export function sshAck(sessionId: SessionId, bytes: number): Promise<void> {
  return call<undefined>('ssh_ack', { sessionId, bytes });
}

/** Accepte ou refuse une clé d'hôte inconnue. */
export function sshHostKeyDecision(sessionId: SessionId, accept: boolean): Promise<void> {
  return call<undefined>('ssh_host_key_decision', { sessionId, accept });
}

/** Ferme une session. */
export function sshClose(sessionId: SessionId): Promise<void> {
  return call<undefined>('ssh_close', { sessionId });
}

/** Ouvre la boîte de dialogue du système pour choisir une clé privée (`null` si annulé). */
export function sshPickKeyFile(): Promise<KeyFileRef | null> {
  return call<KeyFileRef | null>('ssh_pick_key_file');
}

/** Ouvre un lien http(s) dans le navigateur du système (revalidé par le cœur). */
export function openExternalUrl(url: string): Promise<void> {
  return call<undefined>('open_external_url', { url });
}
