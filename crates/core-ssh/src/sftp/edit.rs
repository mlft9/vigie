//! Édition d'un fichier distant avec un éditeur local (SFTP-05).
//!
//! Le fichier est copié dans un dossier temporaire **privé** (nom aléatoire, 0700 sous Unix,
//! fichier 0600), créé dans un dossier propre à l'application (jamais dans le `/tmp` partagé),
//! surveillé, et renvoyé automatiquement à chaque enregistrement. Avant chaque
//! renvoi, la taille et la date du fichier distant sont comparées à celles connues : si
//! quelqu'un l'a modifié entre-temps, rien n'est écrasé ; un conflit est signalé et seul un
//! renvoi forcé ([`EditSession::force_upload`]) écrase. Le dossier temporaire est supprimé
//! quand la session d'édition se termine (fermeture, fin de la session SSH, verrouillage).
//!
//! L'ouverture de l'éditeur (processus local) est faite par l'appelant. Après un arrêt brutal
//! de l'application, les copies restantes sont supprimées au démarrage suivant
//! ([`cleanup_stale`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::names::{RemotePath, check_local_name};
use super::{EntryKind, RemoteFs, RemoteMeta};
use crate::SshError;

/// Taille maximale d'un fichier ouvert en édition.
pub const MAX_EDIT_SIZE: u64 = 16 * 1024 * 1024;
/// Intervalle de surveillance du fichier local.
pub const DEFAULT_POLL: Duration = Duration::from_secs(1);
/// Préfixe des dossiers temporaires d'édition.
const DIR_PREFIX: &str = "younji-edit-";

/// Supprime les copies d'édition laissées par une exécution précédente (arrêt brutal, fichier
/// verrouillé par l'éditeur). `parent` est le dossier propre à l'application passé à
/// [`EditSession::start`] ; seuls ses sous-dossiers `younji-edit-*` (pas les liens) sont
/// supprimés. Renvoie le nombre de dossiers supprimés.
pub fn cleanup_stale(parent: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        let ours = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(DIR_PREFIX));
        if is_dir && ours && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Événement d'une session d'édition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditEvent {
    /// Modification renvoyée au serveur.
    Uploaded,
    /// Le fichier distant a changé depuis son ouverture : renvoi suspendu.
    Conflict,
    /// Échec d'un renvoi (connexion perdue, droits…).
    Failed(SshError),
}

/// Destinataire des événements d'une session d'édition.
pub trait EditSink: Send + Sync + 'static {
    /// Événement.
    fn event(&self, event: EditEvent);
}

/// Pilotage d'une session d'édition. La détruire l'arrête et supprime la copie locale.
pub struct EditSession {
    cancel: CancellationToken,
    force: Arc<Notify>,
    local: PathBuf,
}

impl std::fmt::Debug for EditSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditSession")
            .field("closed", &self.cancel.is_cancelled())
            .finish_non_exhaustive()
    }
}

/// Signature d'un fichier pour détecter un changement : taille et date de modification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp(u64, Option<u32>);

impl From<RemoteMeta> for Stamp {
    fn from(meta: RemoteMeta) -> Self {
        Self(meta.size, meta.modified)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LocalStamp(u64, Option<SystemTime>);

fn local_stamp(path: &Path) -> Option<LocalStamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(LocalStamp(meta.len(), meta.modified().ok()))
}

impl EditSession {
    /// Copie le fichier distant dans un dossier temporaire privé, créé dans `parent` (dossier
    /// propre à l'application, accessible au seul utilisateur), et démarre la surveillance.
    /// Renvoie la session et le chemin du fichier local, à ouvrir dans l'éditeur.
    pub async fn start<F: RemoteFs>(
        fs: Arc<F>,
        remote: RemotePath,
        parent: &Path,
        poll: Duration,
        sink: Arc<dyn EditSink>,
    ) -> Result<(Self, PathBuf), SshError> {
        let local_name = remote.file_name().ok_or(SshError::SftpInvalidPath)?;
        check_local_name(local_name)?;

        let meta = fs.stat(&remote).await?;
        if meta.kind != EntryKind::File {
            return Err(SshError::SftpNotFile);
        }
        if meta.size > MAX_EDIT_SIZE {
            return Err(SshError::EditTooLarge);
        }
        let content = read_limited(fs.as_ref(), &remote).await?;
        let baseline = Stamp::from(fs.stat(&remote).await?);

        let dir = tempfile::Builder::new()
            .prefix(DIR_PREFIX)
            .tempdir_in(parent)
            .map_err(|_| SshError::LocalIo)?;
        let local = dir.path().join(local_name);
        let path = local.clone();
        tokio::task::spawn_blocking(move || write_private(&path, &content))
            .await
            .map_err(|_| SshError::LocalIo)??;

        let cancel = CancellationToken::new();
        let force = Arc::new(Notify::new());
        let watcher = Watcher {
            fs,
            remote,
            local: local.clone(),
            baseline,
            last_local: local_stamp(&local),
            pending: None,
            conflicted: false,
            sink,
        };
        tokio::spawn(watcher.run(dir, poll, cancel.clone(), force.clone()));
        Ok((
            Self {
                cancel,
                force,
                local: local.clone(),
            },
            local,
        ))
    }

