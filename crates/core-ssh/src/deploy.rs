//! Déploiement d'une clé publique sur un hôte (KEY-03) : ajout idempotent dans
//! `~/.ssh/authorized_keys`, avec des permissions correctes (700 pour le dossier, 600 pour le
//! fichier).
//!
//! Règle (SECURITE.md §4.2) : la clé n'est **jamais** insérée dans une ligne de commande. Le
//! serveur exécute une commande **constante** et reçoit la ligne de clé par l'entrée standard.
//! La ligne est re-dérivée de la clé privée du coffre et revalidée avant l'envoi.
//!
//! Le script vise un serveur Unix (shell POSIX `sh`). Il reste sur une seule ligne, sans
//! apostrophe ni point d'exclamation, pour être interprété de la même façon quel que soit le
//! shell de connexion de l'utilisateur (bash, zsh, dash, fish, tcsh…), qui ne fait que lancer
//! `sh -c '…'`. Un serveur Windows (shell `cmd`) échoue proprement.

use std::time::Duration;

use russh::ChannelMsg;
use russh::client::{self, Handle};
use russh::keys::{PrivateKey, PublicKey};
use tokio::time::timeout;

use crate::SshError;

/// Script exécuté par `sh` sur le serveur. La ligne de clé est lue sur l'entrée standard.
/// Les lignes commentées (`# …`) ne comptent pas comme une clé déjà présente.
pub const DEPLOY_SCRIPT: &str = concat!(
    "umask 077; ",
    "d=\"$HOME/.ssh\"; f=\"$d/authorized_keys\"; ",
    "mkdir -p \"$d\" && chmod 700 \"$d\" && touch \"$f\" && chmod 600 \"$f\" || exit 2; ",
    "IFS= read -r k || exit 3; ",
    "if grep -v \"^[[:space:]]*#\" \"$f\" | grep -qF -- \"$k\"; then echo ",
    "YOUNJI_PRESENT; exit 0; fi; ",
    "if [ -s \"$f\" ] && [ -n \"$(tail -c 1 \"$f\")\" ]; then echo >> \"$f\" || exit 4; fi; ",
    "printf \"%s\\n\" \"$k\" >> \"$f\" || exit 4; ",
    "echo YOUNJI_ADDED"
);

/// Commande envoyée au serveur : constante, sans aucune donnée de la clé.
pub fn deploy_command() -> String {
    format!("sh -c '{DEPLOY_SCRIPT}'")
}

/// Marqueur de sortie : la clé a été ajoutée.
const ADDED: &str = "YOUNJI_ADDED";
/// Marqueur de sortie : la clé était déjà présente.
const PRESENT: &str = "YOUNJI_PRESENT";
/// Sortie standard lue au plus (le script n'écrit qu'un marqueur).
const MAX_OUTPUT: usize = 1024;
/// Longueur maximale d'une ligne de clé publique (RSA 16384 bits ≈ 2,8 Kio).
const MAX_LINE_LEN: usize = 16 * 1024;
/// Algorithmes de clé déployables.
const ALGORITHMS: &[&str] = &[
    "ssh-ed25519",
    "ecdsa-sha2-nistp256",
    "ecdsa-sha2-nistp384",
    "ecdsa-sha2-nistp521",
    "ssh-rsa",
];

/// Résultat d'un déploiement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployOutcome {
    /// La clé a été ajoutée à `authorized_keys`.
    Added,
    /// La clé y figurait déjà : rien n'a été modifié.
    AlreadyPresent,
}

/// Ligne `authorized_keys` validée : `algorithme base64`, sans options ni commentaire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedKeyLine(String);

impl AuthorizedKeyLine {
    /// Ligne de la clé publique correspondant à une clé privée.
    pub fn from_private(key: &PrivateKey) -> Result<Self, SshError> {
        let openssh = key
            .public_key()
            .to_openssh()
            .map_err(|_| SshError::InvalidPublicKey)?;
        let mut fields = openssh.split_whitespace();
        let (Some(algorithm), Some(blob)) = (fields.next(), fields.next()) else {
            return Err(SshError::InvalidPublicKey);
        };
        Self::parse(&format!("{algorithm} {blob}"))
    }

