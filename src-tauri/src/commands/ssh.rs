//! Commandes IPC des sessions SSH (étape 1). Chaque commande valide son entrée, appelle
//! `core-ssh` et convertit l'erreur en code. Aucune logique SSH ici (ADR-004).
//!
//! Flux de données : la sortie du terminal part en **octets bruts** sur un premier `Channel`
//! (pas de JSON), les changements d'état en JSON sur un second. Les secrets n'arrivent qu'une
//! fois, dans `ssh_connect`, et ne repartent jamais vers l'interface.

use std::sync::Arc;

use core_ssh::auth::load_private_key;
use core_ssh::{
    ConnectParams, Credentials, KeyInfo, SessionEvent, SessionHandle, SessionSink, Target,
    TermSize, Timeouts,
};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::error::AppError;
use crate::state::{KeyFileId, SessionId, SessionManager, SessionRemover};

/// Longueur maximale du nom de fichier de clé renvoyé pour l'affichage.
const MAX_DISPLAY_NAME: usize = 128;

/// Demande de connexion.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectRequest {
    host: String,
    port: u32,
    username: String,
    auth: AuthRequest,
    cols: u32,
    rows: u32,
}

/// Moyen d'authentification demandé. Pas de `Debug` : il contient des secrets.
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AuthRequest {
    /// Mot de passe, transmis une seule fois.
    Password { password: SecretString },
    /// Fichier de clé choisi via `ssh_pick_key_file`, avec sa passphrase éventuelle.
    Key {
        key_file_id: KeyFileId,
        passphrase: Option<SecretString>,
    },
}

/// Description affichable d'une clé d'hôte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfoDto {
    algorithm: String,
    fingerprint: String,
}

impl From<KeyInfo> for KeyInfoDto {
    fn from(key: KeyInfo) -> Self {
        Self {
            algorithm: key.algorithm,
            fingerprint: key.fingerprint,
        }
    }
}

/// Événement d'état envoyé à l'interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SessionEventDto {
    /// Clé d'hôte inconnue, à accepter ou refuser.
    HostKeyUnknown { key: KeyInfoDto },
    /// Clé d'hôte modifiée : connexion refusée, sans contournement possible.
    HostKeyChanged {
        key: KeyInfoDto,
        known: Vec<KeyInfoDto>,
        /// Fichier à corriger à la main pour débloquer (étape 1, en attendant HKY-04).
        known_hosts_path: String,
    },
    /// Authentification en cours.
    Authenticating,
    /// Session prête.
    Connected,
    /// Session terminée (dernier événement).
    Disconnected { code: &'static str },
}

impl SessionEventDto {
    fn from_event(event: SessionEvent, known_hosts_path: &str) -> Self {
        match event {
            SessionEvent::HostKeyUnknown { key } => Self::HostKeyUnknown { key: key.into() },
            SessionEvent::HostKeyChanged { key, known } => Self::HostKeyChanged {
                key: key.into(),
                known: known.into_iter().map(Into::into).collect(),
                known_hosts_path: known_hosts_path.to_owned(),
            },
            SessionEvent::Authenticating => Self::Authenticating,
            SessionEvent::Connected => Self::Connected,
            SessionEvent::Disconnected { reason } => Self::Disconnected {
                code: reason.code(),
            },
        }
    }
}

/// Relie une session `core-ssh` aux deux `Channel` de l'interface.
struct ChannelSink {
    events: Channel<SessionEventDto>,
    output: Channel<InvokeResponseBody>,
    remover: SessionRemover,
    known_hosts_path: String,
}

impl SessionSink for ChannelSink {
    fn event(&self, event: SessionEvent) {
        let last = matches!(event, SessionEvent::Disconnected { .. });
        let dto = SessionEventDto::from_event(event, &self.known_hosts_path);
        if self.events.send(dto).is_err() {
            tracing::debug!("événement de session non délivré (interface fermée)");
        }
        if last {
            self.remover.remove();
        }
    }

