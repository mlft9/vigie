import { create } from 'zustand';

import type { AuthPromptInfo, EntryId, HostKeyInfo, KeyFileRef } from '@/lib/types';

/** Onglet d'accueil (sections de la barre latérale), toujours présent en premier. */
export const HOME_TAB_ID = 'home';

/** Longueur maximale d'un titre d'onglet personnalisé. */
export const MAX_TAB_TITLE = 64;

/**
 * État d'un onglet de session :
 * - `form` : formulaire de connexion (nouvel onglet, échec avant connexion, reconnexion) ;
 * - `connecting` / `authenticating` : connexion en cours ;
 * - `hostKey` : clé d'hôte inconnue, en attente de la décision de l'utilisateur ;
 * - `authPrompt` : le serveur pose des questions (2FA), en attente des réponses ;
 * - `blocked` : clé d'hôte modifiée, connexion refusée sans contournement ;
 * - `connected` : terminal utilisable ;
 * - `disconnected` : session terminée après avoir été établie.
 */
export type TabStatus =
  | 'form'
  | 'connecting'
  | 'hostKey'
  | 'authenticating'
  | 'authPrompt'
  | 'blocked'
  | 'connected'
  | 'disconnected';

/** Cible d'une connexion, **sans aucun secret** (le mot de passe n'est jamais stocké). */
export interface ConnectTarget {
  host: string;
  port: number;
  username: string;
  authKind: 'password' | 'key' | 'interactive';
  keyFile: KeyFileRef | null;
  /** Hôte du carnet : la connexion utilise son identité enregistrée, sans rien redemander. */
  hostId?: EntryId;
  /** Nom de l'hôte dans le carnet, utilisé comme titre d'onglet. */
  label?: string;
}

export interface HostKeyChange {
  key: HostKeyInfo;
  known: HostKeyInfo[];
  /** Forme de l'hôte dans les clés connues (`hôte` ou `[hôte]:port`). */
  hostPattern: string;
}

/** `terminal` : session SSH avec terminal ; `sftp` : explorateur double panneau (SFTP-02). */
export type TabKind = 'terminal' | 'sftp';

export interface SessionTab {
  id: string;
  kind: TabKind;
  /** Onglet de terminal dont un onglet SFTP utilise la connexion. */
  parentTabId: string | null;
  customTitle: string | null;
  /** Dernière cible tentée, pour pré-remplir le formulaire de reconnexion. */
  target: ConnectTarget | null;
  status: TabStatus;
  /** Code d'erreur (`errors.<code>`) de la dernière tentative ou de la déconnexion. */
  errorCode: string | null;
  /** Clé d'hôte inconnue en attente de décision. */
  pendingHostKey: HostKeyInfo | null;
  /** Clé d'hôte modifiée (connexion bloquée). */
  hostKeyChange: HostKeyChange | null;
  /** Questions du serveur en attente de réponse (aucune réponse n'est conservée ici). */
  pendingPrompt: AuthPromptInfo | null;
  /** Barre de recherche du terminal ouverte. */
  searchOpen: boolean;
  /** Identifiant de la session du cœur (opaque, sans secret), tant qu'elle est ouverte. */
  sessionId: number | null;
  /** Dossier courant du shell annoncé par OSC 7 (donnée du serveur, non fiable). */
  remoteCwd: string | null;
  /** Panneau SFTP latéral ouvert (onglet de terminal). */
  sftpPanelOpen: boolean;
}

interface SessionsState {
  tabs: SessionTab[];
  activeTabId: string;
  /** Ouvre un onglet de connexion et l'active. Renvoie son identifiant. */
  openTab: () => string;
  /**
   * Ouvre un onglet SFTP double panneau sur la connexion d'un onglet de terminal et l'active.
   * Renvoie son identifiant, ou `null` si l'onglet parent n'existe pas.
   */
  openSftpTab: (parentTabId: string) => string | null;
  /** Retire un onglet (la session doit avoir été fermée par l'appelant). */
  removeTab: (id: string) => void;
  activate: (id: string) => void;
  /** Active l'onglet suivant (+1) ou précédent (-1), accueil compris, en boucle. */
  activateRelative: (delta: number) => void;
  /** Titre personnalisé ; une chaîne vide rétablit le titre par défaut. */
  renameTab: (id: string, title: string) => void;
  /** Déplace un onglet de session vers la gauche (-1) ou la droite (+1). */
  moveTab: (id: string, delta: number) => void;
  /** Place l'onglet `id` à la position de l'onglet `targetId`. */
  moveTabTo: (id: string, targetId: string) => void;
  update: (id: string, patch: Partial<Omit<SessionTab, 'id'>>) => void;
}

