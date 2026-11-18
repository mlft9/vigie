//! File de transferts d'un client SFTP (SFTP-04) : les transferts s'exécutent l'un après
//! l'autre dans une tâche dédiée, chacun annulable, avec les conflits posés à l'utilisateur.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use super::RemoteFs;
use super::names::display_name;
use super::transfer::{
    ConflictAction, ConflictDecision, Download, Progress, TransferReporter, TransferSummary,
    Upload, download, upload,
};
use crate::SshError;

/// Identifiant d'un transfert, unique pour le processus.
pub type JobId = u64;

/// Transferts en attente au plus par client.
const MAX_QUEUED: usize = 64;
/// Délai de réponse à un conflit : au-delà, l'élément et les conflits suivants du transfert
/// sont ignorés (jamais écrasés).
const CONFLICT_TIMEOUT: Duration = Duration::from_secs(600);

static NEXT_JOB: AtomicU64 = AtomicU64::new(1);

/// Transfert à exécuter.
#[derive(Debug, Clone)]
pub enum JobSpec {
    /// Téléchargement.
    Download(Download),
    /// Envoi.
    Upload(Upload),
}

/// Sens d'un transfert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Du serveur vers le poste.
    Download,
    /// Du poste vers le serveur.
    Upload,
}

/// Événement de la file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferEvent {
    /// Transfert ajouté à la file.
    Queued {
        /// Transfert.
        job: JobId,
        /// Sens.
        direction: Direction,
        /// Nom affichable du premier élément.
        label: String,
        /// Nombre d'éléments sélectionnés.
        count: usize,
    },
    /// Transfert commencé.
    Started {
        /// Transfert.
        job: JobId,
    },
    /// Avancement.
    Progress {
        /// Transfert.
        job: JobId,
        /// Avancement.
        progress: Progress,
    },
    /// Conflit : attendre [`TransferQueue::decide`].
    Conflict {
        /// Transfert.
        job: JobId,
        /// Nom affichable de l'élément en conflit.
        name: String,
    },
    /// Transfert terminé (réussi, partiel, en échec ou annulé).
    Finished {
        /// Transfert.
        job: JobId,
        /// Bilan.
        summary: TransferSummary,
    },
}

/// Destinataire des événements de la file.
pub trait TransferSink: Send + Sync + 'static {
    /// Événement.
    fn event(&self, event: TransferEvent);
}

#[derive(Default)]
struct JobState {
    cancel: CancellationToken,
    conflict: Option<oneshot::Sender<ConflictDecision>>,
}

type Jobs = Arc<Mutex<HashMap<JobId, JobState>>>;

/// File de transferts. La détruire annule tous les transferts.
pub struct TransferQueue {
    sender: mpsc::Sender<(JobId, JobSpec)>,
    jobs: Jobs,
    sink: Arc<dyn TransferSink>,
    shutdown: CancellationToken,
}

impl std::fmt::Debug for TransferQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferQueue").finish_non_exhaustive()
    }
}

impl TransferQueue {
    /// Démarre la tâche de la file (à appeler dans un contexte tokio).
    pub fn start<F: RemoteFs>(fs: Arc<F>, sink: Arc<dyn TransferSink>) -> Self {
        let (sender, receiver) = mpsc::channel(MAX_QUEUED);
        let jobs = Jobs::default();
        let shutdown = CancellationToken::new();
        tokio::spawn(worker(
            fs,
            receiver,
            jobs.clone(),
            sink.clone(),
            shutdown.clone(),
        ));
        Self {
            sender,
            jobs,
            sink,
            shutdown,
        }
    }

    /// Ajoute un transfert à la file.
    pub fn submit(&self, spec: JobSpec) -> Result<JobId, SshError> {
        let job = NEXT_JOB.fetch_add(1, Ordering::Relaxed);
        let (direction, label, count) = describe(&spec);
        {
            let mut jobs = self.jobs.lock().map_err(|_| SshError::SftpFailed)?;
            jobs.insert(job, JobState::default());
        }
        if self.sender.try_send((job, spec)).is_err() {
            if let Ok(mut jobs) = self.jobs.lock() {
                jobs.remove(&job);
            }
            return Err(SshError::SftpTooManyEntries);
        }
        self.sink.event(TransferEvent::Queued {
            job,
            direction,
            label,
            count,
        });
        Ok(job)
    }

    /// Annule un transfert (en attente ou en cours).
    pub fn cancel(&self, job: JobId) -> Result<(), SshError> {
        let jobs = self.jobs.lock().map_err(|_| SshError::SftpFailed)?;
        let state = jobs.get(&job).ok_or(SshError::TransferNotFound)?;
        state.cancel.cancel();
        Ok(())
    }

