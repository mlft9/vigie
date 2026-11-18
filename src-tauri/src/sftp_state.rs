//! Registre SFTP : clients ouverts sur les sessions, dossiers locaux choisis par
//! l'utilisateur et éditions en cours.
//!
//! Il appartient au `SessionManager` : fermer une session ferme ses clients SFTP (et leurs
//! transferts et éditions) ; tout fermer (verrouillage, rechargement) oublie aussi les dossiers
//! locaux, qui sont des autorisations accordées à l'interface (ADR-034).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use core_ssh::sftp::SftpClient;
use core_ssh::sftp::edit::EditSession;
use core_ssh::sftp::local::LocalRoot;
use core_ssh::sftp::queue::TransferQueue;

use crate::error::AppError;
use crate::state::SessionId;

/// Clients SFTP ouverts au plus (toutes sessions confondues).
pub const MAX_SFTP_CLIENTS: usize = 64;
/// Dossiers locaux mémorisés au plus ; au-delà, les plus anciens sont oubliés.
const MAX_LOCAL_ROOTS: usize = 32;
/// Éditions simultanées au plus.
pub const MAX_EDITS: usize = 16;

/// Identifiant opaque d'un client SFTP.
pub type SftpId = u64;
/// Identifiant opaque d'un dossier local choisi par l'utilisateur.
pub type LocalRootId = u64;
/// Identifiant opaque d'une édition en cours.
pub type EditId = u64;

/// Client SFTP ouvert sur une session.
pub struct SftpEntry {
    /// Session dont le client utilise la connexion.
    pub session: SessionId,
    /// Client.
    pub client: Arc<SftpClient>,
    /// File de transferts du client.
    pub queue: TransferQueue,
}

struct EditEntry {
    sftp: SftpId,
    session: EditSession,
}

/// Registre partagé (voir le module).
#[derive(Default)]
pub struct SftpRegistry {
    clients: Mutex<HashMap<SftpId, Arc<SftpEntry>>>,
    roots: Mutex<Vec<(LocalRootId, LocalRoot)>>,
    edits: Mutex<HashMap<EditId, EditEntry>>,
    next_id: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, AppError> {
    mutex.lock().map_err(|_| AppError::INTERNAL)
}

impl SftpRegistry {
    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Enregistre un client SFTP.
    pub fn insert(&self, entry: SftpEntry) -> Result<SftpId, AppError> {
        let mut clients = lock(&self.clients)?;
        if clients.len() >= MAX_SFTP_CLIENTS {
            entry.queue.shutdown();
            return Err(AppError::TOO_MANY_SFTP);
        }
        let id = self.next_id();
        clients.insert(id, Arc::new(entry));
        Ok(id)
    }

    /// Client SFTP ouvert.
    pub fn get(&self, id: SftpId) -> Result<Arc<SftpEntry>, AppError> {
        lock(&self.clients)?
            .get(&id)
            .cloned()
            .ok_or(AppError::SFTP_NOT_FOUND)
    }

    /// Ferme un client : transferts annulés, éditions arrêtées (copies locales supprimées).
    pub fn close(&self, id: SftpId) -> Result<(), AppError> {
        let entry = lock(&self.clients)?
            .remove(&id)
            .ok_or(AppError::SFTP_NOT_FOUND)?;
        entry.queue.shutdown();
        self.close_edits(|sftp| sftp == id);
        Ok(())
    }

    /// Ferme les clients d'une session qui se termine.
    pub fn close_session(&self, session: SessionId) {
        let removed: Vec<SftpId> = match self.clients.lock() {
            Ok(mut clients) => {
                let ids: Vec<_> = clients
                    .iter()
                    .filter(|(_, entry)| entry.session == session)
                    .map(|(id, _)| *id)
                    .collect();
                for id in &ids {
                    if let Some(entry) = clients.remove(id) {
                        entry.queue.shutdown();
                    }
                }
                ids
            }
            Err(_) => return,
        };
        self.close_edits(|sftp| removed.contains(&sftp));
    }

    /// Ferme tout et oublie les dossiers locaux (verrouillage, rechargement).
    pub fn close_all(&self) {
        if let Ok(mut clients) = self.clients.lock() {
            for (_, entry) in clients.drain() {
                entry.queue.shutdown();
            }
        }
        self.close_edits(|_| true);
        if let Ok(mut roots) = self.roots.lock() {
            roots.clear();
        }
    }

