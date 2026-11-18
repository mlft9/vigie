//! Configuration du client russh : algorithmes autorisés et délais (SECURITE.md §4.1).
//!
//! La liste des algorithmes est explicite plutôt que de dépendre des défauts de russh, qui
//! peuvent évoluer d'une version à l'autre. Sont exclus : SHA-1 (échange de clés, MAC et
//! signatures `ssh-rsa`), les chiffrements CBC et 3DES, DSA, les groupes Diffie-Hellman
//! faibles ou choisis par le serveur (group exchange), la compression et l'algorithme `none`.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use russh::keys::{Algorithm, EcdsaCurve, HashAlg};
use russh::{Preferred, cipher, client, compression, kex, mac};

/// Échanges de clés, du plus préféré au moins préféré. Hybride post-quantique en tête.
const KEX: &[kex::Name] = &[
    kex::MLKEM768X25519_SHA256,
    kex::CURVE25519,
    kex::CURVE25519_PRE_RFC_8731,
    kex::ECDH_SHA2_NISTP256,
    kex::ECDH_SHA2_NISTP384,
    kex::ECDH_SHA2_NISTP521,
    kex::DH_G16_SHA512,
    kex::DH_G18_SHA512,
    kex::DH_G14_SHA256,
    // Pseudo-algorithmes : annonce des extensions et « strict kex » (parade à Terrapin).
    kex::EXTENSION_SUPPORT_AS_CLIENT,
    kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT,
];

/// Algorithmes de clé d'hôte acceptés. RSA uniquement avec SHA-2.
const HOST_KEYS: &[Algorithm] = &[
    Algorithm::Ed25519,
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP256,
    },
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP384,
    },
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP521,
    },
    Algorithm::Rsa {
        hash: Some(HashAlg::Sha512),
    },
    Algorithm::Rsa {
        hash: Some(HashAlg::Sha256),
    },
];

/// Chiffrements : AEAD en priorité, AES-CTR (avec MAC *-etm) en repli.
const CIPHERS: &[cipher::Name] = &[
    cipher::CHACHA20_POLY1305,
    cipher::AES_256_GCM,
    cipher::AES_128_GCM,
    cipher::AES_256_CTR,
    cipher::AES_192_CTR,
    cipher::AES_128_CTR,
];

/// MAC « encrypt-then-MAC » en SHA-2 uniquement (inutilisés avec un chiffrement AEAD).
const MACS: &[mac::Name] = &[mac::HMAC_SHA512_ETM, mac::HMAC_SHA256_ETM];

/// Pas de compression (surface d'attaque et canaux auxiliaires inutiles pour un terminal).
const COMPRESSION: &[compression::Name] = &[compression::NONE];

/// Délais de connexion et de session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// Résolution DNS et connexion TCP.
    pub connect: Duration,
    /// Poignée de main SSH, hors temps de décision de l'utilisateur.
    pub handshake: Duration,
    /// Temps laissé à l'utilisateur pour accepter ou refuser une clé d'hôte inconnue.
    pub host_key_decision: Duration,
    /// Chaque échange réseau de l'authentification, puis l'ouverture du canal, du PTY et du
    /// shell. Le temps de saisie de l'utilisateur n'y est pas compté.
    pub auth: Duration,
    /// Temps laissé à l'utilisateur pour répondre à une demande keyboard-interactive (code
    /// TOTP…).
    pub auth_prompt: Duration,
    /// Intervalle des keepalives quand le serveur ne dit rien.
    pub keepalive_interval: Duration,
    /// Nombre de keepalives sans réponse avant de déclarer la connexion perdue.
    pub keepalive_max: usize,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(15),
            handshake: Duration::from_secs(20),
            host_key_decision: Duration::from_secs(120),
            auth: Duration::from_secs(30),
            auth_prompt: Duration::from_secs(120),
            keepalive_interval: Duration::from_secs(15),
            keepalive_max: 3,
        }
    }
}

/// Algorithmes négociables par le client.
pub fn preferred() -> Preferred {
    Preferred {
        kex: Cow::Borrowed(KEX),
        key: Cow::Borrowed(HOST_KEYS),
        // Les certificats d'hôte sont hors périmètre : on ne les annonce pas.
        host_key_certificates: Cow::Borrowed(&[]),
        cipher: Cow::Borrowed(CIPHERS),
        mac: Cow::Borrowed(MACS),
        compression: Cow::Borrowed(COMPRESSION),
    }
}

/// Configuration russh du client.
pub(crate) fn client_config(timeouts: &Timeouts) -> Arc<client::Config> {
    Arc::new(client::Config {
        preferred: preferred(),
        keepalive_interval: Some(timeouts.keepalive_interval),
        keepalive_max: timeouts.keepalive_max,
        // La détection des coupures repose sur les keepalives ; une session inactive mais
        // joignable doit rester ouverte.
        inactivity_timeout: None,
        nodelay: true,
        ..client::Config::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_weak_algorithms() {
        let preferred = preferred();
        let names: Vec<String> = preferred
            .kex
            .iter()
            .map(|n| n.as_ref().to_owned())
            .chain(preferred.cipher.iter().map(|n| n.as_ref().to_owned()))
            .chain(preferred.mac.iter().map(|n| n.as_ref().to_owned()))
            .chain(preferred.compression.iter().map(|n| n.as_ref().to_owned()))
            .chain(preferred.key.iter().map(|a| a.to_string()))
            .collect();
        for name in &names {
            let weak = name.contains("sha1")
                || name.contains("cbc")
                || name.contains("3des")
                || name.contains("dss")
                || name.contains("group1-")
                || name.contains("group-exchange")
                || name.contains("zlib")
                || name == "ssh-rsa"
                || name == "hmac-sha2-256"
                || name == "hmac-sha2-512";
            assert!(!weak, "algorithme faible autorisé : {name}");
        }
        assert!(!preferred.cipher.iter().any(|c| c.as_ref() == "none"));
        assert!(!preferred.kex.iter().any(|k| k.as_ref() == "none"));
        assert_eq!(preferred.compression.len(), 1);
    }

    #[test]
    fn prefers_post_quantum_hybrid_kex_and_strict_kex() {
        let preferred = preferred();
        assert_eq!(preferred.kex[0].as_ref(), "mlkem768x25519-sha256");
        assert!(
            preferred
                .kex
                .iter()
                .any(|k| k.as_ref() == "kex-strict-c-v00@openssh.com")
        );
    }

    #[test]
    fn config_enables_keepalive() {
        let timeouts = Timeouts::default();
        let config = client_config(&timeouts);
        assert_eq!(config.keepalive_interval, Some(Duration::from_secs(15)));
        assert_eq!(config.keepalive_max, 3);
        assert!(config.inactivity_timeout.is_none());
    }
}
