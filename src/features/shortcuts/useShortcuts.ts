import { useEffect } from 'react';

import { closeTab, existingTerminalSession } from '@/features/terminal/TerminalSession';
import { lockNow } from '@/features/vault/lock';
import { useSessions } from '@/stores/sessions';

import { matchShortcut } from './shortcuts';

/** Vrai si le focus est dans un terminal xterm.js. */
function focusInTerminal(): boolean {
  return document.activeElement?.closest('.xterm') != null;
}

/**
 * Empêche le rechargement de la page (F5, Ctrl+R, Ctrl+Maj+R) hors du terminal : il
 * fermerait toutes les sessions. Dans le terminal, xterm.js consomme déjà ces touches
 * (Ctrl+R est la recherche inverse de bash).
 */
function isReloadKey(event: KeyboardEvent): boolean {
  return event.key === 'F5' || (event.ctrlKey && event.key.toLowerCase() === 'r');
}

/** Raccourcis globaux (APP-04), interceptés avant xterm.js. Monté une fois dans AppShell. */
export function useShortcuts() {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (isReloadKey(event) && !focusInTerminal()) {
        event.preventDefault();
        return;
      }
      const action = matchShortcut(event);
      if (!action) return;
      const sessions = useSessions.getState();
      const tab = sessions.tabs.find((t) => t.id === sessions.activeTabId);
      const terminal = tab ? existingTerminalSession(tab.id) : undefined;

      const handled = () => {
        event.preventDefault();
        event.stopPropagation();
      };

      switch (action) {
        case 'newTab':
          handled();
          sessions.openTab();
          break;
        case 'closeTab':
          handled();
          if (tab) closeTab(tab.id);
          break;
        case 'nextTab':
          handled();
          sessions.activateRelative(1);
          break;
        case 'previousTab':
          handled();
          sessions.activateRelative(-1);
          break;
        case 'moveTabLeft':
        case 'moveTabRight':
          handled();
          if (tab) sessions.moveTab(tab.id, action === 'moveTabLeft' ? -1 : 1);
          break;
        case 'search':
          if (tab && terminal && (tab.status === 'connected' || tab.status === 'disconnected')) {
            handled();
            sessions.update(tab.id, { searchOpen: true });
          }
          break;
        case 'copy':
          if (terminal && focusInTerminal()) {
            handled();
            void terminal.copySelection().catch(() => undefined);
          }
          break;
        case 'lock':
          // Verrouillage (VLT-03, APP-04) : prioritaire, même depuis le terminal.
          handled();
          void lockNow();
          break;
        case 'paste':
          // Pas de preventDefault : le navigateur déclenche le collage natif, intercepté par la
          // garde de collage du terminal. On empêche seulement xterm d'envoyer Ctrl+V au shell.
          if (terminal && focusInTerminal()) event.stopPropagation();
          break;
      }
    }
    window.addEventListener('keydown', onKeyDown, { capture: true });
    return () => {
      window.removeEventListener('keydown', onKeyDown, { capture: true });
    };
  }, []);
}
