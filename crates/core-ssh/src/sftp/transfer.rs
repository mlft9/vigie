//! Transferts de fichiers et de dossiers (SFTP-03, SFTP-04), en flux.
//!
//! Chaque transfert se fait en deux temps :
//! 1. **Plan** : parcours borné ([`MAX_DEPTH`], [`MAX_TREE_ENTRIES`]) de ce qu'il faut copier.
//!    Côté distant, les liens symboliques ne sont **pas suivis** (attributs `lstat`) et chaque
//!    nom passe par [`check_local_name`] avant d'être retenu : un nom malveillant est refusé,
//!    jamais corrigé. Côté local, les liens ne sont pas suivis non plus.
//! 2. **Copie** : fichier par fichier, avec un tampon fixe (la mémoire ne dépend pas de la
//!    taille), une progression limitée à 10 messages par seconde et une annulation vérifiée à
//!    chaque bloc.
//!
//! **Écriture locale** : chaque fichier est écrit dans un fichier temporaire du dossier de
//! destination puis renommé. Le renommage remplace l'entrée elle-même : un lien symbolique
//! planté dans la destination n'est jamais suivi. Sans écrasement demandé, le renommage
//! refuse d'écraser (`persist_noclobber`). Chaque dossier créé est canonicalisé et doit rester
//! dans la racine choisie. Un transfert annulé ne laisse pas de fichier partiel local.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::local::LocalRoot;
use super::names::{RemotePath, check_local_name, numbered_name};
use super::ops::{MAX_DEPTH, MAX_TREE_ENTRIES};
use super::{EntryKind, RemoteFs, lstat_opt};
use crate::SshError;

/// Taille du tampon de copie.
const CHUNK: usize = 256 * 1024;
/// Intervalle minimal entre deux messages de progression.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
/// Nombre maximal de noms essayés pour « renommer » en cas de conflit.
const MAX_RENAME_ATTEMPTS: u32 = 999;

/// Que faire quand la destination existe déjà.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// Demander à l'utilisateur (par défaut).
    Ask,
    /// Écraser.
    Overwrite,
    /// Ignorer l'élément.
    Skip,
    /// Garder les deux : `nom (1).ext`.
    Rename,
}

/// Réponse de l'utilisateur à un conflit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictAction {
    /// Écraser.
    Overwrite,
    /// Ignorer.
    Skip,
    /// Garder les deux.
    Rename,
}

impl ConflictAction {
    fn as_policy(self) -> ConflictPolicy {
        match self {
            Self::Overwrite => ConflictPolicy::Overwrite,
            Self::Skip => ConflictPolicy::Skip,
            Self::Rename => ConflictPolicy::Rename,
        }
    }
}

/// Décision sur un conflit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConflictDecision {
    /// Action.
    pub action: ConflictAction,
    /// Appliquer aux conflits suivants du même transfert.
    pub apply_to_all: bool,
}

/// Avancement d'un transfert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    /// Octets copiés.
    pub bytes_done: u64,
    /// Octets à copier (peut augmenter si un fichier grossit pendant la copie).
    pub bytes_total: u64,
    /// Fichiers terminés (copiés, ignorés ou en échec).
    pub files_done: u32,
    /// Fichiers à traiter.
    pub files_total: u32,
}

/// Bilan d'un transfert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransferSummary {
    /// Fichiers copiés.
    pub transferred: u32,
    /// Fichiers ignorés à la suite d'un conflit.
    pub skipped: u32,
    /// Liens symboliques et fichiers spéciaux non copiés.
    pub ignored: u32,
    /// Noms distants refusés pour une écriture locale.
    pub refused: u32,
    /// Éléments en échec.
    pub failed: u32,
    /// Première erreur rencontrée, ou annulation (`TransferCancelled`).
    pub error: Option<SshError>,
}

impl TransferSummary {
    fn fail(&mut self, error: SshError) {
        self.failed += 1;
        self.error.get_or_insert(error);
    }
}

/// Retour d'information d'un transfert vers l'appelant.
pub trait TransferReporter: Send + Sync {
    /// Avancement (au plus 10 fois par seconde, plus un dernier message).
    fn progress(&self, progress: Progress);
    /// Conflit sur `name` (nom affichable de la destination) : renvoie la décision.
    fn conflict(&self, name: String)
    -> Pin<Box<dyn Future<Output = ConflictDecision> + Send + '_>>;
}

