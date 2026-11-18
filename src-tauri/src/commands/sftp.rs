//! Commandes SFTP (SFTP-01 à SFTP-05) : validation des entrées, appel à `core-ssh`, conversion
//! des erreurs. Aucune logique métier ici.
//!
//! Règles (SECURITE.md §4.2, §5, ADR-034) :
//! - les chemins distants sont revalidés (`RemotePath::parse`) ;
//! - **aucun chemin local absolu n'est accepté de l'interface** : un dossier local est choisi
//!   dans la boîte de dialogue ouverte par le cœur et désigné ensuite par un identifiant ;
//!   les chemins relatifs à ce dossier sont confinés par `LocalRoot::resolve` ;
//! - les chemins locaux ne remontent jamais vers l'interface (seulement des noms).

use std::path::PathBuf;
use std::sync::Arc;

use core_ssh::sftp::edit::{DEFAULT_POLL, EditEvent, EditSession, EditSink};
use core_ssh::sftp::local::{LocalListing, LocalRoot};
use core_ssh::sftp::ops::{self, RemoteEntry, RemoteListing};
use core_ssh::sftp::queue::{
    Direction, JobId, JobSpec, TransferEvent, TransferQueue, TransferSink,
};
use core_ssh::sftp::transfer::{
    ConflictAction, ConflictDecision, ConflictPolicy, Download, LocalDest, Upload,
};
use core_ssh::sftp::{EntryKind, RemoteMeta, RemotePath};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, State, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, FilePath};

use crate::editor::{EDIT_SUFFIX, open_in_editor};
use crate::error::AppError;
use crate::sftp_state::{EditId, LocalRootId, MAX_EDITS, SftpEntry, SftpId};
use crate::state::{SessionId, SessionManager};
use crate::vault_state::VaultState;

/// Éléments sélectionnés au plus pour un transfert.
const MAX_SELECTION: usize = 1000;
/// Longueur maximale d'un nom affiché (dossier local choisi).
const MAX_DISPLAY_NAME: usize = 128;

// ---------------------------------------------------------------------- DTO

/// Type d'un élément, pour l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKindDto {
    File,
    Dir,
    Symlink,
    Other,
}

impl From<EntryKind> for EntryKindDto {
    fn from(kind: EntryKind) -> Self {
        match kind {
            EntryKind::File => Self::File,
            EntryKind::Dir => Self::Dir,
            EntryKind::Symlink => Self::Symlink,
            EntryKind::Other => Self::Other,
        }
    }
}

/// Élément d'un dossier distant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntryDto {
    /// Nom exact (pour désigner l'élément) ; à ne pas afficher tel quel.
    name: String,
    /// Nom affichable (caractères de contrôle et invisibles remplacés).
    display_name: String,
    /// Nom suspect ou impropre à une écriture locale.
    suspicious: bool,
    kind: EntryKindDto,
    size: u64,
    modified: Option<u32>,
    permissions: Option<u32>,
    link_target: Option<EntryKindDto>,
}

impl From<RemoteEntry> for RemoteEntryDto {
    fn from(entry: RemoteEntry) -> Self {
        let RemoteMeta {
            kind,
            size,
            modified,
            permissions,
        } = entry.meta;
        Self {
            name: entry.name,
            display_name: entry.display_name,
            suspicious: entry.suspicious,
            kind: kind.into(),
            size,
            modified,
            permissions,
            link_target: entry.link_target.map(Into::into),
        }
    }
}

/// Contenu d'un dossier distant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteListingDto {
    path: String,
    entries: Vec<RemoteEntryDto>,
    truncated: bool,
}

impl From<RemoteListing> for RemoteListingDto {
    fn from(listing: RemoteListing) -> Self {
        Self {
            path: listing.path.as_str().to_owned(),
            entries: listing.entries.into_iter().map(Into::into).collect(),
            truncated: listing.truncated,
        }
    }
}

/// Élément d'un dossier local.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEntryDto {
    name: String,
    kind: EntryKindDto,
    size: u64,
    modified: Option<u64>,
}

/// Contenu d'un dossier local.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalListingDto {
    entries: Vec<LocalEntryDto>,
    truncated: bool,
}

