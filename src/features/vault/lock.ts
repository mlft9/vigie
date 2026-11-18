// Verrouillage côté interface (VLT-03, ADR-026) : le cœur verrouille le coffre et ferme les
// sessions SSH ; l'interface détruit les terminaux (tampon de défilement compris) et oublie le
// contenu du coffre affiché.

import { closeAllTabs } from '@/features/terminal/TerminalSession';
import { vaultLock } from '@/lib/ipc';
import type { LockReason } from '@/lib/types';
import { useSftp } from '@/stores/sftp';
import { useVault } from '@/stores/vault';

/** Applique un verrouillage décidé par le cœur (inactivité) ou demandé par l'utilisateur. */
export function applyLocked(reason: LockReason): void {
  closeAllTabs();
  // Transferts et éditions : le cœur les a arrêtés (copies locales supprimées).
  useSftp.getState().reset();
  useVault.getState().setStatus('locked', reason);
}

/** Verrouillage manuel (bouton, Ctrl+Maj+L). */
export async function lockNow(): Promise<void> {
  // L'interface se verrouille même si l'appel au cœur échoue : rien ne doit rester affiché.
  applyLocked('manual');
  try {
    await vaultLock();
  } catch {
    // Le cœur verrouillera de toute façon à l'expiration du délai d'inactivité.
  }
}
