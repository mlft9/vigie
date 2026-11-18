//! Génération de clés SSH (KEY-01) : Ed25519 par défaut, ECDSA (P-256, P-384, P-521) et
//! RSA 4096 bits. L'aléa vient uniquement du générateur du système (`getrandom`, équivalent
//! d'`OsRng`). La clé produite est prête à être stockée dans le coffre, comme une clé importée
//! (ADR-030) : elle ne quitte jamais le cœur.

use getrandom::SysRng;
use getrandom::rand_core::UnwrapErr;
use russh::keys::{Algorithm, EcdsaCurve, HashAlg, PrivateKey};

use crate::SshError;
use crate::auth::{ImportedKey, prepare_import};

/// Taille des clés RSA générées, en bits (pas de RSA plus court).
pub const RSA_BITS: usize = 4096;

/// Type de clé à générer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyKind {
    /// Ed25519 (choix par défaut).
    #[default]
    Ed25519,
    /// ECDSA sur la courbe NIST P-256.
    EcdsaP256,
    /// ECDSA sur la courbe NIST P-384.
    EcdsaP384,
    /// ECDSA sur la courbe NIST P-521.
    EcdsaP521,
    /// RSA 4096 bits (signatures en SHA-2 uniquement à l'utilisation).
    Rsa4096,
}

impl KeyKind {
    fn algorithm(self) -> Algorithm {
        match self {
            Self::Ed25519 => Algorithm::Ed25519,
            Self::EcdsaP256 => Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP256,
            },
            Self::EcdsaP384 => Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP384,
            },
            Self::EcdsaP521 => Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP521,
            },
            Self::Rsa4096 => Algorithm::Rsa {
                hash: Some(HashAlg::Sha512),
            },
        }
    }
}

/// Génère une clé et la prépare pour le coffre (opération coûteuse pour RSA : à appeler dans
/// `spawn_blocking`).
pub fn generate_key(kind: KeyKind) -> Result<ImportedKey, SshError> {
    // `UnwrapErr` : le générateur du système ne devrait jamais échouer ; s'il échouait, la
    // génération s'arrêterait (panique) plutôt que de produire une clé avec un aléa dégradé.
    let mut rng = UnwrapErr(SysRng);
    let key = match kind {
        KeyKind::Rsa4096 => {
            let keypair = russh::keys::ssh_key::private::RsaKeypair::random(&mut rng, RSA_BITS)
                .map_err(generation_error)?;
            PrivateKey::from(keypair)
        }
        other => PrivateKey::random(&mut rng, other.algorithm()).map_err(generation_error)?,
    };
    prepare_import(&key)
}

fn generation_error(error: russh::keys::ssh_key::Error) -> SshError {
    tracing::debug!(%error, "génération de clé impossible");
    SshError::KeyGenerationFailed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::decode_stored_key;
    use crate::deploy::AuthorizedKeyLine;

    #[test]
    fn generates_each_kind_and_reloads_it() {
        for (kind, algorithm) in [
            (KeyKind::Ed25519, "ssh-ed25519"),
            (KeyKind::EcdsaP256, "ecdsa-sha2-nistp256"),
            (KeyKind::EcdsaP384, "ecdsa-sha2-nistp384"),
            (KeyKind::EcdsaP521, "ecdsa-sha2-nistp521"),
        ] {
            let key = generate_key(kind).unwrap();
            assert_eq!(key.algorithm, algorithm);
            assert!(key.public_key.starts_with(algorithm));
            assert!(key.fingerprint.starts_with("SHA256:"));
            let decoded = decode_stored_key(&key.private_key).unwrap();
            assert_eq!(
                AuthorizedKeyLine::from_private(&decoded).unwrap().as_str(),
                key.public_key
            );
        }
        assert_eq!(KeyKind::default(), KeyKind::Ed25519);
    }

    #[test]
    fn two_generations_differ() {
        let a = generate_key(KeyKind::Ed25519).unwrap();
        let b = generate_key(KeyKind::Ed25519).unwrap();
        assert_ne!(a.public_key, b.public_key);
    }

    #[test]
    fn generates_rsa_4096() {
        let key = generate_key(KeyKind::Rsa4096).unwrap();
        assert_eq!(key.algorithm, "ssh-rsa");
        let decoded = decode_stored_key(&key.private_key).unwrap();
        let rsa = decoded.public_key().key_data().rsa().unwrap();
        assert_eq!(rsa.key_size(), RSA_BITS as u32);
    }
}