    /// Valide une ligne : deux champs séparés par une espace, algorithme admis, base64
    /// strict, clé publique décodable.
    pub fn parse(line: &str) -> Result<Self, SshError> {
        if line.len() > MAX_LINE_LEN {
            return Err(SshError::InvalidPublicKey);
        }
        let Some((algorithm, blob)) = line.split_once(' ') else {
            return Err(SshError::InvalidPublicKey);
        };
        let blob_ok = !blob.is_empty()
            && blob
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='));
        if !ALGORITHMS.contains(&algorithm) || !blob_ok {
            return Err(SshError::InvalidPublicKey);
        }
        let key = PublicKey::from_openssh(line).map_err(|_| SshError::InvalidPublicKey)?;
        if key.algorithm().as_str() != algorithm {
            return Err(SshError::InvalidPublicKey);
        }
        Ok(Self(line.to_owned()))
    }

    /// Texte de la ligne.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Interprète la sortie et le code de retour du script.
fn outcome(stdout: &[u8], exit_status: Option<u32>) -> Result<DeployOutcome, SshError> {
    if exit_status != Some(0) {
        return Err(SshError::DeployFailed);
    }
    match std::str::from_utf8(stdout).map(str::trim) {
        Ok(ADDED) => Ok(DeployOutcome::Added),
        Ok(PRESENT) => Ok(DeployOutcome::AlreadyPresent),
        _ => Err(SshError::DeployFailed),
    }
}

/// Exécute le déploiement sur une connexion authentifiée. `limit` borne chaque attente réseau.
pub(crate) async fn run<H: client::Handler>(
    handle: &Handle<H>,
    line: &AuthorizedKeyLine,
    limit: Duration,
) -> Result<DeployOutcome, SshError> {
    let failed = |error: russh::Error| {
        tracing::debug!(%error, "déploiement de la clé : erreur de canal");
        SshError::ChannelFailed
    };
    let mut channel = timeout(limit, handle.channel_open_session())
        .await
        .map_err(|_| SshError::Timeout)?
        .map_err(failed)?;
    channel.exec(true, deploy_command()).await.map_err(failed)?;
    let input = format!("{}\n", line.as_str());
    channel.data(input.as_bytes()).await.map_err(failed)?;
    channel.eof().await.map_err(failed)?;

    let mut stdout = Vec::new();
    let mut exit_status = None;
    loop {
        let message = timeout(limit, channel.wait())
            .await
            .map_err(|_| SshError::Timeout)?;
        match message {
            Some(ChannelMsg::Data { data }) => {
                if stdout.len() + data.len() > MAX_OUTPUT {
                    return Err(SshError::DeployFailed);
                }
                stdout.extend_from_slice(&data);
            }
            Some(ChannelMsg::ExitStatus {
                exit_status: status,
            }) => exit_status = Some(status),
            Some(ChannelMsg::Failure) => return Err(SshError::DeployFailed),
            Some(ChannelMsg::Close) | None => break,
            // Sortie d'erreur, fin de flux, accusés : ignorés (jamais journalisés en clair).
            Some(_) => {}
        }
    }
    let result = outcome(&stdout, exit_status);
    tracing::debug!(
        ?exit_status,
        ok = result.is_ok(),
        "déploiement de la clé terminé"
    );
    result
}

#[cfg(test)]
mod tests {
    use russh::keys::Algorithm;

    use super::*;

    fn line_of(algorithm: Algorithm) -> AuthorizedKeyLine {
        let key = PrivateKey::random(&mut rand::rng(), algorithm).unwrap();
        AuthorizedKeyLine::from_private(&key).unwrap()
    }

    #[test]
    fn command_is_constant_and_shell_neutral() {
        let command = deploy_command();
        assert!(command.starts_with("sh -c '") && command.ends_with('\''));
        // Ni apostrophe (fin de la chaîne), ni « ! » (historique de csh), ni retour à la ligne.
        assert!(!DEPLOY_SCRIPT.contains('\''));
        assert!(!DEPLOY_SCRIPT.contains('!'));
        assert!(!DEPLOY_SCRIPT.contains('\n'));
        // Aucune donnée de clé dans la commande : elle passe par l'entrée standard.
        let line = line_of(Algorithm::Ed25519);
        let blob = line.as_str().split(' ').nth(1).unwrap();
        assert!(!command.contains(blob));
        assert!(!command.contains("ssh-ed25519"));
    }

    #[test]
    fn lines_are_derived_without_comment() {
        let line = line_of(Algorithm::Ed25519);
        assert!(line.as_str().starts_with("ssh-ed25519 AAAA"));
        assert_eq!(line.as_str().split(' ').count(), 2);
        let ecdsa = line_of(Algorithm::Ecdsa {
            curve: russh::keys::EcdsaCurve::NistP384,
        });
        assert!(ecdsa.as_str().starts_with("ecdsa-sha2-nistp384 "));
    }

