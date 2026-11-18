//! Moyens d'authentification (AUTH-01, AUTH-02, AUTH-03) et chargement des clés privées.
//!
//! Les secrets restent dans des types qui s'effacent de la mémoire : `SecretString` pour le mot
//! de passe et la passphrase, `Zeroizing` pour le contenu brut du fichier de clé, et la clé
//! décodée (`ssh_key::PrivateKey`) efface ses données à sa destruction. Aucun de ces types
//! n'a de `Debug` qui expose le secret.
//!
//! Limite connue : russh reçoit le mot de passe sous forme de `String` ordinaire pendant
//! l'authentification ; cette copie n'est pas effacée par nous.

use std::fmt;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use russh::keys::{self, PrivateKey};
use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroizing;

use crate::SshError;

/// Longueur maximale d'un mot de passe ou d'une passphrase, en octets.
pub const MAX_SECRET_LEN: usize = 1024;
/// Taille maximale d'un fichier de clé privée (une clé RSA 16384 bits tient en ~13 Kio).
pub const MAX_KEY_FILE_SIZE: u64 = 64 * 1024;

/// Moyen d'authentification d'une session.
pub enum Credentials {
    /// Mot de passe.
    Password(SecretString),
    /// Clé privée déjà déchiffrée.
    PrivateKey(Arc<PrivateKey>),
    /// Identité du coffre avec clé **et** mot de passe : la clé est essayée d'abord, le mot de
    /// passe ensuite si le serveur la refuse (HST-03).
    KeyThenPassword(Arc<PrivateKey>, SecretString),
    /// Aucun secret fourni d'avance : tout est demandé à l'utilisateur au fil des questions
    /// du serveur (keyboard-interactive, AUTH-03).
    Interactive,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Password(_) => f.write_str("Credentials::Password(<masqué>)"),
            Self::PrivateKey(key) => write!(
                f,
                "Credentials::PrivateKey({}, <masquée>)",
                key.algorithm().as_str()
            ),
            Self::KeyThenPassword(key, _) => write!(
                f,
                "Credentials::KeyThenPassword({}, <masquée>, <masqué>)",
                key.algorithm().as_str()
            ),
            Self::Interactive => f.write_str("Credentials::Interactive"),
        }
    }
}

impl Credentials {
    /// Authentification par mot de passe, après contrôle de la longueur.
    pub fn password(secret: SecretString) -> Result<Self, SshError> {
        validate_secret(&secret)?;
        Ok(Self::Password(secret))
    }
}

/// Vérifie la longueur d'un secret (mot de passe ou passphrase).
pub fn validate_secret(secret: &SecretString) -> Result<(), SshError> {
    let len = secret.expose_secret().len();
    if len == 0 || len > MAX_SECRET_LEN {
        Err(SshError::InvalidSecret)
    } else {
        Ok(())
    }
}