    fn output(&self, data: Vec<u8>) {
        if self.output.send(InvokeResponseBody::Raw(data)).is_err() {
            tracing::debug!("sortie de terminal non délivrée (interface fermée)");
        }
    }
}

/// Fichier de clé choisi : identifiant opaque et nom (sans le chemin) pour l'affichage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyFileRef {
    id: KeyFileId,
    name: String,
}

/// Ouvre une session SSH. Renvoie son identifiant ; la suite arrive par les `Channel`.
#[tauri::command]
pub async fn ssh_connect(
    request: ConnectRequest,
    on_event: Channel<SessionEventDto>,
    on_output: Channel<InvokeResponseBody>,
    state: State<'_, SessionManager>,
) -> Result<SessionId, AppError> {
    let ConnectRequest {
        host,
        port,
        username,
        auth,
        cols,
        rows,
    } = request;
    let target = Target::new(&host, port, &username)?;
    let size = TermSize::new(cols, rows)?;
    let credentials = match auth {
        AuthRequest::Password { password } => Credentials::password(password)?,
        AuthRequest::Key {
            key_file_id,
            passphrase,
        } => {
            let path = state.key_file(key_file_id)?;
            // Lecture du fichier et déchiffrement (bcrypt-pbkdf) hors de l'exécuteur async.
            let key = tauri::async_runtime::spawn_blocking(move || {
                load_private_key(&path, passphrase.as_ref())
            })
            .await
            .map_err(|_| AppError::INTERNAL)??;
            Credentials::PrivateKey(Arc::new(key))
        }
    };

    let known_hosts = state.known_hosts();
    let known_hosts_path = known_hosts.path().display().to_string();
    let params = ConnectParams {
        target,
        credentials,
        size,
        known_hosts,
        timeouts: Timeouts::default(),
    };
    let id = state.insert_session(|_, remover| {
        let sink = ChannelSink {
            events: on_event,
            output: on_output,
            remover,
            known_hosts_path,
        };
        SessionHandle::spawn(params, Arc::new(sink))
    })?;
    tracing::debug!(session = id, "session SSH créée");
    Ok(id)
}

/// Envoie la saisie de l'utilisateur. `binary` signale des octets bruts (chaque caractère
/// représente un octet 0–255), sinon le texte est encodé en UTF-8.
#[tauri::command]
pub async fn ssh_write(
    session_id: SessionId,
    data: String,
    binary: bool,
    state: State<'_, SessionManager>,
) -> Result<(), AppError> {
    let bytes = if binary {
        data.chars()
            .map(|c| u8::try_from(u32::from(c)).map_err(|_| AppError::INVALID_INPUT))
            .collect::<Result<Vec<u8>, _>>()?
    } else {
        data.into_bytes()
    };
    let session = state.session(session_id)?;
    session.write(bytes).await.map_err(Into::into)
}

/// Répercute la nouvelle taille du terminal.
#[tauri::command]
pub async fn ssh_resize(
    session_id: SessionId,
    cols: u32,
    rows: u32,
    state: State<'_, SessionManager>,
) -> Result<(), AppError> {
    let size = TermSize::new(cols, rows)?;
    let session = state.session(session_id)?;
    session.resize(size).await.map_err(Into::into)
}

/// Acquitte des octets de sortie affichés (contrôle de flux).
#[tauri::command]
pub fn ssh_ack(
    session_id: SessionId,
    bytes: u32,
    state: State<'_, SessionManager>,
) -> Result<(), AppError> {
    state.session(session_id)?.ack(bytes as usize);
    Ok(())
}

/// Décision de l'utilisateur sur une clé d'hôte inconnue.
#[tauri::command]
pub fn ssh_host_key_decision(
    session_id: SessionId,
    accept: bool,
    state: State<'_, SessionManager>,
) -> Result<(), AppError> {
    state
        .session(session_id)?
        .decide_host_key(accept)
        .map_err(Into::into)
}

/// Ferme une session.
#[tauri::command]
pub fn ssh_close(session_id: SessionId, state: State<'_, SessionManager>) -> Result<(), AppError> {
    state.close_session(session_id)
}