    #[test]
    fn invalid_lines_are_refused() {
        let good = line_of(Algorithm::Ed25519);
        let blob = good.as_str().split(' ').nth(1).unwrap().to_owned();
        for bad in [
            String::new(),
            "ssh-ed25519".to_owned(),
            format!("ssh-ed25519 {blob} commentaire"),
            format!("ssh-ed25519 {blob}\nssh-ed25519 {blob}"),
            format!("ssh-ed25519 {blob};reboot"),
            format!("ssh-dss {blob}"),
            format!("ssh-rsa {blob}"),
            format!("command=\"sh\" ssh-ed25519 {blob}"),
            "ssh-ed25519 AAAA$(id)".to_owned(),
            format!("ssh-ed25519 {}", "A".repeat(MAX_LINE_LEN)),
        ] {
            assert_eq!(
                AuthorizedKeyLine::parse(&bad).unwrap_err(),
                SshError::InvalidPublicKey,
                "{bad:?}"
            );
        }
        assert!(AuthorizedKeyLine::parse(good.as_str()).is_ok());
    }

    #[test]
    fn outcome_requires_success_and_exact_marker() {
        assert_eq!(
            outcome(b"YOUNJI_ADDED\n", Some(0)),
            Ok(DeployOutcome::Added)
        );
        assert_eq!(
            outcome(b"YOUNJI_PRESENT\n", Some(0)),
            Ok(DeployOutcome::AlreadyPresent)
        );
        assert_eq!(
            outcome(b"YOUNJI_ADDED\n", Some(1)),
            Err(SshError::DeployFailed)
        );
        assert_eq!(
            outcome(b"YOUNJI_ADDED\n", None),
            Err(SshError::DeployFailed)
        );
        assert_eq!(outcome(b"", Some(0)), Err(SshError::DeployFailed));
        assert_eq!(
            outcome(b"motd\nYOUNJI_ADDED\n", Some(0)),
            Err(SshError::DeployFailed)
        );
    }

    /// Exécute le vrai script avec le `sh` local, dans un dossier personnel temporaire.
    /// Ignoré (avec un message) si aucun `sh` n'est disponible.
    fn run_script(home: &std::path::Path, line: &str) -> Option<(String, bool)> {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut child = match Command::new("sh")
            .arg("-c")
            .arg(DEPLOY_SCRIPT)
            .env("HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => {
                eprintln!("test ignoré : `sh` introuvable");
                return None;
            }
        };
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("{line}\n").as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        Some((
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            output.status.success(),
        ))
    }

    #[test]
    fn script_is_idempotent_and_keeps_existing_lines() {
        let home = tempfile::tempdir().unwrap();
        let line = line_of(Algorithm::Ed25519);
        let Some((out, ok)) = run_script(home.path(), line.as_str()) else {
            return;
        };
        assert!(ok);
        assert_eq!(out, ADDED);
        let file = home.path().join(".ssh").join("authorized_keys");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            format!("{}\n", line.as_str())
        );

        // Second passage : rien n'est dupliqué.
        let (out, ok) = run_script(home.path(), line.as_str()).unwrap();
        assert!(ok);
        assert_eq!(out, PRESENT);
        assert_eq!(std::fs::read_to_string(&file).unwrap().lines().count(), 1);

        // Fichier existant sans fin de ligne, avec la clé en commentaire et une autre clé
        // dont la ligne porte un commentaire : la nouvelle clé arrive sur sa propre ligne.
        let other = line_of(Algorithm::Ed25519);
        let third = line_of(Algorithm::Ed25519);
        std::fs::write(
            &file,
            format!("# {}\n{} moi@poste", third.as_str(), other.as_str()),
        )
        .unwrap();
        let (out, _) = run_script(home.path(), other.as_str()).unwrap();
        assert_eq!(out, PRESENT, "une clé suivie d'un commentaire est reconnue");
        let (out, _) = run_script(home.path(), third.as_str()).unwrap();
        assert_eq!(out, ADDED, "une clé commentée ne compte pas");
        let content = std::fs::read_to_string(&file).unwrap();
        let lines: Vec<_> = content.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1], format!("{} moi@poste", other.as_str()));
        assert_eq!(lines[2], third.as_str());
    }

    #[cfg(unix)]
    #[test]
    fn script_sets_restrictive_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let ssh = home.path().join(".ssh");
        std::fs::create_dir(&ssh).unwrap();
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let file = ssh.join("authorized_keys");
        std::fs::write(&file, "").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let line = line_of(Algorithm::Ed25519);
        let Some((_, ok)) = run_script(home.path(), line.as_str()) else {
            return;
        };
        assert!(ok);
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&ssh), 0o700);
        assert_eq!(mode(&file), 0o600);
    }
}