/// Destination locale d'un téléchargement : un dossier canonicalisé dans une racine choisie.
#[derive(Debug, Clone)]
pub struct LocalDest {
    /// Racine qui confine toutes les écritures.
    pub root: LocalRoot,
    /// Dossier de destination (canonicalisé, dans la racine).
    pub dir: PathBuf,
}

/// Téléchargement d'éléments distants.
#[derive(Debug, Clone)]
pub struct Download {
    /// Éléments sélectionnés (fichiers ou dossiers).
    pub sources: Vec<RemotePath>,
    /// Destination locale.
    pub dest: LocalDest,
    /// Politique de conflit initiale.
    pub policy: ConflictPolicy,
}

/// Envoi d'éléments locaux.
#[derive(Debug, Clone)]
pub struct Upload {
    /// Éléments sélectionnés (chemins canonicalisés, choisis par l'utilisateur).
    pub sources: Vec<PathBuf>,
    /// Dossier distant de destination.
    pub dest: RemotePath,
    /// Politique de conflit initiale.
    pub policy: ConflictPolicy,
}

// ---------------------------------------------------------------------- progression

struct Tracker<'a> {
    progress: Progress,
    last: Option<Instant>,
    /// Aucun octet encore copié pour le fichier en cours.
    file_start: bool,
    reporter: &'a dyn TransferReporter,
}

impl<'a> Tracker<'a> {
    fn new(reporter: &'a dyn TransferReporter, bytes_total: u64, files_total: u32) -> Self {
        let mut tracker = Self {
            progress: Progress {
                bytes_total,
                files_total,
                ..Progress::default()
            },
            last: None,
            file_start: true,
            reporter,
        };
        tracker.emit(true);
        tracker
    }

    /// Octets copiés ; le premier bloc de chaque fichier est toujours signalé.
    fn bytes(&mut self, n: u64) {
        self.progress.bytes_done += n;
        self.progress.bytes_total = self.progress.bytes_total.max(self.progress.bytes_done);
        let first = std::mem::replace(&mut self.file_start, false);
        self.emit(first);
    }

    fn file_done(&mut self) {
        self.progress.files_done += 1;
        self.file_start = true;
        self.emit(false);
    }

    fn emit(&mut self, force: bool) {
        let now = Instant::now();
        if force || self.last.is_none_or(|last| now - last >= PROGRESS_INTERVAL) {
            self.last = Some(now);
            self.reporter.progress(self.progress);
        }
    }
}

/// Copie en flux, annulable à chaque bloc. `read_err` / `write_err` traduisent les erreurs
/// d'E/S de chaque côté (distant ou local).
async fn copy<R, W>(
    reader: &mut R,
    writer: &mut W,
    cancel: &CancellationToken,
    tracker: &mut Tracker<'_>,
    read_err: SshError,
    write_err: SshError,
) -> Result<(), SshError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0u8; CHUNK];
    loop {
        let n = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(SshError::TransferCancelled),
            n = reader.read(&mut buffer) => n.map_err(|_| read_err)?,
        };
        if n == 0 {
            break;
        }
        tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(SshError::TransferCancelled),
            r = writer.write_all(&buffer[..n]) => r.map_err(|_| write_err)?,
        }
        tracker.bytes(n as u64);
    }
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(SshError::TransferCancelled),
        r = writer.shutdown() => r.map_err(|_| write_err),
    }
}

/// Résout un conflit selon la politique en cours (et la met à jour si « pour tous »).
async fn resolve_conflict(
    policy: &mut ConflictPolicy,
    name: &str,
    reporter: &dyn TransferReporter,
    cancel: &CancellationToken,
) -> Result<ConflictAction, SshError> {
    match *policy {
        ConflictPolicy::Overwrite => Ok(ConflictAction::Overwrite),
        ConflictPolicy::Skip => Ok(ConflictAction::Skip),
        ConflictPolicy::Rename => Ok(ConflictAction::Rename),
        ConflictPolicy::Ask => {
            let (shown, _) = super::names::display_name(name);
            let decision = tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(SshError::TransferCancelled),
                d = reporter.conflict(shown) => d,
            };
            if decision.apply_to_all {
                *policy = decision.action.as_policy();
            }
            Ok(decision.action)
        }
    }
}

// ---------------------------------------------------------------------- téléchargement

