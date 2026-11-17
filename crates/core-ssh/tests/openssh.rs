//! Tests d'intégration contre un vrai serveur OpenSSH (conteneur en CI, VM du lab en local).
//!
//! Ignorés par défaut ; lancés par `npm run test:integration`. La cible est lue dans les
//! variables d'environnement (jamais d'identifiant en dur, voir docs/ENVIRONNEMENT_DE_TEST.md) :
//!
//! | Variable | Rôle |
//! |---|---|
//! | `YOUNJI_TEST_SSH_HOST`, `YOUNJI_TEST_SSH_PORT` | Serveur (port 22 par défaut) |
//! | `YOUNJI_TEST_SSH_USER`, `YOUNJI_TEST_SSH_PASSWORD` | Compte à mot de passe |
//! | `YOUNJI_TEST_SSH_KEY_USER`, `YOUNJI_TEST_SSH_KEY_PATH` | Compte à clé et sa clé privée |
//! | `YOUNJI_TEST_SSH_KEY_PASSPHRASE` | Passphrase de cette clé (facultative) |
//! | `YOUNJI_TEST_SSH_EXTRA_KEYS` | Autres clés non chiffrées (ECDSA, RSA…), séparées par `;` |
//! | `YOUNJI_TEST_REQUIRED` | `1` : une variable manquante fait échouer au lieu d'ignorer |
//!
//! Chaque test utilise un `known_hosts` vierge dans un dossier temporaire.

// Fichier de test : les fonctions utilitaires échouent volontairement par panique.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use core_ssh::auth::load_private_key;
use core_ssh::{
    ConnectParams, Credentials, KnownHosts, SessionEvent, SessionHandle, SessionSink, SshError,
    Target, TermSize, Timeouts,
};
use secrecy::SecretString;
use tokio::sync::mpsc;

const WAIT: Duration = Duration::from_secs(20);

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Lit une variable obligatoire pour ce test ; `None` (test ignoré) si elle manque, sauf si
/// `YOUNJI_TEST_REQUIRED=1`.
fn required(name: &str) -> Option<String> {
    let value = var(name);
    if value.is_none() {
        assert!(
            var("YOUNJI_TEST_REQUIRED").as_deref() != Some("1"),
            "variable {name} manquante alors que YOUNJI_TEST_REQUIRED=1"
        );
        eprintln!("test ignoré : variable {name} absente");
    }
    value
}

fn host_and_port() -> Option<(String, u32)> {
    let host = required("YOUNJI_TEST_SSH_HOST")?;
    let port = var("YOUNJI_TEST_SSH_PORT").map_or(22, |p| p.parse().expect("port invalide"));
    Some((host, port))
}

enum Received {
    Event(SessionEvent),
    Output(Vec<u8>),
}

struct Sink(mpsc::UnboundedSender<Received>);

impl SessionSink for Sink {
    fn event(&self, event: SessionEvent) {
        let _ = self.0.send(Received::Event(event));
    }
    fn output(&self, data: Vec<u8>) {
        let _ = self.0.send(Received::Output(data));
    }
}

struct Client {
    handle: SessionHandle,
    rx: mpsc::UnboundedReceiver<Received>,
    output: String,
    _dir: tempfile::TempDir,
    known_hosts: Arc<KnownHosts>,
}

impl Client {
    fn connect(user: &str, credentials: Credentials) -> Option<Self> {
        let (host, port) = host_and_port()?;
        let dir = tempfile::tempdir().unwrap();
        let known_hosts = Arc::new(KnownHosts::new(dir.path().join("known_hosts")));
        Some(Self::connect_with(
            &host,
            port,
            user,
            credentials,
            known_hosts,
            dir,
        ))
    }

    fn connect_with(
        host: &str,
        port: u32,
        user: &str,
        credentials: Credentials,
        known_hosts: Arc<KnownHosts>,
        dir: tempfile::TempDir,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let params = ConnectParams {
            target: Target::new(host, port, user).unwrap(),
            credentials,
            size: TermSize::new(80, 24).unwrap(),
            known_hosts: known_hosts.clone(),
            timeouts: Timeouts::default(),
        };
        Self {
            handle: SessionHandle::spawn(params, Arc::new(Sink(tx))),
            rx,
            output: String::new(),
            _dir: dir,
            known_hosts,
        }
    }

    async fn next_event(&mut self) -> SessionEvent {
        loop {
            match tokio::time::timeout(WAIT, self.rx.recv()).await {
                Ok(Some(Received::Event(event))) => return event,
                Ok(Some(Received::Output(data))) => self.absorb(&data),
                _ => panic!("aucun événement reçu à temps"),
            }
        }
    }

    fn absorb(&mut self, data: &[u8]) {
        self.handle.ack(data.len());
        self.output.push_str(&String::from_utf8_lossy(data));
    }

    async fn wait_output(&mut self, needle: &str) {
        let deadline = tokio::time::Instant::now() + WAIT;
        while !self.output.contains(needle) {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(left, self.rx.recv()).await {
                Ok(Some(Received::Output(data))) => self.absorb(&data),
                Ok(Some(Received::Event(event))) => panic!("événement inattendu : {event:?}"),
                _ => panic!("{needle:?} absent de la sortie :\n{}", self.output),
            }
        }
    }

    /// Accepte la clé d'hôte (known_hosts vierge) et attend l'ouverture du shell.
    async fn accept_and_open(&mut self) {
        match self.next_event().await {
            SessionEvent::HostKeyUnknown { key } => {
                assert!(key.fingerprint.starts_with("SHA256:"));
            }
            other => panic!("attendu HostKeyUnknown, reçu {other:?}"),
        }
        self.handle.decide_host_key(true).unwrap();
        assert_eq!(self.next_event().await, SessionEvent::Authenticating);
        assert_eq!(self.next_event().await, SessionEvent::Connected);
    }