/// Lit et décode une clé privée depuis un fichier (opération bloquante : à appeler dans
/// `spawn_blocking`). Le fichier doit être un fichier ordinaire de 64 Kio au plus.
pub fn load_private_key(
    path: &Path,
    passphrase: Option<&SecretString>,
) -> Result<PrivateKey, SshError> {
    if let Some(passphrase) = passphrase {
        validate_secret(passphrase)?;
    }
    let read_error = |error: std::io::Error| {
        tracing::debug!(%error, "lecture du fichier de clé impossible");
        SshError::KeyRead
    };
    let file = std::fs::File::open(path).map_err(read_error)?;
    let meta = file.metadata().map_err(read_error)?;
    if !meta.is_file() {
        return Err(SshError::KeyRead);
    }
    if meta.len() > MAX_KEY_FILE_SIZE {
        return Err(SshError::KeyTooLarge);
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(meta.len() as usize));
    // La limite protège aussi contre un fichier qui grossit entre `metadata` et la lecture.
    file.take(MAX_KEY_FILE_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(read_error)?;
    if bytes.len() as u64 > MAX_KEY_FILE_SIZE {
        return Err(SshError::KeyTooLarge);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| SshError::KeyInvalid)?;
    decode_private_key(text, passphrase)
}

/// Clé privée prête à être stockée dans le coffre (KEY-02).
pub struct ImportedKey {
    /// Algorithme (`ssh-ed25519`…).
    pub algorithm: String,
    /// Empreinte `SHA256:…`.
    pub fingerprint: String,
    /// Clé publique au format OpenSSH (une ligne, sans commentaire).
    pub public_key: String,
    /// Clé privée au format OpenSSH, **non chiffrée** : c'est le coffre qui la protège.
    pub private_key: SecretString,
}

impl fmt::Debug for ImportedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportedKey")
            .field("algorithm", &self.algorithm)
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

/// Lit une clé privée (chiffrée ou non), puis la ré-encode au format OpenSSH sans passphrase
/// pour la stocker dans le coffre (opération bloquante : `spawn_blocking`).
pub fn import_private_key(
    path: &Path,
    passphrase: Option<&SecretString>,
) -> Result<ImportedKey, SshError> {
    let key = load_private_key(path, passphrase)?;
    prepare_import(&key)
}

/// Ré-encode une clé déjà déchiffrée pour le coffre.
pub fn prepare_import(key: &PrivateKey) -> Result<ImportedKey, SshError> {
    let encoded = key
        .to_openssh(keys::ssh_key::LineEnding::LF)
        .map_err(|_| SshError::KeyInvalid)?;
    // Copie de taille exacte (aucune réallocation dans `SecretString`) ; le tampon d'origine,
    // un `Zeroizing<String>`, est effacé à sa destruction.
    let private_key = SecretString::from(String::from(encoded.as_str()));
    let public = key.public_key();
    let openssh = public.to_openssh().map_err(|_| SshError::KeyInvalid)?;
    // Sans le commentaire : il pourrait contenir un nom d'utilisateur ou de machine.
    let public_key = openssh
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    let info = crate::KeyInfo::of(public);
    Ok(ImportedKey {
        algorithm: info.algorithm,
        fingerprint: info.fingerprint,
        public_key,
        private_key,
    })
}

/// Décode une clé stockée dans le coffre (format OpenSSH, non chiffrée).
pub fn decode_stored_key(text: &SecretString) -> Result<PrivateKey, SshError> {
    decode_private_key(text.expose_secret(), None)
}

/// Décode une clé privée (OpenSSH, PEM PKCS#1/PKCS#8, PuTTY), chiffrée ou non.
pub fn decode_private_key(
    text: &str,
    passphrase: Option<&SecretString>,
) -> Result<PrivateKey, SshError> {
    let passphrase = passphrase.map(ExposeSecret::expose_secret);
    keys::decode_secret_key(text, passphrase).map_err(|error| {
        let mapped = match (&error, passphrase.is_some()) {
            (keys::Error::KeyIsEncrypted, _) => SshError::KeyPassphraseRequired,
            (keys::Error::SshKey(russh::keys::ssh_key::Error::Encrypted), false) => {
                SshError::KeyPassphraseRequired
            }
            (keys::Error::SshKey(russh::keys::ssh_key::Error::Crypto), true)
            | (keys::Error::Unpad(_), true) => SshError::KeyWrongPassphrase,
            _ => SshError::KeyInvalid,
        };
        // Le message de russh ne contient ni la clé ni la passphrase, seulement le type d'erreur.
        tracing::debug!(error = %error, code = mapped.code(), "décodage de la clé impossible");
        mapped
    })
}

#[cfg(test)]
mod tests {
    use russh::keys::ssh_key::LineEnding;
    use russh::keys::{Algorithm, HashAlg};

    use super::*;

    fn secret(s: &str) -> SecretString {
        SecretString::from(s.to_owned())
    }

    fn generate(algorithm: Algorithm, passphrase: Option<&str>) -> (PrivateKey, String) {
        let mut rng = rand::rng();
        let key = PrivateKey::random(&mut rng, algorithm).unwrap();
        let stored = match passphrase {
            Some(p) => key.encrypt(&mut rng, p).unwrap(),
            None => key.clone(),
        };
        let text = stored.to_openssh(LineEnding::LF).unwrap().to_string();
        (key, text)
    }

    #[test]
    fn decodes_unencrypted_key() {
        let (key, text) = generate(Algorithm::Ed25519, None);
        let decoded = decode_private_key(&text, None).unwrap();
        assert_eq!(decoded.public_key().key_data(), key.public_key().key_data());
        // Une passphrase superflue est ignorée.
        assert!(decode_private_key(&text, Some(&secret("inutile"))).is_ok());
    }

    #[test]
    fn decodes_encrypted_key_with_passphrase() {
        let algorithm = Algorithm::Ecdsa {
            curve: russh::keys::EcdsaCurve::NistP256,
        };
        let (key, text) = generate(algorithm, Some("correct horse"));
        let decoded = decode_private_key(&text, Some(&secret("correct horse"))).unwrap();
        assert_eq!(decoded.public_key().key_data(), key.public_key().key_data());
    }

    #[test]
    fn reports_missing_and_wrong_passphrase() {
        let (_, text) = generate(Algorithm::Ed25519, Some("bonne"));
        assert_eq!(
            decode_private_key(&text, None).unwrap_err(),
            SshError::KeyPassphraseRequired
        );
        assert_eq!(
            decode_private_key(&text, Some(&secret("mauvaise"))).unwrap_err(),
            SshError::KeyWrongPassphrase
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(
            decode_private_key("pas une clé", None).unwrap_err(),
            SshError::KeyInvalid
        );
        let public = include_str!("../tests/fixtures/test_host_ed25519.pub");
        assert_eq!(
            decode_private_key(public, None).unwrap_err(),
            SshError::KeyInvalid
        );
    }

    #[test]
    fn loads_key_from_file_with_limits() {
        let dir = tempfile::tempdir().unwrap();
        let (key, text) = generate(Algorithm::Ed25519, Some("pass"));
        let path = dir.path().join("cle");
        std::fs::write(&path, &text).unwrap();
        let loaded = load_private_key(&path, Some(&secret("pass"))).unwrap();
        assert_eq!(
            loaded.public_key().fingerprint(HashAlg::Sha256),
            key.public_key().fingerprint(HashAlg::Sha256)
        );

        let big = dir.path().join("gros");
        std::fs::write(&big, vec![b'a'; (MAX_KEY_FILE_SIZE + 1) as usize]).unwrap();
        assert_eq!(
            load_private_key(&big, None).unwrap_err(),
            SshError::KeyTooLarge
        );

        assert_eq!(
            load_private_key(&dir.path().join("absent"), None).unwrap_err(),
            SshError::KeyRead
        );
        assert_eq!(
            load_private_key(dir.path(), None).unwrap_err(),
            SshError::KeyRead
        );
    }

    #[test]
    fn validates_secret_length() {
        assert_eq!(validate_secret(&secret("")), Err(SshError::InvalidSecret));
        assert_eq!(
            validate_secret(&secret(&"x".repeat(MAX_SECRET_LEN + 1))),
            Err(SshError::InvalidSecret)
        );
        assert!(validate_secret(&secret(&"x".repeat(MAX_SECRET_LEN))).is_ok());
        assert!(Credentials::password(secret("")).is_err());
    }

    #[test]
    fn imported_key_is_reencoded_without_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let (key, text) = generate(Algorithm::Ed25519, Some("pass"));
        let path = dir.path().join("cle");
        std::fs::write(&path, &text).unwrap();
        let imported = import_private_key(&path, Some(&secret("pass"))).unwrap();
        assert_eq!(imported.algorithm, "ssh-ed25519");
        assert_eq!(
            imported.fingerprint,
            key.public_key().fingerprint(HashAlg::Sha256).to_string()
        );
        assert!(imported.public_key.starts_with("ssh-ed25519 AAAA"));
        assert_eq!(imported.public_key.split_whitespace().count(), 2);
        // Relisible sans passphrase, et identique à l'originale.
        let decoded = decode_stored_key(&imported.private_key).unwrap();
        assert_eq!(decoded.public_key().key_data(), key.public_key().key_data());
        assert!(!format!("{imported:?}").contains("PRIVATE"));
        assert_eq!(
            import_private_key(&path, None).unwrap_err(),
            SshError::KeyPassphraseRequired
        );
    }

    #[test]
    fn debug_never_shows_secrets() {
        let creds = Credentials::password(secret("hunter2")).unwrap();
        assert!(!format!("{creds:?}").contains("hunter2"));
        let (key, _) = generate(Algorithm::Ed25519, None);
        let creds = Credentials::PrivateKey(Arc::new(key));
        let shown = format!("{creds:?}");
        assert!(shown.contains("ssh-ed25519") && shown.contains("masquée"));
        let (key, _) = generate(Algorithm::Ed25519, None);
        let creds = Credentials::KeyThenPassword(Arc::new(key), secret("hunter2"));
        assert!(!format!("{creds:?}").contains("hunter2"));
    }
}