enum DlKind {
    Dir,
    File(u64),
}

struct DlItem {
    remote: RemotePath,
    /// Noms locaux (validés) depuis le dossier de destination.
    rel: Vec<String>,
    kind: DlKind,
}

/// Plan d'un téléchargement : liste ordonnée (un dossier avant son contenu).
async fn plan_download<F: RemoteFs>(
    fs: &F,
    sources: &[RemotePath],
    summary: &mut TransferSummary,
    cancel: &CancellationToken,
) -> Result<Vec<DlItem>, SshError> {
    let mut items = Vec::new();
    for source in sources {
        let Some(name) = source.file_name() else {
            summary.fail(SshError::SftpInvalidPath);
            continue;
        };
        if check_local_name(name).is_err() {
            summary.refused += 1;
            continue;
        }
        // Un élément sélectionné explicitement est résolu s'il s'agit d'un lien.
        let meta = match fs.stat(source).await {
            Ok(meta) => meta,
            Err(error) => {
                summary.fail(error);
                continue;
            }
        };
        let rel = vec![name.to_owned()];
        match meta.kind {
            EntryKind::File => items.push(DlItem {
                remote: source.clone(),
                rel,
                kind: DlKind::File(meta.size),
            }),
            EntryKind::Dir => {
                items.push(DlItem {
                    remote: source.clone(),
                    rel: rel.clone(),
                    kind: DlKind::Dir,
                });
                walk_remote(fs, source, rel, &mut items, summary, cancel).await?;
            }
            EntryKind::Symlink | EntryKind::Other => summary.ignored += 1,
        }
        if items.len() > MAX_TREE_ENTRIES {
            return Err(SshError::SftpTooManyEntries);
        }
    }
    Ok(items)
}

async fn walk_remote<F: RemoteFs>(
    fs: &F,
    root: &RemotePath,
    root_rel: Vec<String>,
    items: &mut Vec<DlItem>,
    summary: &mut TransferSummary,
    cancel: &CancellationToken,
) -> Result<(), SshError> {
    let mut stack = vec![(root.clone(), root_rel)];
    while let Some((dir, rel)) = stack.pop() {
        if cancel.is_cancelled() {
            return Err(SshError::TransferCancelled);
        }
        if rel.len() > MAX_DEPTH {
            return Err(SshError::SftpTooManyEntries);
        }
        let budget = MAX_TREE_ENTRIES.saturating_sub(items.len());
        let (entries, truncated) = fs.read_dir(&dir, budget).await?;
        if truncated {
            return Err(SshError::SftpTooManyEntries);
        }
        let mut subdirs = Vec::new();
        for entry in entries {
            if entry.name == "." || entry.name == ".." {
                continue;
            }
            let Ok(child) = dir.join(&entry.name) else {
                summary.refused += 1;
                continue;
            };
            if check_local_name(&entry.name).is_err() {
                summary.refused += 1;
                continue;
            }
            let mut child_rel = rel.clone();
            child_rel.push(entry.name);
            match entry.meta.kind {
                EntryKind::File => items.push(DlItem {
                    remote: child,
                    rel: child_rel,
                    kind: DlKind::File(entry.meta.size),
                }),
                EntryKind::Dir => {
                    items.push(DlItem {
                        remote: child.clone(),
                        rel: child_rel.clone(),
                        kind: DlKind::Dir,
                    });
                    subdirs.push((child, child_rel));
                }
                // Liens non suivis : ni boucle, ni fuite vers un autre endroit du serveur.
                EntryKind::Symlink | EntryKind::Other => summary.ignored += 1,
            }
        }
        // Ordre de parcours stable (le dossier vient avant son contenu dans `items`).
        stack.extend(subdirs.into_iter().rev());
    }
    Ok(())
}

/// Chemin local d'un élément du plan, à partir du dossier de destination (les noms ont déjà
/// été validés par `check_local_name`).
fn local_path(dest: &Path, rel: &[String]) -> PathBuf {
    let mut path = dest.to_path_buf();
    for part in rel {
        path.push(part);
    }
    path
}