impl From<LocalListing> for LocalListingDto {
    fn from(listing: LocalListing) -> Self {
        Self {
            entries: listing
                .entries
                .into_iter()
                .map(|e| LocalEntryDto {
                    name: e.name,
                    kind: e.kind.into(),
                    size: e.size,
                    modified: e.modified,
                })
                .collect(),
            truncated: listing.truncated,
        }
    }
}

/// Dossier local choisi : identifiant opaque et nom (jamais le chemin).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRootRef {
    id: LocalRootId,
    name: String,
}

/// Événement d'un client SFTP (transferts et éditions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SftpEventDto {
    TransferQueued {
        job: JobId,
        direction: &'static str,
        label: String,
        count: usize,
    },
    TransferStarted {
        job: JobId,
    },
    TransferProgress {
        job: JobId,
        bytes_done: u64,
        bytes_total: u64,
        files_done: u32,
        files_total: u32,
    },
    TransferConflict {
        job: JobId,
        name: String,
    },
    TransferFinished {
        job: JobId,
        transferred: u32,
        skipped: u32,
        ignored: u32,
        refused: u32,
        failed: u32,
        code: Option<&'static str>,
    },
    EditUploaded {
        edit: EditId,
    },
    EditConflict {
        edit: EditId,
    },
    EditFailed {
        edit: EditId,
        code: &'static str,
    },
}

impl From<TransferEvent> for SftpEventDto {
    fn from(event: TransferEvent) -> Self {
        match event {
            TransferEvent::Queued {
                job,
                direction,
                label,
                count,
            } => Self::TransferQueued {
                job,
                direction: match direction {
                    Direction::Download => "download",
                    Direction::Upload => "upload",
                },
                label,
                count,
            },
            TransferEvent::Started { job } => Self::TransferStarted { job },
            TransferEvent::Progress { job, progress } => Self::TransferProgress {
                job,
                bytes_done: progress.bytes_done,
                bytes_total: progress.bytes_total,
                files_done: progress.files_done,
                files_total: progress.files_total,
            },
            TransferEvent::Conflict { job, name } => Self::TransferConflict { job, name },
            TransferEvent::Finished { job, summary } => Self::TransferFinished {
                job,
                transferred: summary.transferred,
                skipped: summary.skipped,
                ignored: summary.ignored,
                refused: summary.refused,
                failed: summary.failed,
                code: summary.error.map(|e| e.code()),
            },
        }
    }
}

struct ChannelSink(Channel<SftpEventDto>);

impl TransferSink for ChannelSink {
    fn event(&self, event: TransferEvent) {
        if self.0.send(event.into()).is_err() {
            tracing::debug!("événement SFTP non délivré (interface fermée)");
        }
    }
}

struct EditChannelSink {
    channel: Channel<SftpEventDto>,
    edit: std::sync::OnceLock<EditId>,
}

impl EditSink for EditChannelSink {
    fn event(&self, event: EditEvent) {
        let Some(&edit) = self.edit.get() else {
            return;
        };
        let dto = match event {
            EditEvent::Uploaded => SftpEventDto::EditUploaded { edit },
            EditEvent::Conflict => SftpEventDto::EditConflict { edit },
            EditEvent::Failed(error) => SftpEventDto::EditFailed {
                edit,
                code: error.code(),
            },
        };
        if self.channel.send(dto).is_err() {
            tracing::debug!("événement d'édition non délivré (interface fermée)");
        }
    }
}

// ---------------------------------------------------------------------- requêtes

/// Politique de conflit demandée par l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictPolicyDto {
    Ask,
    Overwrite,
    Skip,
    Rename,
}

impl From<ConflictPolicyDto> for ConflictPolicy {
    fn from(policy: ConflictPolicyDto) -> Self {
        match policy {
            ConflictPolicyDto::Ask => Self::Ask,
            ConflictPolicyDto::Overwrite => Self::Overwrite,
            ConflictPolicyDto::Skip => Self::Skip,
            ConflictPolicyDto::Rename => Self::Rename,
        }
    }
}

