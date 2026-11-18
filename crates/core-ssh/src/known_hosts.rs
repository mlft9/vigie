//! Fichier `known_hosts` propre à l'application, au format OpenSSH (HKY-01 à HKY-03, ADR-006).
//!
//! Règles de vérification (SECURITE.md §4.1) :
//! - hôte jamais vu : [`HostKeyStatus::Unknown`], l'utilisateur doit accepter explicitement ;
//! - clé présente pour cet hôte : [`HostKeyStatus::Trusted`] ;
//! - hôte connu mais clé présentée absente de la liste : [`HostKeyStatus::Changed`], **même si
//!   le type de clé diffère**. Sinon, un homme du milieu pourrait proposer un autre algorithme
//!   que celui enregistré et retomber sur une simple demande de confiance ;
//! - clé marquée `@revoked` : [`HostKeyStatus::Revoked`], refus.
//!
//! Le fichier est écrit par Younji, en clair et sans hachage des noms d'hôtes (le déplacement
//! éventuel dans le coffre sera tranché à l'étape 2). Les lignes hachées (`|1|…`), les motifs
//! (`*`, `?`, `!`) et les lignes `@cert-authority` sont ignorés : seules les correspondances
//! exactes comptent. Chaque ajout réécrit le fichier de façon atomique (fichier temporaire dans
//! le même dossier, `fsync`, renommage), avec des permissions 0600 sous Linux.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use russh::keys::{HashAlg, PublicKey};

use crate::SshError;

/// Taille maximale lue : un fichier plus gros est considéré comme anormal.
const MAX_FILE_SIZE: u64 = 4 * 1024 * 1024;
/// Port SSH par défaut : il n'est pas écrit dans la ligne (`host` au lieu de `[host]:22`).
const DEFAULT_PORT: u16 = 22;

/// Résultat de la vérification d'une clé d'hôte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyStatus {
    /// La clé présentée est enregistrée pour cet hôte.
    Trusted,
    /// Aucune clé n'est enregistrée pour cet hôte.
    Unknown,
    /// L'hôte est connu, mais avec d'autres clés : connexion à refuser.
    Changed {
        /// Clés enregistrées pour cet hôte.
        known: Vec<KeyInfo>,
    },
    /// La clé présentée est révoquée.
    Revoked,
}

/// Description affichable d'une clé publique.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    /// Algorithme, par exemple `ssh-ed25519`.
    pub algorithm: String,
    /// Empreinte `SHA256:…`, identique à `ssh-keygen -lf`.
    pub fingerprint: String,
}

impl KeyInfo {
    /// Décrit une clé publique.
    pub fn of(key: &PublicKey) -> Self {
        Self {
            algorithm: key.algorithm().as_str().to_owned(),
            fingerprint: fingerprint(key),
        }
    }
}

/// Empreinte SHA-256 au format OpenSSH (`SHA256:` + base64 sans remplissage).
pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Accès au fichier `known_hosts` de l'application.
#[derive(Debug)]
pub struct KnownHosts {
    path: PathBuf,
    /// Sérialise les ajouts (deux onglets qui acceptent une clé en même temps).
    write_lock: Mutex<()>,
}

