// Suivi du dossier courant du shell (SFTP-01) par la séquence OSC 7.
//
// Le chemin vient du serveur : c'est une donnée non fiable. Il est décodé strictement et ne
// sert qu'à lister ce même serveur ; le cœur le revalide (chemin absolu, longueur, NUL).

/** Longueur maximale d'un chemin accepté (comme `MAX_REMOTE_PATH` côté Rust). */
const MAX_PATH = 4096;

/** Caractères de contrôle (C0, DEL, C1) : NUL, retour à la ligne, échappements. */
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;

/**
 * Extrait le chemin d'une séquence OSC 7 (`file://hôte/chemin`, encodé en pourcentage).
 * Renvoie `null` si la forme est invalide. Le nom d'hôte est ignoré : il ne peut pas être
 * comparé de façon fiable à l'adresse de connexion.
 */
export function parseOsc7(data: string): string | null {
  if (data.length > MAX_PATH * 3 + 300 || !data.startsWith('file://')) return null;
  const rest = data.slice('file://'.length);
  const slash = rest.indexOf('/');
  if (slash === -1) return null;
  let path: string;
  try {
    path = decodeURIComponent(rest.slice(slash));
  } catch {
    return null;
  }
  if (path.length > MAX_PATH || CONTROL.test(path)) return null;
  return path;
}

/**
 * Intégration au prompt pour bash et zsh, envoyée au shell **seulement sur demande** de
 * l'utilisateur. Texte constant : aucune donnée de l'utilisateur ni du serveur n'y est
 * insérée. L'espace initiale l'écarte de l'historique (`HISTCONTROL=ignorespace`, réglage
 * par défaut des distributions courantes). Le chemin est encodé en pourcentage.
 */
export const SHELL_INTEGRATION = [
  ' __younji_osc7() { local LC_ALL=C p="$PWD" o="" c i;',
  ' for ((i=0; i<${#p}; i++)); do c="${p:i:1}";',
  ` case "$c" in [a-zA-Z0-9/._~-]) o+="$c" ;; *) printf -v c '%%%02X' "'$c"; o+="$c" ;; esac; done;`,
  ` printf '\\033]7;file://%s%s\\033\\\\' "\${HOSTNAME:-}" "$o"; };`,
  ' if [ -n "${ZSH_VERSION:-}" ]; then autoload -Uz add-zsh-hook && add-zsh-hook precmd __younji_osc7;',
  ' else PROMPT_COMMAND="__younji_osc7${PROMPT_COMMAND:+;$PROMPT_COMMAND}"; fi; __younji_osc7\r',
].join('');