    /// Répond au conflit en attente d'un transfert.
    pub fn decide(&self, job: JobId, decision: ConflictDecision) -> Result<(), SshError> {
        let mut jobs = self.jobs.lock().map_err(|_| SshError::SftpFailed)?;
        let sender = jobs
            .get_mut(&job)
            .and_then(|state| state.conflict.take())
            .ok_or(SshError::TransferNotFound)?;
        sender
            .send(decision)
            .map_err(|_| SshError::TransferNotFound)
    }

    /// Annule tous les transferts et arrête la file.
    pub fn shutdown(&self) {
        self.shutdown.cancel();
        if let Ok(jobs) = self.jobs.lock() {
            for state in jobs.values() {
                state.cancel.cancel();
            }
        }
    }
}

impl Drop for TransferQueue {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn describe(spec: &JobSpec) -> (Direction, String, usize) {
    match spec {
        JobSpec::Download(job) => {
            let name = job
                .sources
                .first()
                .and_then(|p| p.file_name())
                .unwrap_or("/");
            (Direction::Download, display_name(name).0, job.sources.len())
        }
        JobSpec::Upload(job) => {
            let name = job
                .sources
                .first()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            (Direction::Upload, display_name(&name).0, job.sources.len())
        }
    }
}

async fn worker<F: RemoteFs>(
    fs: Arc<F>,
    mut receiver: mpsc::Receiver<(JobId, JobSpec)>,
    jobs: Jobs,
    sink: Arc<dyn TransferSink>,
    shutdown: CancellationToken,
) {
    loop {
        let next = tokio::select! {
            biased;
            () = shutdown.cancelled() => None,
            next = receiver.recv() => next,
        };
        let Some((job, spec)) = next else { break };
        let cancel = match jobs.lock() {
            Ok(jobs) => jobs.get(&job).map(|s| s.cancel.clone()),
            Err(_) => None,
        };
        let summary = match cancel {
            Some(cancel) if !cancel.is_cancelled() && !shutdown.is_cancelled() => {
                sink.event(TransferEvent::Started { job });
                let reporter = Reporter {
                    job,
                    jobs: jobs.clone(),
                    sink: sink.clone(),
                };
                match spec {
                    JobSpec::Download(spec) => {
                        download(fs.as_ref(), spec, &reporter, &cancel).await
                    }
                    JobSpec::Upload(spec) => upload(fs.as_ref(), spec, &reporter, &cancel).await,
                }
            }
            _ => TransferSummary {
                error: Some(SshError::TransferCancelled),
                ..TransferSummary::default()
            },
        };
        if let Ok(mut jobs) = jobs.lock() {
            jobs.remove(&job);
        }
        tracing::debug!(
            transferred = summary.transferred,
            failed = summary.failed,
            refused = summary.refused,
            code = summary.error.map(|e| e.code()),
            "transfert terminé"
        );
        sink.event(TransferEvent::Finished { job, summary });
    }
    // File arrêtée : les transferts restants sont annoncés comme annulés.
    receiver.close();
    while let Some((job, _)) = receiver.recv().await {
        sink.event(TransferEvent::Finished {
            job,
            summary: TransferSummary {
                error: Some(SshError::TransferCancelled),
                ..TransferSummary::default()
            },
        });
    }
}

struct Reporter {
    job: JobId,
    jobs: Jobs,
    sink: Arc<dyn TransferSink>,
}

impl TransferReporter for Reporter {
    fn progress(&self, progress: Progress) {
        self.sink.event(TransferEvent::Progress {
            job: self.job,
            progress,
        });
    }

    fn conflict(
        &self,
        name: String,
    ) -> Pin<Box<dyn Future<Output = ConflictDecision> + Send + '_>> {
        Box::pin(async move {
            let skip_all = ConflictDecision {
                action: ConflictAction::Skip,
                apply_to_all: true,
            };
            let (sender, receiver) = oneshot::channel();
            match self.jobs.lock() {
                Ok(mut jobs) => match jobs.get_mut(&self.job) {
                    Some(state) => state.conflict = Some(sender),
                    None => return skip_all,
                },
                Err(_) => return skip_all,
            }
            self.sink.event(TransferEvent::Conflict {
                job: self.job,
                name,
            });
            match timeout(CONFLICT_TIMEOUT, receiver).await {
                Ok(Ok(decision)) => decision,
                _ => {
                    if let Ok(mut jobs) = self.jobs.lock()
                        && let Some(state) = jobs.get_mut(&self.job)
                    {
                        state.conflict = None;
                    }
                    skip_all
                }
            }
        })
    }
}
