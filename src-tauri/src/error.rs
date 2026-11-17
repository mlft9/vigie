//! Erreurs renvoyées à l'interface.
//!
//! Une commande ne renvoie jamais de message libre : seulement un code stable, qui correspond
//! à une clé de traduction `errors.<code>` côté interface. Le détail technique part dans les
//! journaux, sans secret (SECURITE.md §6).

use serde::Serialize;

/// Erreur sérialisable à destination de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "le premier usage arrive avec les commandes SSH de l'étape 1"
    )
)]
pub struct AppError {
    /// Code stable, par exemple `ssh.auth_failed`.
    code: &'static str,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "le premier usage arrive avec les commandes SSH de l'étape 1"
    )
)]
impl AppError {
    /// Crée une erreur à partir d'un code. Le code doit exister dans `src/locales/*.json`.
    pub const fn new(code: &'static str) -> Self {
        Self { code }
    }

    /// Code de l'erreur.
    pub const fn code(&self) -> &'static str {
        self.code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_only_the_code() {
        let error = AppError::new("ipc.command_denied");
        assert_eq!(error.code(), "ipc.command_denied");
        let json = serde_json::to_value(error).unwrap();
        assert_eq!(json, serde_json::json!({ "code": "ipc.command_denied" }));
    }
}
