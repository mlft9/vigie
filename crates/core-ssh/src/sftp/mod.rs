//! SFTP (SFTP-01 à SFTP-05) : client sur une session SSH déjà authentifiée, opérations sur les
//! fichiers distants, transferts en flux avec conflits, édition avec l'éditeur local.
//!
//! La logique (noms, confinement local, transferts, conflits, édition) est écrite contre le
//! trait [`RemoteFs`] : le client réel ([`SftpClient`]) l'implémente, et les tests utilisent un
//! faux système de fichiers en mémoire. Aucune dépendance à Tauri.

mod client;
pub mod edit;
pub mod local;
#[cfg(test)]
mod memfs;
pub mod names;
pub mod ops;
pub mod queue;
#[cfg(test)]
mod tests;
pub mod transfer;

use std::future::Future;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::SshError;
pub use client::SftpClient;
pub use names::RemotePath;

/// Type d'un élément, sans suivre les liens symboliques.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// Fichier ordinaire.
    File,
    /// Dossier.
    Dir,
    /// Lien symbolique.
    Symlink,
    /// Autre (périphérique, tube, socket…).
    Other,
}

impl EntryKind {
    /// Type d'un élément local (métadonnées obtenues sans suivre les liens).
    pub fn of_std(meta: &std::fs::Metadata) -> Self {
        let kind = meta.file_type();
        if kind.is_symlink() {
            Self::Symlink
        } else if kind.is_dir() {
            Self::Dir
        } else if kind.is_file() {
            Self::File
        } else {
            Self::Other
        }
    }
}

/// Attributs d'un élément distant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteMeta {
    /// Type.
    pub kind: EntryKind,
    /// Taille en octets (0 si inconnue).
    pub size: u64,
    /// Date de modification (secondes depuis l'époque Unix).
    pub modified: Option<u32>,
    /// Droits Unix (12 bits de poids faible : `rwx` et bits spéciaux).
    pub permissions: Option<u32>,
}

/// Élément brut d'un listage distant (nom tel que renvoyé par le serveur, non fiable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEntry {
    /// Nom renvoyé par le serveur.
    pub name: String,
    /// Attributs (sémantique `lstat` : les liens ne sont pas suivis).
    pub meta: RemoteMeta,
}

/// Accès au système de fichiers distant. Toutes les opérations sont bornées dans le temps par
/// l'implémentation.
pub trait RemoteFs: Send + Sync + 'static {
    /// Lecture en flux d'un fichier.
    type Reader: AsyncRead + Unpin + Send + 'static;
    /// Écriture en flux d'un fichier (à fermer par `shutdown` pour attendre les acquittements).
    type Writer: AsyncWrite + Unpin + Send + 'static;

    /// Attributs sans suivre un lien symbolique.
    fn lstat(&self, path: &RemotePath)
    -> impl Future<Output = Result<RemoteMeta, SshError>> + Send;
    /// Attributs en suivant un lien symbolique.
    fn stat(&self, path: &RemotePath) -> impl Future<Output = Result<RemoteMeta, SshError>> + Send;
    /// Entrées d'un dossier, au plus `limit` (au-delà, le second élément est vrai). Les
    /// entrées `.` et `..` peuvent figurer dans le résultat.
    fn read_dir(
        &self,
        path: &RemotePath,
        limit: usize,
    ) -> impl Future<Output = Result<(Vec<RawEntry>, bool), SshError>> + Send;
    /// Crée un dossier.
    fn mkdir(&self, path: &RemotePath) -> impl Future<Output = Result<(), SshError>> + Send;
    /// Supprime un dossier vide.
    fn rmdir(&self, path: &RemotePath) -> impl Future<Output = Result<(), SshError>> + Send;
    /// Supprime un fichier ou un lien (le lien lui-même).
    fn remove_file(&self, path: &RemotePath) -> impl Future<Output = Result<(), SshError>> + Send;
    /// Renomme ou déplace (échoue si la destination existe, comme SFTP v3).
    fn rename(
        &self,
        from: &RemotePath,
        to: &RemotePath,
    ) -> impl Future<Output = Result<(), SshError>> + Send;
    /// Modifie les droits Unix.
    fn chmod(
        &self,
        path: &RemotePath,
        mode: u32,
    ) -> impl Future<Output = Result<(), SshError>> + Send;
    /// Chemin absolu canonique (résolution de `.` : dossier personnel).
    fn realpath(&self, path: &str) -> impl Future<Output = Result<String, SshError>> + Send;
    /// Ouvre un fichier en lecture.
    fn open_read(
        &self,
        path: &RemotePath,
    ) -> impl Future<Output = Result<Self::Reader, SshError>> + Send;
    /// Crée (ou tronque) un fichier. Avec `exclusive`, échoue si le fichier existe déjà.
    fn create(
        &self,
        path: &RemotePath,
        exclusive: bool,
    ) -> impl Future<Output = Result<Self::Writer, SshError>> + Send;
}

/// Attributs d'un élément, ou `None` s'il n'existe pas.
pub(crate) async fn lstat_opt<F: RemoteFs>(
    fs: &F,
    path: &RemotePath,
) -> Result<Option<RemoteMeta>, SshError> {
    match fs.lstat(path).await {
        Ok(meta) => Ok(Some(meta)),
        Err(SshError::SftpNotFound) => Ok(None),
        Err(error) => Err(error),
    }
}
