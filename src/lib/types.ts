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
  | { type: 'hostKeyChanged'; key: HostKeyInfo; known: HostKeyInfo[]; knownHostsPath: string }
  | { type: 'authenticating' }
  | { type: 'connected' }
  | { type: 'disconnected'; code: string };