/// Réponse à un conflit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictActionDto {
    Overwrite,
    Skip,
    Rename,
}

impl From<ConflictActionDto> for ConflictAction {
    fn from(action: ConflictActionDto) -> Self {
        match action {
            ConflictActionDto::Overwrite => Self::Overwrite,
            ConflictActionDto::Skip => Self::Skip,
            ConflictActionDto::Rename => Self::Rename,
        }
    }
}

/// Destination d'un téléchargement.
#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DownloadTarget {
    /// Dossier choisi dans la boîte de dialogue du système.
    Pick {},
    /// Dossier (relatif) d'un dossier local déjà choisi.
    Root { root_id: LocalRootId, path: String },
}

/// Source d'un envoi.
#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum UploadSource {
    /// Fichiers choisis dans la boîte de dialogue du système.
    PickFiles {},
    /// Dossier choisi dans la boîte de dialogue du système.
    PickFolder {},
    /// Éléments (relatifs) d'un dossier local déjà choisi.
    Root {
        root_id: LocalRootId,
        paths: Vec<String>,
    },
}

// ---------------------------------------------------------------------- utilitaires

fn remote(path: &str) -> Result<RemotePath, AppError> {
    Ok(RemotePath::parse(path)?)
}

fn ensure_unlocked(vault: &VaultState) -> Result<(), AppError> {
    if vault.is_unlocked() {
        Ok(())
    } else {
        Err(AppError::VAULT_LOCKED)
    }
}

fn entry(state: &SessionManager, sftp_id: SftpId) -> Result<Arc<SftpEntry>, AppError> {
    state.sftp().get(sftp_id)
}

/// Exécute une fonction bloquante (disque local) hors de l'exécuteur.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| AppError::INTERNAL)?
}

fn into_path(picked: FilePath) -> Result<PathBuf, AppError> {
    picked.into_path().map_err(|_| AppError::DIALOG_FAILED)
}

async fn pick_folder(app: &AppHandle, window: &WebviewWindow) -> Result<Option<PathBuf>, AppError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_parent(window)
        .pick_folder(move |picked| {
            let _ = sender.send(picked);
        });
    match receiver.await.map_err(|_| AppError::DIALOG_FAILED)? {
        Some(picked) => Ok(Some(into_path(picked)?)),
        None => Ok(None),
    }
}

async fn pick_files(
    app: &AppHandle,
    window: &WebviewWindow,
) -> Result<Option<Vec<PathBuf>>, AppError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_parent(window)
        .pick_files(move |picked| {
            let _ = sender.send(picked);
        });
    match receiver.await.map_err(|_| AppError::DIALOG_FAILED)? {
        Some(picked) => Ok(Some(
            picked
                .into_iter()
                .map(into_path)
                .collect::<Result<_, _>>()?,
        )),
        None => Ok(None),
    }
}

/// Chemins choisis dans la boîte de dialogue, canonicalisés (liens résolus).
fn canonical_all(paths: Vec<PathBuf>) -> Result<Vec<PathBuf>, AppError> {
    if paths.len() > MAX_SELECTION {
        return Err(AppError::INVALID_INPUT);
    }
    paths
        .into_iter()
        .map(|p| std::fs::canonicalize(p).map_err(|_| AppError::from(core_ssh::SshError::LocalIo)))
        .collect()
}

// ---------------------------------------------------------------------- commandes

/// Ouvre un client SFTP sur une session de terminal connectée (même connexion, sans nouvelle
/// authentification). Les transferts et les éditions de ce client émettent sur `on_event`.
#[tauri::command]
pub async fn sftp_open(
    session_id: SessionId,
    on_event: Channel<SftpEventDto>,
    state: State<'_, SessionManager>,
    vault: State<'_, VaultState>,
) -> Result<SftpId, AppError> {
    ensure_unlocked(&vault)?;
    let session = state.session(session_id)?;
    let client = Arc::new(session.open_sftp().await?);
    let queue = TransferQueue::start(client.clone(), Arc::new(ChannelSink(on_event)));
    let id = state.sftp().insert(SftpEntry {
        session: session_id,
        client,
        queue,
    })?;
    // La session a pu se fermer (ou le coffre se verrouiller) pendant l'ouverture : le client
    // ne doit pas lui survivre.
    if state.session(session_id).is_err() || !vault.is_unlocked() {
        let _ = state.sftp().close(id);
        return Err(AppError::SESSION_NOT_FOUND);
    }
    tracing::debug!(session = session_id, sftp = id, "client SFTP ouvert");
    Ok(id)
}

