// Types partagés avec le backend. Ils reflètent les structures Rust de src-tauri
// (serde en camelCase) : toute modification doit être faite des deux côtés.

/** Erreur renvoyée par une commande : uniquement un code stable, traduit par `errors.<code>`. */
export interface AppError {
  code: string;
}

/** Réponse de la commande `app_info` (src-tauri/src/commands/app.rs). */
export interface AppInfo {
  name: string;
  version: string;
}

/** Identifiant opaque d'une session SSH, attribué par le cœur. */
export type SessionId = number;

/** Fichier de clé choisi dans la boîte de dialogue du système (le chemin reste dans le cœur). */
export interface KeyFileRef {
  id: number;
  name: string;
}

/** Moyen d'authentification envoyé une seule fois à `ssh_connect`. */
export type AuthRequest =
  | { kind: 'password'; password: string }
  | { kind: 'key'; keyFileId: number; passphrase: string | null };

/** Demande de connexion (src-tauri/src/commands/ssh.rs, `ConnectRequest`). */
export interface ConnectRequest {
  host: string;
  port: number;
  username: string;
  auth: AuthRequest;
  cols: number;
  rows: number;
}

/** Description d'une clé d'hôte : algorithme et empreinte `SHA256:…`. */
export interface HostKeyInfo {
  algorithm: string;
  fingerprint: string;
}

/** Événement d'état d'une session (`SessionEventDto`). */
export type SessionEvent =
  | { type: 'hostKeyUnknown'; key: HostKeyInfo }
  | {
      type: 'hostKeyChanged';
      key: HostKeyInfo;
      known: HostKeyInfo[];
      hostPattern: string;
    }
  | { type: 'authenticating' }
  | { type: 'connected' }
  | { type: 'disconnected'; code: string };

// ------------------------------------------------------------------ coffre (étape 2)

/** État du coffre (`vault_status`). */
export type VaultStatus = 'missing' | 'locked' | 'unlocked';

/** Cause d'un verrouillage. */
export type LockReason = 'manual' | 'idle' | 'reload';

/** Événement du coffre reçu par le `Channel` de `vault_watch`. */
export interface VaultEvent {
  type: 'locked';
  reason: LockReason;
}

/** Identifiant opaque d'une entrée du coffre (UUID). */
export type EntryId = string;

/** Hôte du carnet (aucun secret). */
export interface HostEntry {
  id: EntryId;
  name: string;
  address: string;
  port: number;
  username: string;
  identityId: EntryId | null;
  groupId: EntryId | null;
  tags: string[];
  notes: string;
}

/** Groupe d'hôtes. */
export interface GroupEntry {
  id: EntryId;
  name: string;
}

/** Identité : le mot de passe ne quitte jamais le cœur, seule sa présence est connue. */
export interface IdentityEntry {
  id: EntryId;
  name: string;
  username: string;
  hasPassword: boolean;
  keyId: EntryId | null;
}

/** Clé importée : partie publique uniquement. */
export interface KeyEntry {
  id: EntryId;
  name: string;
  algorithm: string;
  fingerprint: string;
  publicKey: string;
}

/** Contenu affichable du coffre (`vault_snapshot`). */
export interface VaultSnapshot {
  hosts: HostEntry[];
  groups: GroupEntry[];
  identities: IdentityEntry[];
  keys: KeyEntry[];
  autoLockMinutes: number;
}

/** Création (`id` nul) ou modification d'un hôte. */
export type HostRequest = Omit<HostEntry, 'id'> & { id: EntryId | null };

/** Changement du mot de passe d'une identité. */
export type PasswordRequest =
  { action: 'keep' } | { action: 'clear' } | { action: 'set'; value: string };

/** Création (`id` nul) ou modification d'une identité. */
export interface IdentityRequest {
  id: EntryId | null;
  name: string;
  username: string;
  password: PasswordRequest;
  keyId: EntryId | null;
}

/** Import d'une clé choisie par `ssh_pick_key_file`. */
export interface KeyImportRequest {
  keyFileId: number;
  name: string;
  passphrase: string | null;
}

/** Clé d'hôte connue (HKY-04). */
export interface KnownHostEntry {
  hosts: string;
  algorithm: string;
  fingerprint: string;
  revoked: boolean;
}

/** Paramètres d'apparence (`settings.json`). */
export interface AppSettings {
  language: 'system' | 'fr' | 'en';
  theme: 'system' | 'light' | 'dark';
  terminalFontSize: number;
}

/** Réponse de `settings_get`. */
export interface SettingsResponse {
  settings: AppSettings;
  /** Aucun fichier encore écrit : reprendre les anciennes préférences éventuelles. */
  firstRun: boolean;
}
