//! Primitives du coffre (SECURITE.md §3.2) : Argon2id pour la KEK, XChaCha20-Poly1305 pour
//! l'enveloppe de la DEK et pour les données, aléa du système d'exploitation.
//!
//! Aucune crypto « maison » : ce module ne fait qu'assembler les crates RustCrypto avec les
//! paramètres imposés.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::AeadInOut;
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use secrecy::{ExposeSecret, SecretBox, SecretString};
use zeroize::Zeroizing;

use crate::VaultError;
use crate::format::{HEADER_LEN, Header, KEY_LEN, NONCE_LEN, TAG_LEN};

/// Clé symétrique de 32 octets (KEK ou DEK), effacée de la mémoire à sa destruction.
pub type SecretKey = SecretBox<[u8; KEY_LEN]>;

/// Octets aléatoires tirés du générateur du système (`getrandom`, équivalent d'`OsRng`).
pub fn random<const N: usize>() -> Result<[u8; N], VaultError> {
    let mut out = [0u8; N];
    getrandom::fill(&mut out).map_err(|error| {
        tracing::error!(%error, "aléa du système indisponible");
        VaultError::RandomUnavailable
    })?;
    Ok(out)
}

/// Nouvelle clé aléatoire (DEK).
pub fn random_key() -> Result<SecretKey, VaultError> {
    let mut failed = false;
    let key = SecretBox::init_with_mut(|key: &mut [u8; KEY_LEN]| {
        failed = getrandom::fill(key).is_err();
    });
    if failed {
        tracing::error!("aléa du système indisponible");
        return Err(VaultError::RandomUnavailable);
    }
    Ok(key)
}

/// Dérive la KEK du mot de passe maître avec Argon2id et les paramètres de l'en-tête.
/// Opération volontairement coûteuse (~64 Mio de mémoire) : à appeler hors de l'exécuteur async.
pub fn derive_kek(password: &SecretString, header: &Header) -> Result<SecretKey, VaultError> {
    let params = header.params.validate()?;
    let params = Params::new(params.m_kib, params.t, params.p, Some(KEY_LEN))
        .map_err(|_| VaultError::KdfParamsRejected)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut result = Ok(());
    let kek = SecretBox::init_with_mut(|kek: &mut [u8; KEY_LEN]| {
        result = argon2.hash_password_into(password.expose_secret().as_bytes(), &header.salt, kek);
    });
    result.map_err(|error| {
        tracing::error!(%error, "échec de la dérivation Argon2id");
        VaultError::KdfParamsRejected
    })?;
    Ok(kek)
}

fn cipher(key: &SecretKey) -> Result<XChaCha20Poly1305, VaultError> {
    // L'état du chiffrement est effacé à sa destruction (fonctionnalité `zeroize`).
    let key: &[u8] = key.expose_secret();
    XChaCha20Poly1305::new_from_slice(key).map_err(|_| VaultError::Io)
}

/// Chiffre `plaintext` sur place. Le tampon doit avoir une capacité d'au moins `len + 16`
/// pour que l'ajout du tag ne provoque pas de réallocation (qui laisserait une copie du clair).
pub fn seal(
    key: &SecretKey,
    nonce: &[u8; NONCE_LEN],
    aad: &[u8; HEADER_LEN],
    mut plaintext: Zeroizing<Vec<u8>>,
) -> Result<Vec<u8>, VaultError> {
    plaintext.reserve_exact(TAG_LEN);
    cipher(key)?
        .encrypt_in_place(&XNonce::from(*nonce), aad, &mut *plaintext)
        .map_err(|_| VaultError::Io)?;
    // Le tampon contient désormais le chiffré : il n'est plus secret.
    Ok(std::mem::take(&mut *plaintext))
}

/// Déchiffre et authentifie. Toute erreur donne `WrongPasswordOrCorrupted`.
pub fn open(
    key: &SecretKey,
    nonce: &[u8; NONCE_LEN],
    aad: &[u8; HEADER_LEN],
    ciphertext: Vec<u8>,
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let mut buffer = Zeroizing::new(ciphertext);
    cipher(key)?
        .decrypt_in_place(&XNonce::from(*nonce), aad, &mut *buffer)
        .map_err(|_| VaultError::WrongPasswordOrCorrupted)?;
    Ok(buffer)
}

