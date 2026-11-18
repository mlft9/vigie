//! Client SFTP réel, sur deux canaux d'une session SSH déjà authentifiée.
//!
//! - `meta` ([`RawSftpSession`]) sert aux métadonnées et aux listages. On n'utilise **pas**
//!   `SftpSession::read_dir`, qui accumule toutes les entrées sans limite : un serveur
//!   malveillant pourrait saturer la mémoire. Nos listages sont bornés en entrées, en requêtes
//!   et en durée.
//! - `files` ([`SftpSession`]) sert au contenu des fichiers, lu et écrit en pipeline.
//!
//! Les paquets reçus sont bornés par `Config::max_packet_len` (vérifié par la crate avant
//! toute allocation) et chaque requête par un délai.

use std::time::Duration;

use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::fs::File;
use russh_sftp::client::{Config, RawSftpSession, SftpSession};
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::timeout;

use super::{EntryKind, RawEntry, RemoteFs, RemoteMeta, RemotePath};
use crate::SshError;

/// Taille maximale d'un paquet SFTP reçu.
const MAX_PACKET: u32 = 256 * 1024;
/// Délai de réponse d'une requête SFTP.
const REQUEST_TIMEOUT_SECS: u64 = 30;
/// Durée maximale d'un listage complet.
const LIST_TIMEOUT: Duration = Duration::from_secs(120);
/// Nombre maximal de requêtes `READDIR` pour un dossier : un serveur qui renverrait des
/// réponses vides à l'infini ne bloque pas le listage.
const MAX_READDIR_CALLS: usize = 20_000;
/// Délai d'initialisation du protocole.
const INIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Client SFTP.
pub struct SftpClient {
    meta: RawSftpSession,
    files: SftpSession,
}

impl std::fmt::Debug for SftpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SftpClient").finish_non_exhaustive()
    }
}

fn config() -> Config {
    Config {
        max_packet_len: MAX_PACKET,
        request_timeout_secs: REQUEST_TIMEOUT_SECS,
        ..Config::default()
    }
}

impl SftpClient {
    /// Initialise le protocole sur deux flux (canaux `sftp` d'une même session).
    pub async fn from_streams<A, B>(meta: A, files: B) -> Result<Self, SshError>
    where
        A: AsyncRead + AsyncWrite + Unpin + Send + 'static,
        B: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let raw = RawSftpSession::new_with_config(meta, config());
        match timeout(INIT_TIMEOUT, raw.init()).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                tracing::debug!(%error, "initialisation SFTP impossible");
                return Err(SshError::SftpUnavailable);
            }
            Err(_) => return Err(SshError::Timeout),
        }
        let files = match timeout(INIT_TIMEOUT, SftpSession::new_with_config(files, config())).await
        {
            Ok(Ok(session)) => session,
            Ok(Err(error)) => {
                tracing::debug!(%error, "initialisation SFTP impossible");
                return Err(SshError::SftpUnavailable);
            }
            Err(_) => return Err(SshError::Timeout),
        };
        Ok(Self { meta: raw, files })
    }

    /// Ferme les deux canaux.
    pub async fn close(&self) {
        let _ = self.files.close().await;
        let _ = self.meta.close_session();
    }
}

/// Traduit une erreur de la crate en code stable, sans recopier le texte du serveur.
fn map_error(error: SftpError) -> SshError {
    match error {
        SftpError::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => SshError::SftpNotFound,
            StatusCode::PermissionDenied => SshError::SftpPermissionDenied,
            StatusCode::NoConnection | StatusCode::ConnectionLost => SshError::ConnectionLost,
            _ => SshError::SftpFailed,
        },
        SftpError::Timeout => SshError::Timeout,
        SftpError::IO(_) => SshError::ConnectionLost,
        SftpError::Limited(_) => SshError::SftpTooManyEntries,
        SftpError::UnexpectedPacket | SftpError::UnexpectedBehavior(_) => {
            tracing::debug!(%error, "réponse SFTP inattendue");
            SshError::SftpFailed
        }
    }
}

/// Type d'après les bits `S_IFMT` des droits (absents : type inconnu).
fn kind_of(attrs: &FileAttributes) -> EntryKind {
    match attrs.permissions.map(|p| p & 0o170_000) {
        Some(0o040_000) => EntryKind::Dir,
        Some(0o100_000) => EntryKind::File,
        Some(0o120_000) => EntryKind::Symlink,
        _ => EntryKind::Other,
    }
}

fn meta_of(attrs: &FileAttributes) -> RemoteMeta {
    RemoteMeta {
        kind: kind_of(attrs),
        size: attrs.size.unwrap_or(0),
        modified: attrs.mtime,
        permissions: attrs.permissions.map(|p| p & 0o7777),
    }
}

