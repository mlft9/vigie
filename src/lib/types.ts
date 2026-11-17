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
