// Application d'isolation Tauri (ADR-015).
//
// Tauri fait transiter chaque appel IPC de l'interface par cette iframe sandboxée,
// sur une origine distincte, avant de le chiffrer pour le cœur Rust. Une dépendance npm
// compromise dans l'interface ne peut donc ni lire ce script ni le contourner.
//
// Règle : seules les commandes de la liste blanche passent. Toute autre commande est
// remplacée par une commande inexistante : le cœur la rejette avec une erreur, ce qui
// fait échouer l'appel proprement (lever une exception ici le laisserait bloqué).
//
// Ce fichier doit rester autonome : aucune dépendance, aucune étape de compilation.
(function (root) {
  'use strict';

  /** Commandes que l'interface a le droit d'appeler. À tenir à jour avec build.rs et les capabilities. */
  var ALLOWED_COMMANDS = Object.freeze(['app_info']);

  /** Nom volontairement absent du cœur : un appel refusé échoue avec une erreur. */
  var DENIED_COMMAND = 'younji_isolation_denied';

  function isAllowed(cmd) {
    return typeof cmd === 'string' && ALLOWED_COMMANDS.indexOf(cmd) !== -1;
  }

  function hook(message) {
    if (message !== null && typeof message === 'object' && isAllowed(message.cmd)) {
      return message;
    }
    var denied = Object.create(null);
    denied.cmd = DENIED_COMMAND;
    denied.callback = message && message.callback;
    denied.error = message && message.error;
    denied.options = undefined;
    // La charge utile d'un appel refusé n'est jamais transmise au cœur.
    denied.payload = Object.create(null);
    return denied;
  }

  Object.defineProperty(root, '__TAURI_ISOLATION_HOOK__', { value: hook });
  Object.defineProperty(root, '__YOUNJI_ISOLATION__', {
    value: Object.freeze({ allowedCommands: ALLOWED_COMMANDS, deniedCommand: DENIED_COMMAND }),
  });
})(window);