/// Chiffre la DEK avec la KEK.
pub fn wrap_key(
    kek: &SecretKey,
    nonce: &[u8; NONCE_LEN],
    aad: &[u8; HEADER_LEN],
    dek: &SecretKey,
) -> Result<[u8; KEY_LEN + TAG_LEN], VaultError> {
    let mut buffer = Zeroizing::new(Vec::with_capacity(KEY_LEN + TAG_LEN));
    buffer.extend_from_slice(dek.expose_secret());
    let sealed = seal(kek, nonce, aad, buffer)?;
    sealed.try_into().map_err(|_| VaultError::Io)
}

/// Déchiffre la DEK. Un mauvais mot de passe donne une KEK fausse, donc un tag invalide.
pub fn unwrap_key(
    kek: &SecretKey,
    nonce: &[u8; NONCE_LEN],
    aad: &[u8; HEADER_LEN],
    wrapped: &[u8; KEY_LEN + TAG_LEN],
) -> Result<SecretKey, VaultError> {
    let plain = open(kek, nonce, aad, wrapped.to_vec())?;
    if plain.len() != KEY_LEN {
        return Err(VaultError::WrongPasswordOrCorrupted);
    }
    Ok(SecretBox::init_with_mut(|dek: &mut [u8; KEY_LEN]| {
        dek.copy_from_slice(&plain);
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{KdfParams, SALT_LEN};

    fn header(salt: u8) -> Header {
        Header {
            params: KdfParams::DEFAULT,
            salt: [salt; SALT_LEN],
        }
    }

    #[test]
    fn random_values_differ() {
        assert_ne!(random::<24>().unwrap(), random::<24>().unwrap());
        assert_ne!(
            random_key().unwrap().expose_secret(),
            random_key().unwrap().expose_secret()
        );
    }

    #[test]
    fn kek_depends_on_password_and_salt() {
        let a = derive_kek(&SecretString::from("mot de passe 1"), &header(1)).unwrap();
        let b = derive_kek(&SecretString::from("mot de passe 1"), &header(1)).unwrap();
        let c = derive_kek(&SecretString::from("mot de passe 2"), &header(1)).unwrap();
        let d = derive_kek(&SecretString::from("mot de passe 1"), &header(2)).unwrap();
        assert_eq!(a.expose_secret(), b.expose_secret());
        assert_ne!(a.expose_secret(), c.expose_secret());
        assert_ne!(a.expose_secret(), d.expose_secret());
    }

    #[test]
    fn kek_uses_argon2id_v13_with_the_documented_parameters() {
        // Recalcul direct avec Argon2id v1.3, m = 65536 Kio, t = 3, p = 4, 32 octets : le test
        // échoue si `derive_kek` dérive silencieusement (autre variante, version ou longueur).
        let kek = derive_kek(&SecretString::from("password"), &header(2)).unwrap();
        let params = Params::new(65536, 3, 4, Some(32)).unwrap();
        let mut out = [0u8; KEY_LEN];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(b"password", &[2u8; SALT_LEN], &mut out)
            .unwrap();
        assert_eq!(kek.expose_secret(), &out);
    }

    #[test]
    fn seal_and_open_round_trip_with_aad() {
        let key = random_key().unwrap();
        let nonce = random::<NONCE_LEN>().unwrap();
        let aad = header(1).encode();
        let sealed = seal(&key, &nonce, &aad, Zeroizing::new(b"donnees".to_vec())).unwrap();
        assert_eq!(sealed.len(), 7 + TAG_LEN);
        assert_eq!(
            &*open(&key, &nonce, &aad, sealed.clone()).unwrap(),
            b"donnees"
        );

        // Autre AAD (en-tête modifié), autre nonce ou autre clé : refus.
        let other_aad = header(9).encode();
        assert_eq!(
            open(&key, &nonce, &other_aad, sealed.clone()).err(),
            Some(VaultError::WrongPasswordOrCorrupted)
        );
        let other_nonce = random::<NONCE_LEN>().unwrap();
        assert!(open(&key, &other_nonce, &aad, sealed.clone()).is_err());
        assert!(open(&random_key().unwrap(), &nonce, &aad, sealed).is_err());
    }

    #[test]
    fn key_wrapping_round_trips() {
        let kek = random_key().unwrap();
        let dek = random_key().unwrap();
        let nonce = random::<NONCE_LEN>().unwrap();
        let aad = header(1).encode();
        let wrapped = wrap_key(&kek, &nonce, &aad, &dek).unwrap();
        let unwrapped = unwrap_key(&kek, &nonce, &aad, &wrapped).unwrap();
        assert_eq!(unwrapped.expose_secret(), dek.expose_secret());
        assert!(unwrap_key(&random_key().unwrap(), &nonce, &aad, &wrapped).is_err());
    }
}
