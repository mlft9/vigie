// Ouverture d'une session depuis le carnet d'hôtes (HST-02).

import { terminalSession } from '@/features/terminal/TerminalSession';
import type { HostEntry, IdentityEntry } from '@/lib/types';
import { type ConnectTarget, useSessions } from '@/stores/sessions';

import { effectiveUsername } from './filter';

/**
 * Ouvre un onglet vers l'hôte. Avec une identité, le cœur se connecte directement avec les
 * secrets du coffre, sans rien redemander. Sans identité, le formulaire de connexion rapide
 * s'ouvre pré-rempli et demande le mot de passe ou la clé.
 */
export function openHost(host: HostEntry, identities: readonly IdentityEntry[]): string {
  const sessions = useSessions.getState();
  const tabId = sessions.openTab();
  const identity = identities.find((i) => i.id === host.identityId);
  const target: ConnectTarget = {
    host: host.address,
    port: host.port,
    username: effectiveUsername(host.username, identity?.username),
    authKind: 'password',
    keyFile: null,
    label: host.name,
  };
  if (identity) {
    void terminalSession(tabId).connectHost({ ...target, hostId: host.id });
  } else {
    sessions.update(tabId, { target, status: 'form' });
  }
  return tabId;
}