/// Ferme un client SFTP (transferts annulés, éditions arrêtées).
#[tauri::command]
pub fn sftp_close(sftp_id: SftpId, state: State<'_, SessionManager>) -> Result<(), AppError> {
    state.sftp().close(sftp_id)
}

/// Dossier personnel de l'utilisateur distant.
#[tauri::command]
pub async fn sftp_home(
    sftp_id: SftpId,
    state: State<'_, SessionManager>,
) -> Result<String, AppError> {
    let entry = entry(&state, sftp_id)?;
    Ok(ops::home(entry.client.as_ref()).await?.as_str().to_owned())
}

/// Liste un dossier distant (10 000 entrées au plus).
#[tauri::command]
pub async fn sftp_list(
    sftp_id: SftpId,
    path: String,
    state: State<'_, SessionManager>,
) -> Result<RemoteListingDto, AppError> {
    let path = remote(&path)?;
    let entry = entry(&state, sftp_id)?;
    Ok(ops::list(entry.client.as_ref(), &path).await?.into())
}

/// Crée un dossier distant.
#[tauri::command]
pub async fn sftp_mkdir(
    sftp_id: SftpId,
    path: String,
    state: State<'_, SessionManager>,
) -> Result<(), AppError> {
    let path = remote(&path)?;
    let entry = entry(&state, sftp_id)?;
    Ok(ops::mkdir(entry.client.as_ref(), &path).await?)
}

/// Renomme ou déplace un élément distant, sans jamais écraser.
#[tauri::command]
pub async fn sftp_rename(
    sftp_id: SftpId,
    from: String,
    to: String,
    state: State<'_, SessionManager>,
) -> Result<(), AppError> {
    let (from, to) = (remote(&from)?, remote(&to)?);
    let entry = entry(&state, sftp_id)?;
    Ok(ops::rename(entry.client.as_ref(), &from, &to).await?)
}

/// Supprime un élément distant (récursif pour un dossier, sans suivre les liens).
#[tauri::command]
pub async fn sftp_remove(
    sftp_id: SftpId,
    path: String,
    state: State<'_, SessionManager>,
) -> Result<(), AppError> {
    let path = remote(&path)?;
    let entry = entry(&state, sftp_id)?;
    Ok(ops::remove(entry.client.as_ref(), &path).await?)
}

/// Modifie les droits d'un élément distant (0 à 0o7777).
#[tauri::command]
pub async fn sftp_chmod(
    sftp_id: SftpId,
    path: String,
    mode: u32,
    state: State<'_, SessionManager>,
) -> Result<(), AppError> {
    let path = remote(&path)?;
    let entry = entry(&state, sftp_id)?;
    Ok(ops::chmod(entry.client.as_ref(), &path, mode).await?)
}

/// Ajoute un téléchargement à la file. Renvoie `None` si l'utilisateur annule la boîte de
/// dialogue.
#[tauri::command]
pub async fn sftp_download(
    app: AppHandle,
    window: WebviewWindow,
    sftp_id: SftpId,
    paths: Vec<String>,
    target: DownloadTarget,
    policy: ConflictPolicyDto,
    state: State<'_, SessionManager>,
) -> Result<Option<JobId>, AppError> {
    if paths.is_empty() || paths.len() > MAX_SELECTION {
        return Err(AppError::INVALID_INPUT);
    }
    let sources = paths
        .iter()
        .map(|p| remote(p))
        .collect::<Result<Vec<_>, _>>()?;
    entry(&state, sftp_id)?;
    let dest = match target {
        DownloadTarget::Pick {} => {
            let Some(dir) = pick_folder(&app, &window).await? else {
                return Ok(None);
            };
            blocking(move || {
                let root = LocalRoot::new(&dir)?;
                Ok(LocalDest {
                    dir: root.path().to_path_buf(),
                    root,
                })
            })
            .await?
        }
        DownloadTarget::Root { root_id, path } => {
            let root = state.sftp().root(root_id)?;
            blocking(move || {
                let dir = root.resolve(&path)?;
                Ok(LocalDest { root, dir })
            })
            .await?
        }
    };
    // Le client a pu être fermé pendant la boîte de dialogue : on le relit.
    let entry = entry(&state, sftp_id)?;
    let job = entry.queue.submit(JobSpec::Download(Download {
        sources,
        dest,
        policy: policy.into(),
    }))?;
    Ok(Some(job))
}