/// Crée (ou réutilise) un dossier local et vérifie qu'il reste dans la racine.
fn ensure_local_dir(dest: &LocalDest, rel: &[String]) -> Result<(), SshError> {
    let path = local_path(&dest.dir, rel);
    match path.symlink_metadata() {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => {
            // Lien ou fichier : accepté seulement si sa cible canonique est un dossier de la
            // racine (vérifié ci-dessous).
        }
        Err(_) => std::fs::create_dir(&path).map_err(|_| SshError::LocalIo)?,
    }
    let canonical = std::fs::canonicalize(&path).map_err(|_| SshError::LocalIo)?;
    dest.root.confine(&canonical)?;
    if !canonical.is_dir() {
        return Err(SshError::LocalIo);
    }
    Ok(())
}

/// Dossier parent canonicalisé (et confiné) d'un fichier du plan.
fn local_parent(dest: &LocalDest, rel: &[String]) -> Result<PathBuf, SshError> {
    let parent = local_path(&dest.dir, &rel[..rel.len().saturating_sub(1)]);
    let canonical = std::fs::canonicalize(&parent).map_err(|_| SshError::LocalIo)?;
    dest.root.confine(&canonical)?;
    Ok(canonical)
}

/// Télécharge selon le plan. Ne renvoie jamais d'erreur : le bilan la contient.
pub async fn download<F: RemoteFs>(
    fs: &F,
    job: Download,
    reporter: &dyn TransferReporter,
    cancel: &CancellationToken,
) -> TransferSummary {
    let mut summary = TransferSummary::default();
    let items = match plan_download(fs, &job.sources, &mut summary, cancel).await {
        Ok(items) => items,
        Err(error) => {
            summary.error = Some(error);
            return summary;
        }
    };
    let (bytes_total, files_total) =
        items
            .iter()
            .fold((0u64, 0u32), |(b, f), item| match item.kind {
                DlKind::File(size) => (b.saturating_add(size), f + 1),
                DlKind::Dir => (b, f),
            });
    let mut tracker = Tracker::new(reporter, bytes_total, files_total);
    let mut policy = job.policy;
    for item in items {
        if cancel.is_cancelled() {
            summary.error = Some(SshError::TransferCancelled);
            break;
        }
        match item.kind {
            DlKind::Dir => {
                if let Err(error) = ensure_local_dir(&job.dest, &item.rel) {
                    summary.fail(error);
                }
            }
            DlKind::File(_) => {
                let result =
                    download_file(fs, &job.dest, &item, &mut policy, &mut tracker, cancel).await;
                tracker.file_done();
                match result {
                    Ok(true) => summary.transferred += 1,
                    Ok(false) => summary.skipped += 1,
                    Err(SshError::TransferCancelled) => {
                        summary.error = Some(SshError::TransferCancelled);
                        break;
                    }
                    Err(error) => summary.fail(error),
                }
            }
        }
    }
    tracker.emit(true);
    summary
}

/// Télécharge un fichier. `Ok(false)` : ignoré à la suite d'un conflit.
async fn download_file<F: RemoteFs>(
    fs: &F,
    dest: &LocalDest,
    item: &DlItem,
    policy: &mut ConflictPolicy,
    tracker: &mut Tracker<'_>,
    cancel: &CancellationToken,
) -> Result<bool, SshError> {
    let parent = local_parent(dest, &item.rel)?;
    let name = item.rel.last().ok_or(SshError::SftpInvalidPath)?;
    let mut target = parent.join(name);
    let mut overwrite = false;
    if target.symlink_metadata().is_ok() {
        match resolve_conflict(policy, name, tracker.reporter, cancel).await? {
            ConflictAction::Skip => return Ok(false),
            ConflictAction::Overwrite => overwrite = true,
            ConflictAction::Rename => target = free_local_name(&parent, name)?,
        }
    }

    // Fichier temporaire caché dans le dossier de destination (même système de fichiers,
    // renommage atomique), supprimé automatiquement en cas d'erreur ou d'annulation.
    let temp = tempfile::Builder::new()
        .prefix(".younji-")
        .suffix(".part")
        .tempfile_in(&parent)
        .map_err(|_| SshError::LocalIo)?;
    let file = temp.as_file().try_clone().map_err(|_| SshError::LocalIo)?;
    let mut writer = tokio::fs::File::from_std(file);
    let mut reader = fs.open_read(&item.remote).await?;
    copy(
        &mut reader,
        &mut writer,
        cancel,
        tracker,
        SshError::ConnectionLost,
        SshError::LocalIo,
    )
    .await?;
    writer.sync_all().await.map_err(|_| SshError::LocalIo)?;
    drop(writer);
    let persisted = if overwrite {
        temp.persist(&target).map(|_| ())
    } else {
        temp.persist_noclobber(&target).map(|_| ())
    };
    persisted.map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            SshError::SftpAlreadyExists
        } else {
            SshError::LocalIo
        }
    })?;
    mark_from_internet(&target);
    Ok(true)
}

