//! État partagé de l'application : sessions SSH ouvertes, fichiers de clé sélectionnés et
//! registre SFTP (clients, dossiers locaux, éditions), qui vit et meurt avec les sessions.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use core_ssh::{KnownHosts, SessionHandle};

use crate::error::AppError;
use crate::sftp_state::SftpRegistry;

/// Nombre maximal de sessions SSH simultanées (protège contre une interface emballée).
pub const MAX_SESSIONS: usize = 64;
/// Nombre maximal de fichiers de clé mémorisés ; au-delà, les plus anciens sont oubliés.
const MAX_KEY_FILES: usize = 32;

/// Identifiant opaque d'une session, attribué par le cœur.
pub type SessionId = u64;
/// Identifiant opaque d'un fichier de clé choisi par l'utilisateur.
pub type KeyFileId = u64;

type Sessions = Arc<Mutex<HashMap<SessionId, Arc<SessionHandle>>>>;

/// Sessions SSH et fichiers de clé.
///
/// Les fichiers de clé sont désignés par un identifiant opaque : l'interface ne manipule
/// jamais de chemin, et ne peut donc pas faire lire au cœur un fichier que l'utilisateur
/// n'a pas choisi lui-même dans la boîte de dialogue du système.
pub struct SessionManager {
    sessions: Sessions,
    key_files: Mutex<Vec<(KeyFileId, PathBuf)>>,
    next_id: AtomicU64,
    known_hosts: Arc<KnownHosts>,
    sftp: Arc<SftpRegistry>,
}

impl SessionManager {
    /// Crée le gestionnaire avec les clés d'hôte connues de l'application (dans le coffre).
    pub fn new(known_hosts: Arc<KnownHosts>) -> Self {
        Self {
            sessions: Arc::default(),
            key_files: Mutex::default(),
            next_id: AtomicU64::new(1),
            known_hosts,
            sftp: Arc::default(),
        }
    }

    /// Registre SFTP (clients ouverts sur les sessions, dossiers locaux, éditions).
    pub fn sftp(&self) -> &SftpRegistry {
        &self.sftp
    }

    /// Fichier des clés d'hôte connues.
    pub fn known_hosts(&self) -> Arc<KnownHosts> {
        self.known_hosts.clone()
    }

    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn lock_sessions(
        &self,
    ) -> Result<MutexGuard<'_, HashMap<SessionId, Arc<SessionHandle>>>, AppError> {
        self.sessions.lock().map_err(|_| AppError::INTERNAL)
    }

    /// Enregistre une session. `start` reçoit l'identifiant attribué et un moyen de retirer la
    /// session quand elle se termine. Le verrou est tenu pendant le démarrage : une session qui
    /// échoue immédiatement ne peut pas se retirer avant d'avoir été enregistrée.
    pub fn insert_session(
        &self,
        start: impl FnOnce(SessionId, SessionRemover) -> SessionHandle,
    ) -> Result<SessionId, AppError> {
        let mut sessions = self.lock_sessions()?;
        if sessions.len() >= MAX_SESSIONS {
            return Err(AppError::TOO_MANY_SESSIONS);
        }
        let id = self.next_id();
        let remover = SessionRemover {
            sessions: self.sessions.clone(),
            sftp: self.sftp.clone(),
            id,
        };
        sessions.insert(id, Arc::new(start(id, remover)));
        Ok(id)
    }

    /// Session ouverte correspondant à l'identifiant.
    pub fn session(&self, id: SessionId) -> Result<Arc<SessionHandle>, AppError> {
        self.lock_sessions()?
            .get(&id)
            .cloned()
            .ok_or(AppError::SESSION_NOT_FOUND)
    }

    /// Ferme et oublie une session.
    pub fn close_session(&self, id: SessionId) -> Result<(), AppError> {
        let handle = self
            .lock_sessions()?
            .remove(&id)
            .ok_or(AppError::SESSION_NOT_FOUND)?;
        handle.close();
        self.sftp.close_session(id);
        Ok(())
    }

    /// Ferme toutes les sessions (rechargement de l'interface : plus personne ne les pilote),
    /// avec leurs clients SFTP, transferts et éditions, et oublie les dossiers locaux.
    pub fn close_all_sessions(&self) {
        self.sftp.close_all();
        let handles: Vec<_> = match self.sessions.lock() {
            Ok(mut sessions) => sessions.drain().map(|(_, handle)| handle).collect(),
            Err(_) => return,
        };
        for handle in handles {
            handle.close();
        }
    }

    /// Mémorise un fichier de clé choisi par l'utilisateur et renvoie son identifiant.
    pub fn remember_key_file(&self, path: PathBuf) -> Result<KeyFileId, AppError> {
        let mut files = self.key_files.lock().map_err(|_| AppError::INTERNAL)?;
        if files.len() >= MAX_KEY_FILES {
            files.remove(0);
        }
        let id = self.next_id();
        files.push((id, path));
        Ok(id)
    }

    /// Chemin d'un fichier de clé précédemment choisi.
    pub fn key_file(&self, id: KeyFileId) -> Result<PathBuf, AppError> {
        self.key_files
            .lock()
            .map_err(|_| AppError::INTERNAL)?
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .map(|(_, path)| path.clone())
            .ok_or(AppError::KEY_FILE_UNKNOWN)
    }
}

/// Retire une session du gestionnaire quand elle se termine d'elle-même.
pub struct SessionRemover {
    sessions: Sessions,
    sftp: Arc<SftpRegistry>,
    id: SessionId,
}

impl SessionRemover {
    /// Retire la session (sans effet si elle a déjà été fermée) et ferme ses clients SFTP.
    pub fn remove(&self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(&self.id);
        }
        self.sftp.close_session(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_files_are_only_reachable_by_their_id() {
        let manager = SessionManager::new(Arc::new(KnownHosts::new("known_hosts")));
        let id = manager.remember_key_file(PathBuf::from("/cle")).unwrap();
        assert_eq!(manager.key_file(id).unwrap(), PathBuf::from("/cle"));
        assert_eq!(manager.key_file(id + 1000), Err(AppError::KEY_FILE_UNKNOWN));
    }

    #[test]
    fn oldest_key_files_are_forgotten() {
        let manager = SessionManager::new(Arc::new(KnownHosts::new("known_hosts")));
        let first = manager.remember_key_file(PathBuf::from("/0")).unwrap();
        for i in 1..=MAX_KEY_FILES {
            manager
                .remember_key_file(PathBuf::from(format!("/{i}")))
                .unwrap();
        }
        assert_eq!(manager.key_file(first), Err(AppError::KEY_FILE_UNKNOWN));
    }

    #[test]
    fn unknown_sessions_are_reported() {
        let manager = SessionManager::new(Arc::new(KnownHosts::new("known_hosts")));
        assert_eq!(
            manager.session(42).unwrap_err(),
            AppError::SESSION_NOT_FOUND
        );
        assert_eq!(manager.close_session(42), Err(AppError::SESSION_NOT_FOUND));
    }
}