    /// Chemin de la copie locale.
    pub fn local_path(&self) -> &Path {
        &self.local
    }

    /// Renvoie la copie locale même si le fichier distant a changé (après un conflit).
    pub fn force_upload(&self) {
        self.force.notify_one();
    }

    /// Arrête la surveillance et supprime la copie locale.
    pub fn close(&self) {
        self.cancel.cancel();
    }

    /// Vrai si la session est arrêtée.
    pub fn is_closed(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

impl Drop for EditSession {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Lit un fichier distant entier, sans dépasser [`MAX_EDIT_SIZE`] même s'il grossit.
async fn read_limited<F: RemoteFs>(fs: &F, remote: &RemotePath) -> Result<Vec<u8>, SshError> {
    let reader = fs.open_read(remote).await?;
    let mut content = Vec::new();
    reader
        .take(MAX_EDIT_SIZE + 1)
        .read_to_end(&mut content)
        .await
        .map_err(|_| SshError::ConnectionLost)?;
    if content.len() as u64 > MAX_EDIT_SIZE {
        return Err(SshError::EditTooLarge);
    }
    Ok(content)
}

/// Crée la copie locale, lisible par le seul utilisateur.
fn write_private(path: &Path, content: &[u8]) -> Result<(), SshError> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| SshError::LocalIo)?;
    file.write_all(content).map_err(|_| SshError::LocalIo)?;
    file.sync_all().map_err(|_| SshError::LocalIo)
}

struct Watcher<F: RemoteFs> {
    fs: Arc<F>,
    remote: RemotePath,
    local: PathBuf,
    /// Signature distante connue (à l'ouverture, puis après chaque renvoi).
    baseline: Stamp,
    /// Dernière signature locale renvoyée (ou initiale).
    last_local: Option<LocalStamp>,
    /// Changement local observé, renvoyé quand il est stable sur deux relevés.
    pending: Option<LocalStamp>,
    conflicted: bool,
    sink: Arc<dyn EditSink>,
}

impl<F: RemoteFs> Watcher<F> {
    async fn run(
        mut self,
        dir: tempfile::TempDir,
        poll: Duration,
        cancel: CancellationToken,
        force: Arc<Notify>,
    ) {
        let mut ticker = tokio::time::interval(poll);
        loop {
            tokio::select! {
                biased;
                () = cancel.cancelled() => break,
                () = force.notified() => self.upload(true).await,
                _ = ticker.tick() => self.check().await,
            }
        }
        // Suppression du dossier temporaire et de la copie (même en cas d'erreur).
        drop(dir);
    }

    async fn check(&mut self) {
        let current = local_stamp(&self.local);
        if current.is_none() || current == self.last_local {
            self.pending = None;
            return;
        }
        if self.pending != current {
            // Premier relevé du changement : on attend qu'il soit stable (enregistrement fini).
            self.pending = current;
            return;
        }
        if !self.conflicted {
            self.upload(false).await;
        }
    }

    async fn upload(&mut self, forced: bool) {
        let observed = local_stamp(&self.local);
        if !forced {
            match self.fs.stat(&self.remote).await {
                Ok(meta) if Stamp::from(meta) == self.baseline => {}
                Ok(_) => {
                    self.conflicted = true;
                    self.sink.event(EditEvent::Conflict);
                    return;
                }
                Err(error) => {
                    self.sink.event(EditEvent::Failed(error));
                    return;
                }
            }
        }
        match self.send().await {
            Ok(stamp) => {
                self.baseline = stamp;
                self.last_local = observed;
                self.pending = None;
                self.conflicted = false;
                self.sink.event(EditEvent::Uploaded);
            }
            Err(error) => self.sink.event(EditEvent::Failed(error)),
        }
    }

    async fn send(&self) -> Result<Stamp, SshError> {
        let path = self.local.clone();
        let content = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, SshError> {
            let meta = std::fs::metadata(&path).map_err(|_| SshError::LocalIo)?;
            if meta.len() > MAX_EDIT_SIZE {
                return Err(SshError::EditTooLarge);
            }
            std::fs::read(&path).map_err(|_| SshError::LocalIo)
        })
        .await
        .map_err(|_| SshError::LocalIo)??;
        let mut writer = self.fs.create(&self.remote, false).await?;
        writer
            .write_all(&content)
            .await
            .map_err(|_| SshError::ConnectionLost)?;
        writer
            .shutdown()
            .await
            .map_err(|_| SshError::ConnectionLost)?;
        Ok(Stamp::from(self.fs.stat(&self.remote).await?))
    }
}