impl KnownHosts {
    /// Crée un accès au fichier indiqué. Le fichier et son dossier sont créés au premier ajout.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Mutex::new(()),
        }
    }

    /// Chemin du fichier.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Vérifie la clé présentée par `host:port`.
    pub fn check(&self, host: &str, port: u16, key: &PublicKey) -> Result<HostKeyStatus, SshError> {
        let content = self.read()?;
        Ok(evaluate(&content, &host_pattern(host, port), key))
    }

    /// Enregistre une clé acceptée par l'utilisateur. Refuse si, entre-temps, une autre clé a
    /// été enregistrée pour cet hôte (ou si la clé est révoquée). Sans effet si la clé est déjà
    /// présente.
    pub fn add(&self, host: &str, port: u16, key: &PublicKey) -> Result<(), SshError> {
        let _guard = self.write_lock.lock().map_err(|_| SshError::KnownHostsIo)?;
        let content = self.read()?;
        let pattern = host_pattern(host, port);
        match evaluate(&content, &pattern, key) {
            HostKeyStatus::Trusted => return Ok(()),
            HostKeyStatus::Changed { .. } => return Err(SshError::HostKeyChanged),
            HostKeyStatus::Revoked => return Err(SshError::HostKeyRevoked),
            HostKeyStatus::Unknown => {}
        }
        let encoded = key.to_openssh().map_err(|_| SshError::KnownHostsIo)?;
        // `to_openssh` ajoute le commentaire éventuel de la clé : on ne garde que type + base64.
        let mut parts = encoded.split_whitespace();
        let (Some(algorithm), Some(data)) = (parts.next(), parts.next()) else {
            return Err(SshError::KnownHostsIo);
        };
        let mut new_content = content;
        if !new_content.is_empty() && !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(&format!("{pattern} {algorithm} {data}\n"));
        self.write_atomic(new_content.as_bytes()).map_err(|error| {
            tracing::warn!(%error, "écriture de known_hosts impossible");
            SshError::KnownHostsIo
        })
    }

    fn read(&self) -> Result<String, SshError> {
        let io_error = |error: io::Error| {
            tracing::warn!(%error, "lecture de known_hosts impossible");
            SshError::KnownHostsIo
        };
        match fs::metadata(&self.path) {
            Ok(meta) if meta.len() > MAX_FILE_SIZE => {
                tracing::warn!("known_hosts trop volumineux, ignoré");
                return Err(SshError::KnownHostsIo);
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(String::new()),
            Err(error) => return Err(io_error(error)),
        }
        fs::read_to_string(&self.path).map_err(io_error)
    }

    fn write_atomic(&self, bytes: &[u8]) -> io::Result<()> {
        let dir = self
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        create_private_dir(dir)?;
        // NamedTempFile est créé en 0600 sous Unix, dans le même dossier que la cible pour que
        // le renommage soit atomique.
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        file.write_all(bytes)?;
        file.as_file().sync_all()?;
        file.persist(&self.path).map_err(|e| e.error)?;
        Ok(())
    }
}

/// Crée le dossier avec des droits restrictifs (0700 sous Linux) s'il n'existe pas.
fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Forme de l'hôte dans `known_hosts` : `host` sur le port 22, `[host]:port` sinon.
pub fn host_pattern(host: &str, port: u16) -> String {
    let host = host.to_ascii_lowercase();
    if port == DEFAULT_PORT {
        host
    } else {
        format!("[{host}]:{port}")
    }
}

/// Une ligne utile du fichier.
struct Entry<'a> {
    marker: Option<&'a str>,
    hosts: &'a str,
    algorithm: &'a str,
    /// `None` si la clé ne se décode pas (type inconnu, ligne abîmée).
    key: Option<PublicKey>,
}

fn parse_line(line: &str) -> Option<Entry<'_>> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut fields = line.split_whitespace();
    let first = fields.next()?;
    let (marker, hosts) = if first.starts_with('@') {
        (Some(first), fields.next()?)
    } else {
        (None, first)
    };
    let algorithm = fields.next()?;
    let key = fields
        .next()
        .and_then(|data| PublicKey::from_openssh(&format!("{algorithm} {data}")).ok());
    Some(Entry {
        marker,
        hosts,
        algorithm,
        key,
    })
}

/// Vrai si la liste d'hôtes de la ligne contient exactement `pattern`.
fn hosts_match(hosts: &str, pattern: &str) -> bool {
    hosts.split(',').any(|candidate| {
        let is_plain = !candidate.starts_with('|') && !candidate.contains(['*', '?', '!']);
        is_plain && candidate.eq_ignore_ascii_case(pattern)
    })
}

fn same_key(a: Option<&PublicKey>, b: &PublicKey) -> bool {
    a.is_some_and(|a| a.key_data() == b.key_data())
}

/// Longueur maximale du nom d'algorithme recopié d'une ligne illisible.
const MAX_ALGORITHM_LEN: usize = 64;

/// Description d'une clé enregistrée. Une clé illisible garde son nom d'algorithme (nettoyé)
/// et une empreinte vide : elle compte quand même comme « connue », pour refuser par défaut.
fn known_info(entry: &Entry<'_>) -> KeyInfo {
    match &entry.key {
        Some(key) => KeyInfo::of(key),
        None => KeyInfo {
            algorithm: entry
                .algorithm
                .chars()
                .filter(|c| c.is_ascii_graphic())
                .take(MAX_ALGORITHM_LEN)
                .collect(),
            fingerprint: String::new(),
        },
    }
}

