//! Commandes IPC des sessions SSH (étape 1). Chaque commande valide son entrée, appelle
//! `core-ssh` et convertit l'erreur en code. Aucune logique SSH ici (ADR-004).
//!
//! Flux de données : la sortie du terminal part en **octets bruts** sur un premier `Channel`
//! (pas de JSON), les changements d'état en JSON sur un second. Les secrets n'arrivent qu'une
//! fois, dans `ssh_connect`, et ne repartent jamais vers l'interface.

use std::sync::Arc;

use core_ssh::auth::{decode_stored_key, load_private_key};
use core_ssh::{
    ConnectParams, Credentials, KeyInfo, SessionEvent, SessionHandle, SessionSink, Target,
    TermSize, Timeouts,
};
use core_vault::{EntryId, VaultData, VaultError};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::error::AppError;
use crate::state::{KeyFileId, SessionId, SessionManager, SessionRemover};
use crate::vault_state::VaultState;

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
    /// Clé d'hôte modifiée : connexion refusée, sans contournement possible. Pour débloquer,
    /// l'utilisateur supprime l'ancienne clé dans la gestion des clés d'hôte (HKY-04).
    HostKeyChanged {
        key: KeyInfoDto,
        known: Vec<KeyInfoDto>,
        /// Forme de l'hôte dans les clés connues (`hôte` ou `[hôte]:port`, normalisé).
        host_pattern: String,
    },
    /// Authentification en cours.
    Authenticating,
    /// Session prête.
    Connected,
    /// Session terminée (dernier événement).
    Disconnected { code: &'static str },
}

