import { useEffect } from 'react';

import { vaultTouch, vaultWatch } from '@/lib/ipc';
import { useVault } from '@/stores/vault';

import { applyLocked } from './lock';

/** Intervalle minimal entre deux signalements d'activité au cœur. */
export const TOUCH_INTERVAL_MS = 15_000;

/**
 * Relie l'interface au coffre : écoute les verrouillages décidés par le cœur et signale
 * l'activité de l'utilisateur (clavier, souris, molette) au minuteur de verrouillage, au plus
 * une fois toutes les 15 s. Monté une fois, à la racine.
 */
export function useVaultSession(): void {
  useEffect(() => {
    void vaultWatch((event) => {
      // Seul événement à ce jour : le verrouillage.
      if (useVault.getState().status === 'unlocked') {
        applyLocked(event.reason);
      }
    }).catch(() => undefined);
  }, []);

  useEffect(() => {
    let last = 0;
    function onActivity() {
      if (useVault.getState().status !== 'unlocked') return;
      const now = Date.now();
      if (now - last < TOUCH_INTERVAL_MS) return;
      last = now;
      void vaultTouch().catch(() => undefined);
    }
    const events = ['keydown', 'pointerdown', 'wheel'] as const;
    for (const name of events) {
      window.addEventListener(name, onActivity, { capture: true, passive: true });
    }
    return () => {
      for (const name of events) {
        window.removeEventListener(name, onActivity, { capture: true });
      }
    };
  }, []);
}
