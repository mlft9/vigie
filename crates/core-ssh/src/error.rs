//! Erreurs de la crate. Chaque variante correspond à un code stable (`ssh.*`), traduit côté
//! interface par la clé `errors.<code>`.
//!
//! Règle (SECURITE.md §6) : aucune variante ne transporte de secret, d'entrée utilisateur ni de
//! texte brut venant du serveur. Le détail technique éventuel (erreur d'E/S, erreur russh) reste
//! dans les journaux, au niveau `debug`.

/// Erreur du client SSH.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SshError {
    /// Adresse d'hôte invalide (vide, trop longue, caractères interdits).
    #[error("adresse d'hôte invalide")]
    InvalidHost,
    /// Port hors de 1–65535.
    #[error("port invalide")]
    InvalidPort,
    /// Nom d'utilisateur invalide.
    #[error("nom d'utilisateur invalide")]
    InvalidUsername,
    /// Mot de passe ou passphrase trop long.
    #[error("secret invalide")]
    InvalidSecret,
    /// Taille de terminal hors bornes.
    #[error("taille de terminal invalide")]
    InvalidTerminalSize,
    /// Données à envoyer trop volumineuses pour un seul appel.
    #[error("données trop volumineuses")]
    InputTooLarge,

    /// Le fichier de clé n'a pas pu être lu.
    #[error("lecture du fichier de clé impossible")]
    KeyRead,
    /// Le fichier de clé dépasse la taille maximale admise.
    #[error("fichier de clé trop volumineux")]
    KeyTooLarge,
    /// La clé est chiffrée et aucune passphrase n'a été fournie.
    #[error("passphrase requise")]
    KeyPassphraseRequired,
    /// La passphrase ne déchiffre pas la clé.
    #[error("passphrase incorrecte")]
    KeyWrongPassphrase,
    /// Format de clé illisible ou type non pris en charge.
    #[error("clé invalide ou non prise en charge")]
    KeyInvalid,

    /// Le nom d'hôte ne se résout pas.
    #[error("résolution du nom impossible")]
    DnsFailed,
    /// Le serveur refuse la connexion TCP.
    #[error("connexion refusée")]
    ConnectionRefused,
    /// Délai dépassé (connexion, poignée de main ou authentification).
    #[error("délai dépassé")]
    Timeout,
    /// Échec réseau pendant la connexion.
    #[error("connexion impossible")]
    ConnectFailed,
    /// Échec de la poignée de main SSH (aucun algorithme commun, protocole invalide…).
    #[error("échec de la négociation SSH")]
    HandshakeFailed,

    /// L'utilisateur a refusé une clé d'hôte inconnue.
    #[error("clé d'hôte refusée")]
    HostKeyRejected,
    /// La clé de l'hôte ne correspond pas à celle enregistrée : attaque possible.
    #[error("clé d'hôte modifiée")]
    HostKeyChanged,
    /// La clé de l'hôte est marquée comme révoquée.
    #[error("clé d'hôte révoquée")]
    HostKeyRevoked,
    /// Pas de décision sur la clé d'hôte dans le délai imparti.
    #[error("délai de décision dépassé")]
    HostKeyTimeout,
    /// Lecture ou écriture du fichier `known_hosts` impossible.
    #[error("fichier known_hosts inaccessible")]
    KnownHostsIo,
    /// Aucune décision de clé d'hôte n'est attendue pour cette session.
    #[error("aucune décision attendue")]
    NoPendingDecision,

    /// Authentification refusée par le serveur.
    #[error("authentification refusée")]
    AuthFailed,
    /// Ouverture du canal, du PTY ou du shell refusée.
    #[error("ouverture du terminal distant impossible")]
    ChannelFailed,

    /// Connexion perdue (coupure réseau, keepalive sans réponse, serveur arrêté).
    #[error("connexion perdue")]
    ConnectionLost,
    /// Le shell distant s'est terminé normalement.
    #[error("session terminée")]
    SessionEnded,
    /// La session est déjà fermée.
    #[error("session fermée")]
    SessionClosed,
}

impl SshError {
    /// Code stable, clé de traduction `errors.<code>` côté interface.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidHost => "ssh.invalid_host",
            Self::InvalidPort => "ssh.invalid_port",
            Self::InvalidUsername => "ssh.invalid_username",
            Self::InvalidSecret => "ssh.invalid_secret",
            Self::InvalidTerminalSize => "ssh.invalid_terminal_size",
            Self::InputTooLarge => "ssh.input_too_large",
            Self::KeyRead => "ssh.key_read_failed",
            Self::KeyTooLarge => "ssh.key_too_large",
            Self::KeyPassphraseRequired => "ssh.key_passphrase_required",
            Self::KeyWrongPassphrase => "ssh.key_wrong_passphrase",
            Self::KeyInvalid => "ssh.key_invalid",
            Self::DnsFailed => "ssh.dns_failed",
            Self::ConnectionRefused => "ssh.connection_refused",
            Self::Timeout => "ssh.timeout",
            Self::ConnectFailed => "ssh.connect_failed",
            Self::HandshakeFailed => "ssh.handshake_failed",
            Self::HostKeyRejected => "ssh.host_key_rejected",
            Self::HostKeyChanged => "ssh.host_key_changed",
            Self::HostKeyRevoked => "ssh.host_key_revoked",
            Self::HostKeyTimeout => "ssh.host_key_timeout",
            Self::KnownHostsIo => "ssh.known_hosts_io",
            Self::NoPendingDecision => "ssh.no_pending_decision",
            Self::AuthFailed => "ssh.auth_failed",
            Self::ChannelFailed => "ssh.channel_failed",
            Self::ConnectionLost => "ssh.connection_lost",
            Self::SessionEnded => "ssh.session_ended",
            Self::SessionClosed => "ssh.session_closed",
        }
    }

    /// Toutes les variantes, pour vérifier que chaque code a sa traduction.
    pub const ALL: &'static [Self] = &[
        Self::InvalidHost,
        Self::InvalidPort,
        Self::InvalidUsername,
        Self::InvalidSecret,
        Self::InvalidTerminalSize,
        Self::InputTooLarge,
        Self::KeyRead,
        Self::KeyTooLarge,
        Self::KeyPassphraseRequired,
        Self::KeyWrongPassphrase,
        Self::KeyInvalid,
        Self::DnsFailed,
        Self::ConnectionRefused,
        Self::Timeout,
        Self::ConnectFailed,
        Self::HandshakeFailed,
        Self::HostKeyRejected,
        Self::HostKeyChanged,
        Self::HostKeyRevoked,
        Self::HostKeyTimeout,
        Self::KnownHostsIo,
        Self::NoPendingDecision,
        Self::AuthFailed,
        Self::ChannelFailed,
        Self::ConnectionLost,
        Self::SessionEnded,
        Self::SessionClosed,
    ];
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn codes_are_unique_and_well_formed() {
        let codes: HashSet<_> = SshError::ALL.iter().map(|e| e.code()).collect();
        assert_eq!(codes.len(), SshError::ALL.len());
        for code in codes {
            assert!(code.starts_with("ssh."), "{code}");
            assert!(
                code.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c == '.'),
                "{code}"
            );
        }
    }
}