impl RemoteFs for SftpClient {
    type Reader = File;
    type Writer = File;

    async fn lstat(&self, path: &RemotePath) -> Result<RemoteMeta, SshError> {
        let attrs = self.meta.lstat(path.as_str()).await.map_err(map_error)?;
        Ok(meta_of(&attrs.attrs))
    }

    async fn stat(&self, path: &RemotePath) -> Result<RemoteMeta, SshError> {
        let attrs = self.meta.stat(path.as_str()).await.map_err(map_error)?;
        Ok(meta_of(&attrs.attrs))
    }

    async fn read_dir(
        &self,
        path: &RemotePath,
        limit: usize,
    ) -> Result<(Vec<RawEntry>, bool), SshError> {
        let handle = self
            .meta
            .opendir(path.as_str())
            .await
            .map_err(map_error)?
            .handle;
        let listing = timeout(LIST_TIMEOUT, async {
            let mut entries = Vec::new();
            for _ in 0..MAX_READDIR_CALLS {
                match self.meta.readdir(handle.as_str()).await {
                    Ok(name) => {
                        for file in name.files {
                            if entries.len() >= limit {
                                return Ok((entries, true));
                            }
                            entries.push(RawEntry {
                                meta: meta_of(&file.attrs),
                                name: file.filename,
                            });
                        }
                    }
                    Err(SftpError::Status(status)) if status.status_code == StatusCode::Eof => {
                        return Ok((entries, false));
                    }
                    Err(error) => return Err(map_error(error)),
                }
            }
            Err(SshError::SftpTooManyEntries)
        })
        .await;
        let _ = self.meta.close(handle).await;
        listing.unwrap_or(Err(SshError::Timeout))
    }

    async fn mkdir(&self, path: &RemotePath) -> Result<(), SshError> {
        self.meta
            .mkdir(path.as_str(), FileAttributes::empty())
            .await
            .map(|_| ())
            .map_err(map_error)
    }

    async fn rmdir(&self, path: &RemotePath) -> Result<(), SshError> {
        self.meta
            .rmdir(path.as_str())
            .await
            .map(|_| ())
            .map_err(map_error)
    }

    async fn remove_file(&self, path: &RemotePath) -> Result<(), SshError> {
        self.meta
            .remove(path.as_str())
            .await
            .map(|_| ())
            .map_err(map_error)
    }

    async fn rename(&self, from: &RemotePath, to: &RemotePath) -> Result<(), SshError> {
        self.meta
            .rename(from.as_str(), to.as_str())
            .await
            .map(|_| ())
            .map_err(map_error)
    }

    async fn chmod(&self, path: &RemotePath, mode: u32) -> Result<(), SshError> {
        let attrs = FileAttributes {
            permissions: Some(mode & 0o7777),
            ..FileAttributes::empty()
        };
        self.meta
            .setstat(path.as_str(), attrs)
            .await
            .map(|_| ())
            .map_err(map_error)
    }

    async fn realpath(&self, path: &str) -> Result<String, SshError> {
        let name = self.meta.realpath(path).await.map_err(map_error)?;
        name.files
            .into_iter()
            .next()
            .map(|f| f.filename)
            .ok_or(SshError::SftpFailed)
    }

    async fn open_read(&self, path: &RemotePath) -> Result<File, SshError> {
        self.files.open(path.as_str()).await.map_err(map_error)
    }

    async fn create(&self, path: &RemotePath, exclusive: bool) -> Result<File, SshError> {
        let flags = OpenFlags::CREATE
            | OpenFlags::WRITE
            | if exclusive {
                OpenFlags::EXCLUDE
            } else {
                OpenFlags::TRUNCATE
            };
        self.files
            .open_with_flags(path.as_str(), flags)
            .await
            .map_err(|error| match map_error(error) {
                // SFTP v3 n'a pas de code « existe déjà » : un échec en mode exclusif en est un.
                SshError::SftpFailed if exclusive => SshError::SftpAlreadyExists,
                other => other,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_come_from_the_mode_bits() {
        let with = |p| FileAttributes {
            permissions: Some(p),
            ..FileAttributes::empty()
        };
        assert_eq!(kind_of(&with(0o040_755)), EntryKind::Dir);
        assert_eq!(kind_of(&with(0o100_644)), EntryKind::File);
        assert_eq!(kind_of(&with(0o120_777)), EntryKind::Symlink);
        assert_eq!(kind_of(&with(0o060_600)), EntryKind::Other);
        assert_eq!(kind_of(&FileAttributes::empty()), EntryKind::Other);
        assert_eq!(meta_of(&with(0o104_755)).permissions, Some(0o4755));
    }
}