let counter = 0;
function newTabId(): string {
  counter += 1;
  return `session-${Date.now().toString(36)}-${counter}`;
}

export const useSessions = create<SessionsState>()((set, get) => ({
  tabs: [],
  activeTabId: HOME_TAB_ID,

  openTab: () => {
    const id = newTabId();
    const tab: SessionTab = {
      id,
      kind: 'terminal',
      parentTabId: null,
      customTitle: null,
      target: null,
      status: 'form',
      errorCode: null,
      pendingHostKey: null,
      hostKeyChange: null,
      pendingPrompt: null,
      searchOpen: false,
      sessionId: null,
      remoteCwd: null,
      sftpPanelOpen: false,
    };
    set((s) => ({ tabs: [...s.tabs, tab], activeTabId: id }));
    return id;
  },

  openSftpTab: (parentTabId) => {
    const parent = get().tabs.find((t) => t.id === parentTabId && t.kind === 'terminal');
    if (!parent) return null;
    const id = newTabId();
    const tab: SessionTab = {
      id,
      kind: 'sftp',
      parentTabId,
      customTitle: null,
      target: parent.target,
      status: 'connected',
      errorCode: null,
      pendingHostKey: null,
      hostKeyChange: null,
      pendingPrompt: null,
      searchOpen: false,
      sessionId: null,
      remoteCwd: parent.remoteCwd,
      sftpPanelOpen: false,
    };
    set((s) => {
      // Placé juste après le terminal (et ses autres onglets SFTP).
      let index = s.tabs.findIndex((t) => t.id === parentTabId);
      while (s.tabs[index + 1]?.parentTabId === parentTabId) index += 1;
      const tabs = [...s.tabs];
      tabs.splice(index + 1, 0, tab);
      return { tabs, activeTabId: id };
    });
    return id;
  },

  removeTab: (id) => {
    set((s) => {
      const index = s.tabs.findIndex((t) => t.id === id);
      if (index === -1) return s;
      const tabs = s.tabs.filter((t) => t.id !== id);
      let activeTabId = s.activeTabId;
      if (activeTabId === id) {
        // Comme dans un navigateur : l'onglet voisin de droite, sinon de gauche, sinon l'accueil.
        activeTabId = tabs[index]?.id ?? tabs[index - 1]?.id ?? HOME_TAB_ID;
      }
      return { tabs, activeTabId };
    });
  },

  activate: (id) => {
    if (id === HOME_TAB_ID || get().tabs.some((t) => t.id === id)) {
      set({ activeTabId: id });
    }
  },

  activateRelative: (delta) => {
    const ids = [HOME_TAB_ID, ...get().tabs.map((t) => t.id)];
    const current = Math.max(0, ids.indexOf(get().activeTabId));
    const next = (current + delta + ids.length) % ids.length;
    set({ activeTabId: ids[next] ?? HOME_TAB_ID });
  },

  renameTab: (id, title) => {
    const trimmed = title.trim().slice(0, MAX_TAB_TITLE);
    get().update(id, { customTitle: trimmed === '' ? null : trimmed });
  },

  moveTab: (id, delta) => {
    set((s) => {
      const index = s.tabs.findIndex((t) => t.id === id);
      const target = index + delta;
      if (index === -1 || target < 0 || target >= s.tabs.length) return s;
      const tabs = [...s.tabs];
      const [moved] = tabs.splice(index, 1);
      if (moved) tabs.splice(target, 0, moved);
      return { tabs };
    });
  },

  moveTabTo: (id, targetId) => {
    const target = get().tabs.findIndex((t) => t.id === targetId);
    const index = get().tabs.findIndex((t) => t.id === id);
    if (target !== -1 && index !== -1) get().moveTab(id, target - index);
  },

  update: (id, patch) => {
    set((s) => ({ tabs: s.tabs.map((t) => (t.id === id ? { ...t, ...patch } : t)) }));
  },
}));

/** Titre affiché : titre personnalisé, sinon `utilisateur@hôte`, sinon `null` (« Nouvelle session »). */
export function defaultTabTitle(tab: SessionTab): string | null {
  if (tab.customTitle) return tab.customTitle;
  if (!tab.target) return null;
  if (tab.target.label) return tab.target.label;
  const host = tab.target.host.includes(':') ? `[${tab.target.host}]` : tab.target.host;
  return `${tab.target.username}@${host}`;
}