impl SessionEventDto {
    fn from_event(event: SessionEvent, host_pattern: &str) -> Self {
        match event {
            SessionEvent::HostKeyUnknown { key } => Self::HostKeyUnknown { key: key.into() },
            SessionEvent::HostKeyChanged { key, known } => Self::HostKeyChanged {
                key: key.into(),
                known: known.into_iter().map(Into::into).collect(),
                host_pattern: host_pattern.to_owned(),
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
    host_pattern: String,
}

impl SessionSink for ChannelSink {
    fn event(&self, event: SessionEvent) {
        let last = matches!(event, SessionEvent::Disconnected { .. });
        let dto = SessionEventDto::from_event(event, &self.host_pattern);
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
    vault: State<'_, VaultState>,
) -> Result<SessionId, AppError> {
    // Les clés d'hôte connues vivent dans le coffre : pas de connexion sans lui.
    if !vault.is_unlocked() {
        return Err(AppError::VAULT_LOCKED);
    }
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

    spawn_session(target, credentials, size, on_event, on_output, &state)
}

/// Démarre une session et la relie aux deux `Channel` de l'interface.
fn spawn_session(
    target: Target,
    credentials: Credentials,
    size: TermSize,
    on_event: Channel<SessionEventDto>,
    on_output: Channel<InvokeResponseBody>,
    state: &SessionManager,
) -> Result<SessionId, AppError> {
    let host_pattern = core_ssh::known_hosts::host_pattern(target.host(), target.port());
    let params = ConnectParams {
        target,
        credentials,
        size,
        known_hosts: state.known_hosts(),
        timeouts: Timeouts::default(),
    };
    let id = state.insert_session(|_, remover| {
        let sink = ChannelSink {
            events: on_event,
            output: on_output,
            remover,
            host_pattern,
        };
        SessionHandle::spawn(params, Arc::new(sink))
    })?;
    tracing::debug!(session = id, "session SSH créée");
    Ok(id)
}

/// Demande de connexion à un hôte du carnet.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostConnectRequest {
    host_id: EntryId,
    cols: u32,
    rows: u32,
}

/// Cible et moyen d'authentification d'un hôte du carnet, lus dans le coffre.
fn host_credentials(data: &VaultData, host_id: EntryId) -> Result<(Target, Credentials), AppError> {
    let host = data.host(host_id)?;
    let identity_id = host.identity_id.ok_or(AppError::HOST_WITHOUT_IDENTITY)?;
    let identity = data.identity(identity_id)?;
    let username = if host.username.is_empty() {
        identity.username.as_str()
    } else {
        host.username.as_str()
    };
    let target = Target::new(&host.address, u32::from(host.port), username)?;
    let key = match identity.key_id {
        Some(key_id) => Some(Arc::new(decode_stored_key(&data.key(key_id)?.private_key)?)),
        None => None,
    };
    let credentials = match (key, identity.password.clone()) {
        (Some(key), Some(password)) => Credentials::KeyThenPassword(key, password),
        (Some(key), None) => Credentials::PrivateKey(key),
        (None, Some(password)) => Credentials::password(password)?,
        (None, None) => return Err(VaultError::IdentityIncomplete.into()),
    };
    Ok((target, credentials))
}

/// Ouvre une session vers un hôte du carnet, avec l'identité enregistrée : rien n'est
/// redemandé, et aucun secret ne passe par l'interface.
#[tauri::command]
pub fn ssh_connect_host(
    request: HostConnectRequest,
    on_event: Channel<SessionEventDto>,
    on_output: Channel<InvokeResponseBody>,
    state: State<'_, SessionManager>,
    vault: State<'_, VaultState>,
) -> Result<SessionId, AppError> {
    let size = TermSize::new(request.cols, request.rows)?;
    let (target, credentials) =
        vault.read(|vault| host_credentials(vault.data(), request.host_id))?;
    vault.touch();
    spawn_session(target, credentials, size, on_event, on_output, &state)
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
    fn host_credentials_follow_the_identity() {
        use core_vault::{HostInput, IdentityInput, NewKey, PasswordChange};
        use russh::keys::{Algorithm, PrivateKey};

        let mut data = VaultData::default();
        let generated = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        let imported = core_ssh::auth::prepare_import(&generated).unwrap();
        let key = data
            .add_key(NewKey {
                name: "clé".into(),
                algorithm: imported.algorithm,
                fingerprint: imported.fingerprint,
                public_key: imported.public_key,
                private_key: imported.private_key,
            })
            .unwrap();
        let identity = |data: &mut VaultData, password: Option<&str>, key_id| {
            data.upsert_identity(IdentityInput {
                id: None,
                name: "id".into(),
                username: "depuis-identite".into(),
                password: password.map_or(PasswordChange::Clear, |p| {
                    PasswordChange::Set(SecretString::from(p))
                }),
                key_id,
            })
            .unwrap()
        };
        let host = |data: &mut VaultData, username: &str, identity_id| {
            data.upsert_host(HostInput {
                id: None,
                name: "h".into(),
                address: "srv.lab".into(),
                port: 2222,
                username: username.into(),
                identity_id,
                group_id: None,
                tags: vec![],
                notes: String::new(),
            })
            .unwrap()
        };

        let both = identity(&mut data, Some("pw"), Some(key));
        let only_key = identity(&mut data, None, Some(key));
        let only_password = identity(&mut data, Some("pw"), None);
        let h1 = host(&mut data, "", Some(both));
        let h2 = host(&mut data, "autre", Some(only_key));
        let h3 = host(&mut data, "", Some(only_password));
        let h4 = host(&mut data, "u", None);

        let (target, creds) = host_credentials(&data, h1).unwrap();
        assert_eq!(target.username(), "depuis-identite");
        assert_eq!(target.port(), 2222);
        assert!(matches!(creds, Credentials::KeyThenPassword(..)));
        let (target, creds) = host_credentials(&data, h2).unwrap();
        assert_eq!(target.username(), "autre");
        assert!(matches!(creds, Credentials::PrivateKey(_)));
        assert!(matches!(
            host_credentials(&data, h3).unwrap().1,
            Credentials::Password(_)
        ));
        assert_eq!(
            host_credentials(&data, h4).unwrap_err(),
            AppError::HOST_WITHOUT_IDENTITY
        );
        assert_eq!(
            host_credentials(&data, uuid_like()).unwrap_err(),
            AppError::from(VaultError::EntryNotFound)
        );
    }

    fn uuid_like() -> EntryId {
        serde_json::from_value(serde_json::json!("00000000-0000-4000-8000-000000000000")).unwrap()
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
            "[srv]:2222",
        );
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            serde_json::json!({
                "type": "hostKeyChanged",
                "key": { "algorithm": "ssh-ed25519", "fingerprint": "SHA256:abc" },
                "known": [{ "algorithm": "ssh-ed25519", "fingerprint": "SHA256:abc" }],
                "hostPattern": "[srv]:2222"
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