    fn close_edits(&self, matches: impl Fn(SftpId) -> bool) {
        if let Ok(mut edits) = self.edits.lock() {
            edits.retain(|_, edit| {
                let keep = !matches(edit.sftp);
                if !keep {
                    edit.session.close();
                }
                keep
            });
        }
    }

    /// Mémorise un dossier local choisi dans la boîte de dialogue du cœur.
    pub fn remember_root(&self, root: LocalRoot) -> Result<LocalRootId, AppError> {
        let mut roots = lock(&self.roots)?;
        if roots.len() >= MAX_LOCAL_ROOTS {
            roots.remove(0);
        }
        let id = self.next_id();
        roots.push((id, root));
        Ok(id)
    }

    /// Dossier local mémorisé.
    pub fn root(&self, id: LocalRootId) -> Result<LocalRoot, AppError> {
        lock(&self.roots)?
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .map(|(_, root)| root.clone())
            .ok_or(AppError::LOCAL_ROOT_UNKNOWN)
    }

    /// Nombre d'éditions en cours (pour refuser avant de télécharger).
    pub fn edit_count(&self) -> usize {
        self.edits.lock().map(|e| e.len()).unwrap_or(MAX_EDITS)
    }

    /// Enregistre une édition. Refusée (et arrêtée) si le client a été fermé entre-temps.
    pub fn insert_edit(&self, sftp: SftpId, session: EditSession) -> Result<EditId, AppError> {
        let mut edits = lock(&self.edits)?;
        if edits.len() >= MAX_EDITS {
            session.close();
            return Err(AppError::TOO_MANY_EDITS);
        }
        if self.get(sftp).is_err() {
            session.close();
            return Err(AppError::SFTP_NOT_FOUND);
        }
        let id = self.next_id();
        edits.insert(id, EditEntry { sftp, session });
        Ok(id)
    }

    /// Applique une action à une édition en cours.
    pub fn with_edit<T>(
        &self,
        id: EditId,
        action: impl FnOnce(&EditSession) -> T,
    ) -> Result<T, AppError> {
        let edits = lock(&self.edits)?;
        let edit = edits.get(&id).ok_or(AppError::EDIT_NOT_FOUND)?;
        Ok(action(&edit.session))
    }

    /// Arrête une édition et supprime sa copie locale.
    pub fn close_edit(&self, id: EditId) -> Result<(), AppError> {
        let edit = lock(&self.edits)?
            .remove(&id)
            .ok_or(AppError::EDIT_NOT_FOUND)?;
        edit.session.close();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_are_reachable_by_id_and_forgotten_on_close_all() {
        let registry = SftpRegistry::default();
        let dir = tempfile::tempdir().unwrap();
        let id = registry
            .remember_root(LocalRoot::new(dir.path()).unwrap())
            .unwrap();
        assert!(registry.root(id).is_ok());
        assert_eq!(
            registry.root(id + 99).err(),
            Some(AppError::LOCAL_ROOT_UNKNOWN)
        );
        registry.close_all();
        assert_eq!(registry.root(id).err(), Some(AppError::LOCAL_ROOT_UNKNOWN));
    }

    #[test]
    fn oldest_roots_are_forgotten() {
        let registry = SftpRegistry::default();
        let dir = tempfile::tempdir().unwrap();
        let first = registry
            .remember_root(LocalRoot::new(dir.path()).unwrap())
            .unwrap();
        for _ in 0..MAX_LOCAL_ROOTS {
            registry
                .remember_root(LocalRoot::new(dir.path()).unwrap())
                .unwrap();
        }
        assert_eq!(
            registry.root(first).err(),
            Some(AppError::LOCAL_ROOT_UNKNOWN)
        );
    }

    #[test]
    fn unknown_ids_are_reported() {
        let registry = SftpRegistry::default();
        assert_eq!(registry.get(1).err(), Some(AppError::SFTP_NOT_FOUND));
        assert_eq!(registry.close(1).err(), Some(AppError::SFTP_NOT_FOUND));
        assert_eq!(registry.close_edit(1).err(), Some(AppError::EDIT_NOT_FOUND));
    }
}