/// Sous Windows, marque le fichier téléchargé comme venant d'Internet (« Mark-of-the-Web »,
/// flux `Zone.Identifier`, zone 3) : SmartScreen et le mode protégé d'Office s'appliquent à
/// un exécutable ou un document récupéré d'un serveur. Au mieux : un système de fichiers sans
/// flux alternatifs (FAT, réseau) n'empêche pas le téléchargement.
#[cfg(windows)]
pub(crate) fn mark_from_internet(path: &Path) {
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    let _ = std::fs::write(PathBuf::from(stream), b"[ZoneTransfer]\r\nZoneId=3\r\n");
}

#[cfg(not(windows))]
pub(crate) fn mark_from_internet(_path: &Path) {}

/// Premier nom libre `nom (n).ext` dans un dossier local.
fn free_local_name(parent: &Path, name: &str) -> Result<PathBuf, SshError> {
    for n in 1..=MAX_RENAME_ATTEMPTS {
        let candidate = numbered_name(name, n);
        if check_local_name(&candidate).is_err() {
            continue;
        }
        let path = parent.join(&candidate);
        if path.symlink_metadata().is_err() {
            return Ok(path);
        }
    }
    Err(SshError::SftpAlreadyExists)
}

// ---------------------------------------------------------------------- envoi

enum UpKind {
    Dir,
    File(u64),
}

struct UpItem {
    local: PathBuf,
    remote: RemotePath,
    kind: UpKind,
}

/// Plan d'un envoi (bloquant : parcours du disque local, à appeler dans `spawn_blocking`).
fn plan_upload(
    sources: &[PathBuf],
    dest: &RemotePath,
    summary: &mut TransferSummary,
) -> Result<Vec<UpItem>, SshError> {
    let mut items = Vec::new();
    // (chemin local, chemin distant, profondeur)
    let mut stack: Vec<(PathBuf, RemotePath, usize)> = Vec::new();
    for source in sources.iter().rev() {
        let Some(name) = source.file_name().and_then(|n| n.to_str()) else {
            summary.refused += 1;
            continue;
        };
        match dest.join(name) {
            Ok(remote) => stack.push((source.clone(), remote, 0)),
            Err(_) => summary.refused += 1,
        }
    }
    while let Some((local, remote, depth)) = stack.pop() {
        if items.len() >= MAX_TREE_ENTRIES || depth > MAX_DEPTH {
            return Err(SshError::SftpTooManyEntries);
        }
        let Ok(meta) = local.symlink_metadata() else {
            summary.fail(SshError::LocalIo);
            continue;
        };
        match EntryKind::of_std(&meta) {
            EntryKind::File => items.push(UpItem {
                local,
                remote,
                kind: UpKind::File(meta.len()),
            }),
            EntryKind::Dir => {
                let Ok(reader) = std::fs::read_dir(&local) else {
                    summary.fail(SshError::LocalIo);
                    continue;
                };
                let mut children = Vec::new();
                for entry in reader.flatten() {
                    let Ok(name) = entry.file_name().into_string() else {
                        summary.refused += 1;
                        continue;
                    };
                    match remote.join(&name) {
                        Ok(child) => children.push((entry.path(), child, depth + 1)),
                        Err(_) => summary.refused += 1,
                    }
                    if children.len() > MAX_TREE_ENTRIES {
                        return Err(SshError::SftpTooManyEntries);
                    }
                }
                children.sort_by(|a, b| a.1.as_str().cmp(b.1.as_str()));
                items.push(UpItem {
                    local,
                    remote,
                    kind: UpKind::Dir,
                });
                stack.extend(children.into_iter().rev());
            }
            // Liens locaux non suivis ; fichiers spéciaux ignorés.
            EntryKind::Symlink | EntryKind::Other => summary.ignored += 1,
        }
    }
    Ok(items)
}