/// Ouvre la boîte de dialogue du système pour choisir une clé privée. Le chemin reste dans le
/// cœur ; l'interface ne reçoit qu'un identifiant opaque et le nom du fichier.
#[tauri::command]
pub async fn ssh_pick_key_file(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, SessionManager>,
) -> Result<Option<KeyFileRef>, AppError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let mut dialog = app.dialog().file().set_parent(&window);
    if let Ok(ssh_dir) = app.path().home_dir().map(|home| home.join(".ssh"))
        && ssh_dir.is_dir()
    {
        dialog = dialog.set_directory(ssh_dir);
    }
    dialog.pick_file(move |picked| {
        let _ = sender.send(picked);
    });
    let Some(picked) = receiver.await.map_err(|_| AppError::DIALOG_FAILED)? else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|_| AppError::DIALOG_FAILED)?;
    let name: String = path
        .file_name()
        .map(|n| n.to_string_lossy().chars().take(MAX_DISPLAY_NAME).collect())
        .unwrap_or_default();
    let id = state.remember_key_file(path)?;
    Ok(Some(KeyFileRef { id, name }))
}

impl From<tauri::Error> for AppError {
    fn from(error: tauri::Error) -> Self {
        tracing::debug!(%error, "erreur Tauri");
        AppError::INTERNAL
    }
}

#[cfg(test)]
mod tests {
    use core_ssh::SshError;

    use super::*;

    #[test]
    fn connect_request_parses_password_and_key_variants() {
        let request: ConnectRequest = serde_json::from_value(serde_json::json!({
            "host": "srv", "port": 22, "username": "u", "cols": 80, "rows": 24,
            "auth": { "kind": "password", "password": "secret" }
        }))
        .unwrap();
        assert!(matches!(request.auth, AuthRequest::Password { .. }));

        let request: ConnectRequest = serde_json::from_value(serde_json::json!({
            "host": "srv", "port": 22, "username": "u", "cols": 80, "rows": 24,
            "auth": { "kind": "key", "keyFileId": 3, "passphrase": null }
        }))
        .unwrap();
        assert!(matches!(
            request.auth,
            AuthRequest::Key {
                key_file_id: 3,
                passphrase: None
            }
        ));
    }

    #[test]
    fn connect_request_rejects_unknown_fields() {
        // Un chemin de fichier glissé dans la requête est refusé, pas ignoré.
        let result = serde_json::from_value::<ConnectRequest>(serde_json::json!({
            "host": "srv", "port": 22, "username": "u", "cols": 80, "rows": 24,
            "auth": { "kind": "key", "keyFileId": 3, "path": "C:/secret" }
        }));
        assert!(result.is_err());
        let result = serde_json::from_value::<ConnectRequest>(serde_json::json!({
            "host": "srv", "port": 22, "username": "u", "cols": 80, "rows": 24,
            "auth": { "kind": "password", "password": "x" }, "keyPath": "/etc/shadow"
        }));
        assert!(result.is_err());
    }

    #[test]
    fn events_serialize_without_secrets_in_camel_case() {
        let key = KeyInfo {
            algorithm: "ssh-ed25519".into(),
            fingerprint: "SHA256:abc".into(),
        };
        let event = SessionEventDto::from_event(
            SessionEvent::HostKeyChanged {
                key: key.clone(),
                known: vec![key],
            },
            "/data/known_hosts",
        );
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            serde_json::json!({
                "type": "hostKeyChanged",
                "key": { "algorithm": "ssh-ed25519", "fingerprint": "SHA256:abc" },
                "known": [{ "algorithm": "ssh-ed25519", "fingerprint": "SHA256:abc" }],
                "knownHostsPath": "/data/known_hosts"
            })
        );
        let event = SessionEventDto::from_event(
            SessionEvent::Disconnected {
                reason: SshError::AuthFailed,
            },
            "",
        );
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            serde_json::json!({ "type": "disconnected", "code": "ssh.auth_failed" })
        );
    }
}
