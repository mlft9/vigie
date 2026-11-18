//! Politique du mot de passe maître (SECURITE.md §3.4).
//!
//! Le contrôle **fait foi côté cœur** : l'indicateur affiché par l'interface n'est qu'une aide.
//! Minimum 12 caractères et score zxcvbn d'au moins 3 (« difficile à deviner »).
//!
//! Limite connue : zxcvbn copie le mot de passe dans des chaînes internes qui ne sont pas
//! effacées. C'est la même limite que pour russh (SECURITE.md §6) ; elle relève du hors-modèle
//! « lecture de la mémoire par un logiciel malveillant local ».

use secrecy::{ExposeSecret, SecretString};
use zxcvbn::{Score, zxcvbn};

use crate::VaultError;

/// Longueur minimale, en caractères.
pub const MIN_MASTER_LEN: usize = 12;
/// Longueur maximale, en octets (borne le coût de l'estimation).
pub const MAX_MASTER_LEN: usize = 1024;
/// Score zxcvbn minimal exigé.
pub const MIN_SCORE: Score = Score::Three;

/// Vérifie qu'un mot de passe maître respecte la politique.
pub fn check_master_password(password: &SecretString) -> Result<(), VaultError> {
    let text = password.expose_secret();
    if text.chars().count() < MIN_MASTER_LEN || text.len() > MAX_MASTER_LEN {
        return Err(VaultError::PasswordTooShort);
    }
    if zxcvbn(text, &[]).score() < MIN_SCORE {
        return Err(VaultError::PasswordTooWeak);
    }
    Ok(())
}

/// Vérifie un mot de passe saisi pour déverrouiller : seulement les bornes de longueur, pour
/// ne rien révéler de la politique ni coûter d'estimation.
pub fn check_unlock_password(password: &SecretString) -> Result<(), VaultError> {
    let len = password.expose_secret().len();
    if len == 0 || len > MAX_MASTER_LEN {
        return Err(VaultError::WrongPasswordOrCorrupted);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(password: &str) -> Result<(), VaultError> {
        check_master_password(&SecretString::from(password))
    }

    #[test]
    fn short_passwords_are_refused() {
        assert_eq!(check("Xq7!vR2#"), Err(VaultError::PasswordTooShort));
        assert_eq!(check(""), Err(VaultError::PasswordTooShort));
        assert_eq!(
            check(&"a".repeat(MAX_MASTER_LEN + 1)),
            Err(VaultError::PasswordTooShort)
        );
    }

    #[test]
    fn guessable_passwords_are_refused_even_when_long() {
        assert_eq!(check("passwordpassword"), Err(VaultError::PasswordTooWeak));
        assert_eq!(check("123456789012"), Err(VaultError::PasswordTooWeak));
        assert_eq!(check("aaaaaaaaaaaaaaaa"), Err(VaultError::PasswordTooWeak));
    }

    #[test]
    fn strong_passwords_are_accepted() {
        assert_eq!(check("cheval agrafe batterie correct 42"), Ok(()));
        assert_eq!(check("Tq9#mZ2!vLp8@xR"), Ok(()));
        // Les caractères comptent, pas les octets.
        assert_eq!(check("éléphant-vélo-hêtre-forêt"), Ok(()));
    }

    #[test]
    fn unlock_only_checks_bounds() {
        assert!(check_unlock_password(&SecretString::from("court")).is_ok());
        assert_eq!(
            check_unlock_password(&SecretString::from("")),
            Err(VaultError::WrongPasswordOrCorrupted)
        );
    }
}