/// Envoie selon le plan. Ne renvoie jamais d'erreur : le bilan la contient.
pub async fn upload<F: RemoteFs>(
    fs: &F,
    job: Upload,
    reporter: &dyn TransferReporter,
    cancel: &CancellationToken,
) -> TransferSummary {
    let Upload {
        sources,
        dest,
        policy,
    } = job;
    let planned = tokio::task::spawn_blocking(move || {
        let mut summary = TransferSummary::default();
        let items = plan_upload(&sources, &dest, &mut summary);
        (items, summary)
    })
    .await;
    let (items, mut summary) = match planned {
        Ok((Ok(items), summary)) => (items, summary),
        Ok((Err(error), mut summary)) => {
            summary.error = Some(error);
            return summary;
        }
        Err(_) => {
            return TransferSummary {
                error: Some(SshError::LocalIo),
                ..TransferSummary::default()
            };
        }
    };
    let (bytes_total, files_total) =
        items
            .iter()
            .fold((0u64, 0u32), |(b, f), item| match item.kind {
                UpKind::File(size) => (b.saturating_add(size), f + 1),
                UpKind::Dir => (b, f),
            });
    let mut tracker = Tracker::new(reporter, bytes_total, files_total);
    let mut policy = policy;
    for item in items {
        if cancel.is_cancelled() {
            summary.error = Some(SshError::TransferCancelled);
            break;
        }
        match item.kind {
            UpKind::Dir => match lstat_opt(fs, &item.remote).await {
                Ok(None) => {
                    if let Err(error) = fs.mkdir(&item.remote).await {
                        summary.fail(error);
                    }
                }
                Ok(Some(meta)) if meta.kind == EntryKind::Dir => {}
                Ok(Some(_)) => summary.fail(SshError::SftpNotDirectory),
                Err(error) => summary.fail(error),
            },
            UpKind::File(_) => {
                let result = upload_file(fs, &item, &mut policy, &mut tracker, cancel).await;
                tracker.file_done();
                match result {
                    Ok(true) => summary.transferred += 1,
                    Ok(false) => summary.skipped += 1,
                    Err(SshError::TransferCancelled) => {
                        summary.error = Some(SshError::TransferCancelled);
                        break;
                    }
                    Err(error) => summary.fail(error),
                }
            }
        }
    }
    tracker.emit(true);
    summary
}

/// Envoie un fichier. `Ok(false)` : ignoré à la suite d'un conflit.
async fn upload_file<F: RemoteFs>(
    fs: &F,
    item: &UpItem,
    policy: &mut ConflictPolicy,
    tracker: &mut Tracker<'_>,
    cancel: &CancellationToken,
) -> Result<bool, SshError> {
    let mut target = item.remote.clone();
    let mut exclusive = true;
    if lstat_opt(fs, &target).await?.is_some() {
        let name = target.file_name().unwrap_or_default().to_owned();
        match resolve_conflict(policy, &name, tracker.reporter, cancel).await? {
            ConflictAction::Skip => return Ok(false),
            ConflictAction::Overwrite => exclusive = false,
            ConflictAction::Rename => target = free_remote_name(fs, &target).await?,
        }
    }
    let mut reader = tokio::fs::File::open(&item.local)
        .await
        .map_err(|_| SshError::LocalIo)?;
    let mut writer = fs.create(&target, exclusive).await?;
    let result = copy(
        &mut reader,
        &mut writer,
        cancel,
        tracker,
        SshError::LocalIo,
        SshError::ConnectionLost,
    )
    .await;
    if result.is_err() && exclusive {
        // Fichier créé par ce transfert : on ne laisse pas de copie partielle. Un fichier
        // écrasé ne peut pas être restauré (limite documentée).
        drop(writer);
        let _ = fs.remove_file(&target).await;
    }
    result.map(|()| true)
}

/// Premier nom libre `nom (n).ext` dans le dossier distant.
async fn free_remote_name<F: RemoteFs>(
    fs: &F,
    target: &RemotePath,
) -> Result<RemotePath, SshError> {
    let parent = target.parent().ok_or(SshError::SftpInvalidPath)?;
    let name = target.file_name().ok_or(SshError::SftpInvalidPath)?;
    for n in 1..=MAX_RENAME_ATTEMPTS {
        let candidate = parent.join(&numbered_name(name, n))?;
        if lstat_opt(fs, &candidate).await?.is_none() {
            return Ok(candidate);
        }
    }
    Err(SshError::SftpAlreadyExists)
}