    /// Vérifie le shell : calcul, taille du terminal après redimensionnement, sortie propre.
    async fn exercise_shell(&mut self) {
        // Le calcul évite de confondre l'écho de la commande avec son résultat.
        self.handle
            .write(b"echo younji-$((40+2))\r".to_vec())
            .await
            .unwrap();
        self.wait_output("younji-42").await;
        self.handle
            .write(b"echo term=$TERM\r".to_vec())
            .await
            .unwrap();
        self.wait_output("term=xterm-256color").await;

        self.handle
            .resize(TermSize::new(132, 43).unwrap())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        self.handle.write(b"stty size\r".to_vec()).await.unwrap();
        self.wait_output("43 132").await;

        self.handle.write(b"exit\r".to_vec()).await.unwrap();
        assert_eq!(
            self.next_event().await,
            SessionEvent::Disconnected {
                reason: SshError::SessionEnded
            }
        );
    }
}

fn password_credentials(value: String) -> Credentials {
    Credentials::password(SecretString::from(value)).unwrap()
}

#[tokio::test]
#[ignore = "nécessite un serveur SSH de test"]
async fn password_session_runs_a_full_terminal() {
    let (Some(user), Some(password)) = (
        required("YOUNJI_TEST_SSH_USER"),
        required("YOUNJI_TEST_SSH_PASSWORD"),
    ) else {
        return;
    };
    let Some(mut client) = Client::connect(&user, password_credentials(password)) else {
        return;
    };
    client.accept_and_open().await;
    client.exercise_shell().await;
}

#[tokio::test]
#[ignore = "nécessite un serveur SSH de test"]
async fn wrong_password_is_refused() {
    let Some(user) = required("YOUNJI_TEST_SSH_USER") else {
        return;
    };
    let wrong = password_credentials("mauvais-mot-de-passe-younji".into());
    let Some(mut client) = Client::connect(&user, wrong) else {
        return;
    };
    assert!(matches!(
        client.next_event().await,
        SessionEvent::HostKeyUnknown { .. }
    ));
    client.handle.decide_host_key(true).unwrap();
    assert_eq!(client.next_event().await, SessionEvent::Authenticating);
    assert_eq!(
        client.next_event().await,
        SessionEvent::Disconnected {
            reason: SshError::AuthFailed
        }
    );
}

#[tokio::test]
#[ignore = "nécessite un serveur SSH de test"]
async fn key_session_with_passphrase() {
    let (Some(user), Some(path)) = (
        required("YOUNJI_TEST_SSH_KEY_USER"),
        required("YOUNJI_TEST_SSH_KEY_PATH"),
    ) else {
        return;
    };
    let passphrase = var("YOUNJI_TEST_SSH_KEY_PASSPHRASE").map(SecretString::from);
    let key = load_private_key(&PathBuf::from(path), passphrase.as_ref()).unwrap();
    let Some(mut client) = Client::connect(&user, Credentials::PrivateKey(Arc::new(key))) else {
        return;
    };
    client.accept_and_open().await;
    client.exercise_shell().await;
}

#[tokio::test]
#[ignore = "nécessite un serveur SSH de test"]
async fn other_key_types_authenticate() {
    let (Some(user), Some(paths)) = (
        var("YOUNJI_TEST_SSH_KEY_USER"),
        var("YOUNJI_TEST_SSH_EXTRA_KEYS"),
    ) else {
        eprintln!("test ignoré : aucune clé supplémentaire");
        return;
    };
    for path in paths.split(';').filter(|p| !p.is_empty()) {
        let key = load_private_key(&PathBuf::from(path), None).unwrap();
        eprintln!("clé {}", key.algorithm().as_str());
        let Some(mut client) = Client::connect(&user, Credentials::PrivateKey(Arc::new(key)))
        else {
            return;
        };
        client.accept_and_open().await;
        client.handle.write(b"exit\r".to_vec()).await.unwrap();
        assert_eq!(
            client.next_event().await,
            SessionEvent::Disconnected {
                reason: SshError::SessionEnded
            }
        );
    }
}

#[tokio::test]
#[ignore = "nécessite un serveur SSH de test"]
async fn changed_host_key_is_blocked() {
    let (Some(user), Some(password)) = (
        required("YOUNJI_TEST_SSH_USER"),
        required("YOUNJI_TEST_SSH_PASSWORD"),
    ) else {
        return;
    };
    let Some((host, port)) = host_and_port() else {
        return;
    };
    // L'hôte est déjà « connu » avec une autre clé (fixture publique).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("known_hosts");
    let target = Target::new(&host, port, &user).unwrap();
    let pattern = core_ssh::known_hosts::host_pattern(target.host(), target.port());
    let other = include_str!("fixtures/test_host_ed25519.pub");
    let mut fields = other.split_whitespace();
    let line = format!(
        "{pattern} {} {}\n",
        fields.next().unwrap(),
        fields.next().unwrap()
    );
    std::fs::write(&path, line).unwrap();
    let known_hosts = Arc::new(KnownHosts::new(path));

    let mut client = Client::connect_with(
        &host,
        port,
        &user,
        password_credentials(password),
        known_hosts,
        dir,
    );
    assert!(matches!(
        client.next_event().await,
        SessionEvent::HostKeyChanged { .. }
    ));
    assert_eq!(
        client.next_event().await,
        SessionEvent::Disconnected {
            reason: SshError::HostKeyChanged
        }
    );
    // Le fichier n'a pas été modifié : l'ancienne clé reste la seule enregistrée.
    let content = std::fs::read_to_string(client.known_hosts.path()).unwrap();
    assert_eq!(content.lines().count(), 1);
}