fn evaluate(content: &str, pattern: &str, presented: &PublicKey) -> HostKeyStatus {
    let mut known = Vec::new();
    let mut trusted = false;
    for entry in content.lines().filter_map(parse_line) {
        match entry.marker {
            // Une clé révoquée l'est quel que soit l'hôte annoncé sur la ligne.
            Some("@revoked") if same_key(entry.key.as_ref(), presented) => {
                return HostKeyStatus::Revoked;
            }
            Some(_) => {}
            None if hosts_match(entry.hosts, pattern) => {
                if same_key(entry.key.as_ref(), presented) {
                    trusted = true;
                }
                // Une ligne illisible qui nomme cet hôte compte comme une clé connue : on refuse
                // par défaut plutôt que de retomber sur une simple demande de confiance.
                known.push(known_info(&entry));
            }
            None => {}
        }
    }
    if trusted {
        HostKeyStatus::Trusted
    } else if known.is_empty() {
        HostKeyStatus::Unknown
    } else {
        HostKeyStatus::Changed { known }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ED25519: &str = include_str!("../tests/fixtures/test_host_ed25519.pub");
    const ECDSA: &str = include_str!("../tests/fixtures/test_host_ecdsa.pub");
    /// Empreintes calculées par `ssh-keygen -lf <fichier> -E sha256`.
    const ED25519_FP: &str = "SHA256:/jxmmiM5tvPdx2D4xYj6R379gd09Gk7pMElRzkN++Go";
    const ECDSA_FP: &str = "SHA256:qaU+yrRILRPTndSIYjSkhZcK+7MjD1ekSr/yHlB+4mM";

    fn key(text: &str) -> PublicKey {
        PublicKey::from_openssh(text.trim()).unwrap()
    }

    fn line(hosts: &str, text: &str) -> String {
        let mut parts = text.split_whitespace();
        format!(
            "{hosts} {} {}\n",
            parts.next().unwrap(),
            parts.next().unwrap()
        )
    }

    fn store(content: &str) -> (tempfile::TempDir, KnownHosts) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        if !content.is_empty() {
            fs::write(&path, content).unwrap();
        }
        (dir, KnownHosts::new(path))
    }

    #[test]
    fn fingerprint_matches_ssh_keygen() {
        assert_eq!(fingerprint(&key(ED25519)), ED25519_FP);
        assert_eq!(fingerprint(&key(ECDSA)), ECDSA_FP);
        assert_eq!(KeyInfo::of(&key(ED25519)).algorithm, "ssh-ed25519");
        assert_eq!(KeyInfo::of(&key(ECDSA)).algorithm, "ecdsa-sha2-nistp256");
    }

    #[test]
    fn formats_host_patterns_like_openssh() {
        assert_eq!(host_pattern("Example.com", 22), "example.com");
        assert_eq!(host_pattern("example.com", 2222), "[example.com]:2222");
        assert_eq!(host_pattern("::1", 2222), "[::1]:2222");
    }

    #[test]
    fn missing_file_means_unknown() {
        let (_dir, kh) = store("");
        assert_eq!(
            kh.check("h", 22, &key(ED25519)).unwrap(),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn known_key_is_trusted() {
        let content = format!("# commentaire\n\n{}", line("other,srv.lab", ED25519));
        let (_dir, kh) = store(&content);
        assert_eq!(
            kh.check("SRV.lab", 22, &key(ED25519)).unwrap(),
            HostKeyStatus::Trusted
        );
        // Même hôte, autre port : inconnu.
        assert_eq!(
            kh.check("srv.lab", 2222, &key(ED25519)).unwrap(),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn non_default_port_uses_brackets() {
        let (_dir, kh) = store(&line("[srv.lab]:2222", ED25519));
        assert_eq!(
            kh.check("srv.lab", 2222, &key(ED25519)).unwrap(),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            kh.check("srv.lab", 22, &key(ED25519)).unwrap(),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn different_key_is_changed_even_with_another_algorithm() {
        let (_dir, kh) = store(&line("srv.lab", ED25519));
        let status = kh.check("srv.lab", 22, &key(ECDSA)).unwrap();
        assert_eq!(
            status,
            HostKeyStatus::Changed {
                known: vec![KeyInfo {
                    algorithm: "ssh-ed25519".into(),
                    fingerprint: ED25519_FP.into(),
                }],
            }
        );
    }

    #[test]
    fn host_with_several_keys_trusts_each_of_them() {
        let content = line("srv.lab", ED25519) + &line("srv.lab", ECDSA);
        let (_dir, kh) = store(&content);
        assert_eq!(
            kh.check("srv.lab", 22, &key(ED25519)).unwrap(),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            kh.check("srv.lab", 22, &key(ECDSA)).unwrap(),
            HostKeyStatus::Trusted
        );
    }

    #[test]
    fn revoked_key_is_refused() {
        let content = line("@revoked *", ED25519) + &line("srv.lab", ED25519);
        let (_dir, kh) = store(&content);
        assert_eq!(
            kh.check("srv.lab", 22, &key(ED25519)).unwrap(),
            HostKeyStatus::Revoked
        );
        assert_eq!(
            kh.add("new.lab", 22, &key(ED25519)),
            Err(SshError::HostKeyRevoked)
        );
    }

    #[test]
    fn ignores_hashed_wildcard_and_cert_authority_lines() {
        let content = line("|1|c2FsdA==|aGFzaA==", ED25519)
            + &line("*.lab", ED25519)
            + &line("!srv.lab", ED25519)
            + &line("@cert-authority srv.lab", ED25519)
            + "autre ssh-ed25519 pas-du-base64\n"
            + "ligne\n";
        let (_dir, kh) = store(&content);
        assert_eq!(
            kh.check("srv.lab", 22, &key(ED25519)).unwrap(),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn unreadable_line_for_the_host_blocks_instead_of_asking() {
        let content = "srv.lab ssh-dss AAAAB3NzaC1kc3MAAACBAP\nsrv.lab ssh-ed25519\n";
        let (_dir, kh) = store(content);
        assert_eq!(
            kh.check("srv.lab", 22, &key(ED25519)).unwrap(),
            HostKeyStatus::Changed {
                known: vec![
                    KeyInfo {
                        algorithm: "ssh-dss".into(),
                        fingerprint: String::new(),
                    },
                    KeyInfo {
                        algorithm: "ssh-ed25519".into(),
                        fingerprint: String::new(),
                    },
                ],
            }
        );
    }

    #[test]
    fn add_writes_openssh_line_without_comment() {
        let (dir, kh) = store("");
        kh.add("srv.lab", 2222, &key(ED25519)).unwrap();
        let written = fs::read_to_string(dir.path().join("known_hosts")).unwrap();
        assert_eq!(written, line("[srv.lab]:2222", ED25519));
        assert_eq!(
            kh.check("srv.lab", 2222, &key(ED25519)).unwrap(),
            HostKeyStatus::Trusted
        );
    }

    #[test]
    fn add_preserves_existing_lines_and_is_idempotent() {
        let (dir, kh) = store("# garde-moi\nother ssh-ed25519 AAAA");
        kh.add("srv.lab", 22, &key(ED25519)).unwrap();
        kh.add("srv.lab", 22, &key(ED25519)).unwrap();
        let written = fs::read_to_string(dir.path().join("known_hosts")).unwrap();
        assert_eq!(
            written,
            format!(
                "# garde-moi\nother ssh-ed25519 AAAA\n{}",
                line("srv.lab", ED25519)
            )
        );
    }

    #[test]
    fn add_refuses_when_another_key_is_known() {
        let (_dir, kh) = store(&line("srv.lab", ED25519));
        assert_eq!(
            kh.add("srv.lab", 22, &key(ECDSA)),
            Err(SshError::HostKeyChanged)
        );
    }

    #[test]
    fn add_creates_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let kh = KnownHosts::new(dir.path().join("sub").join("known_hosts"));
        kh.add("srv.lab", 22, &key(ED25519)).unwrap();
        assert_eq!(
            kh.check("srv.lab", 22, &key(ED25519)).unwrap(),
            HostKeyStatus::Trusted
        );
    }

    #[cfg(unix)]
    #[test]
    fn written_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, kh) = store("");
        kh.add("srv.lab", 22, &key(ED25519)).unwrap();
        let mode = fs::metadata(dir.path().join("known_hosts"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn oversized_file_is_rejected() {
        let (_dir, kh) = store("");
        let big = vec![b'#'; (MAX_FILE_SIZE + 1) as usize];
        fs::write(kh.path(), big).unwrap();
        assert_eq!(
            kh.check("h", 22, &key(ED25519)),
            Err(SshError::KnownHostsIo)
        );
    }
}
