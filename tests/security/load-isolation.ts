import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

export interface IsolationMessage {
  cmd: unknown;
  callback: unknown;
  error: unknown;
  options?: unknown;
  payload: unknown;
}

export interface IsolationWindow {
  __TAURI_ISOLATION_HOOK__: (message: unknown) => IsolationMessage;
  __YOUNJI_ISOLATION__: { allowedCommands: readonly string[]; deniedCommand: string };
}

/** Charge le script d'isolation dans un contexte vierge, comme le ferait l'iframe sandboxée. */
export function loadIsolation(): IsolationWindow {
  const source = readFileSync(new URL('../../isolation/index.js', import.meta.url), 'utf8');
  const window = {};
  runInNewContext(source, { window });
  return window as IsolationWindow;
}