/// Ajoute un envoi à la file. Renvoie `None` si l'utilisateur annule la boîte de dialogue.
#[tauri::command]
pub async fn sftp_upload(
    app: AppHandle,
    window: WebviewWindow,
    sftp_id: SftpId,
    source: UploadSource,
    remote_dir: String,
    policy: ConflictPolicyDto,
    state: State<'_, SessionManager>,
) -> Result<Option<JobId>, AppError> {
    let dest = remote(&remote_dir)?;
    entry(&state, sftp_id)?;
    let sources = match source {
        UploadSource::PickFiles {} => match pick_files(&app, &window).await? {
            Some(paths) => blocking(move || canonical_all(paths)).await?,
            None => return Ok(None),
        },
        UploadSource::PickFolder {} => match pick_folder(&app, &window).await? {
            Some(path) => blocking(move || canonical_all(vec![path])).await?,
            None => return Ok(None),
        },
        UploadSource::Root { root_id, paths } => {
            if paths.is_empty() || paths.len() > MAX_SELECTION {
                return Err(AppError::INVALID_INPUT);
            }
            let root = state.sftp().root(root_id)?;
            blocking(move || {
                paths
                    .iter()
                    .map(|p| {
                        // La racine elle-même ne peut pas être envoyée (pas de nom).
                        if p.is_empty() {
                            return Err(AppError::INVALID_INPUT);
                        }
                        Ok(root.resolve(p)?)
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .await?
        }
    };
    if sources.is_empty() {
        return Ok(None);
    }
    let entry = entry(&state, sftp_id)?;
    let job = entry.queue.submit(JobSpec::Upload(Upload {
        sources,
        dest,
        policy: policy.into(),
    }))?;
    Ok(Some(job))
}

/// Annule un transfert (en attente ou en cours).
#[tauri::command]
pub fn sftp_transfer_cancel(
    sftp_id: SftpId,
    job: JobId,
    state: State<'_, SessionManager>,
) -> Result<(), AppError> {
    Ok(entry(&state, sftp_id)?.queue.cancel(job)?)
}

/// Répond au conflit en attente d'un transfert.
#[tauri::command]
pub fn sftp_conflict_decision(
    sftp_id: SftpId,
    job: JobId,
    action: ConflictActionDto,
    apply_to_all: bool,
    state: State<'_, SessionManager>,
) -> Result<(), AppError> {
    let decision = ConflictDecision {
        action: action.into(),
        apply_to_all,
    };
    Ok(entry(&state, sftp_id)?.queue.decide(job, decision)?)
}

/// Ouvre un fichier distant dans l'éditeur de texte local ; chaque enregistrement est renvoyé.
#[tauri::command]
pub async fn sftp_edit(
    sftp_id: SftpId,
    path: String,
    on_event: Channel<SftpEventDto>,
    state: State<'_, SessionManager>,
    vault: State<'_, VaultState>,
) -> Result<EditId, AppError> {
    ensure_unlocked(&vault)?;
    let path = remote(&path)?;
    if state.sftp().edit_count() >= MAX_EDITS {
        return Err(AppError::TOO_MANY_EDITS);
    }
    let entry = entry(&state, sftp_id)?;
    let sink = Arc::new(EditChannelSink {
        channel: on_event,
        edit: std::sync::OnceLock::new(),
    });
    let (session, local) = EditSession::start(
        entry.client.clone(),
        path,
        EDIT_SUFFIX,
        DEFAULT_POLL,
        sink.clone(),
    )
    .await?;
    let id = state.sftp().insert_edit(sftp_id, session)?;
    let _ = sink.edit.set(id);
    if !vault.is_unlocked() {
        let _ = state.sftp().close_edit(id);
        return Err(AppError::VAULT_LOCKED);
    }
    if let Err(error) = open_in_editor(&local) {
        let _ = state.sftp().close_edit(id);
        return Err(error);
    }
    Ok(id)
}

/// Arrête une édition et supprime sa copie locale.
#[tauri::command]
pub fn sftp_edit_close(edit_id: EditId, state: State<'_, SessionManager>) -> Result<(), AppError> {
    state.sftp().close_edit(edit_id)
}

/// Renvoie la copie locale malgré un conflit (le fichier distant est écrasé).
#[tauri::command]
pub fn sftp_edit_force(edit_id: EditId, state: State<'_, SessionManager>) -> Result<(), AppError> {
    state.sftp().with_edit(edit_id, EditSession::force_upload)
}

/// Choisit un dossier local dans la boîte de dialogue du système. Le chemin reste dans le
/// cœur ; l'interface reçoit un identifiant et le nom du dossier.
#[tauri::command]
pub async fn local_pick_root(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, SessionManager>,
    vault: State<'_, VaultState>,
) -> Result<Option<LocalRootRef>, AppError> {
    ensure_unlocked(&vault)?;
    let Some(path) = pick_folder(&app, &window).await? else {
        return Ok(None);
    };
    let root = blocking(move || Ok(LocalRoot::new(&path)?)).await?;
    let name = root.display_name().chars().take(MAX_DISPLAY_NAME).collect();
    let id = state.sftp().remember_root(root)?;
    Ok(Some(LocalRootRef { id, name }))
}

/// Liste un dossier (relatif) d'un dossier local choisi.
#[tauri::command]
pub async fn local_list(
    root_id: LocalRootId,
    path: String,
    state: State<'_, SessionManager>,
) -> Result<LocalListingDto, AppError> {
    let root = state.sftp().root(root_id)?;
    let listing = blocking(move || Ok(root.list(&path)?)).await?;
    Ok(listing.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_strictly_deserialized() {
        let pick: DownloadTarget = serde_json::from_str(r#"{"kind":"pick"}"#).unwrap();
        assert!(matches!(pick, DownloadTarget::Pick {}));
        let root: DownloadTarget =
            serde_json::from_str(r#"{"kind":"root","rootId":3,"path":"a/b"}"#).unwrap();
        assert!(matches!(root, DownloadTarget::Root { root_id: 3, .. }));
        // Un chemin absolu ne peut pas être glissé dans la requête.
        assert!(
            serde_json::from_str::<DownloadTarget>(r#"{"kind":"pick","path":"C:\\"}"#).is_err()
        );
        assert!(
            serde_json::from_str::<DownloadTarget>(r#"{"kind":"absolute","path":"/"}"#).is_err()
        );
        let source: UploadSource =
            serde_json::from_str(r#"{"kind":"root","rootId":1,"paths":["x"]}"#).unwrap();
        assert!(matches!(source, UploadSource::Root { .. }));
        assert!(
            serde_json::from_str::<UploadSource>(r#"{"kind":"pickFiles","paths":["/etc"]}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<ConflictPolicyDto>(r#""ask""#).is_ok());
        assert!(serde_json::from_str::<ConflictPolicyDto>(r#""always""#).is_err());
    }

    #[test]
    fn events_are_serialized_without_local_paths() {
        let dto = SftpEventDto::from(TransferEvent::Finished {
            job: 7,
            summary: core_ssh::sftp::transfer::TransferSummary {
                transferred: 2,
                refused: 1,
                error: Some(core_ssh::SshError::SftpUnsafeName),
                ..Default::default()
            },
        });
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({
                "type": "transferFinished",
                "job": 7,
                "transferred": 2,
                "skipped": 0,
                "ignored": 0,
                "refused": 1,
                "failed": 0,
                "code": "ssh.sftp_unsafe_name"
            })
        );
    }
}
