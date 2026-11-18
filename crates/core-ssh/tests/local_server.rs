//! Tests de bout en bout contre un serveur russh lancé en mémoire, sur 127.0.0.1.
//!
//! Ils vérifient le comportement de sécurité de la session (clés d'hôte, authentification)
//! sans dépendre d'un serveur OpenSSH. La compatibilité avec un vrai OpenSSH est couverte par
//! `tests/openssh.rs` (tests ignorés par défaut). Toutes les clés sont générées à l'exécution.

// Fichier de test : les fonctions utilitaires (serveur, client) échouent volontairement par
// panique, comme les tests eux-mêmes (clippy.toml ne couvre que les fonctions #[test]).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use core_ssh::known_hosts::fingerprint;
use core_ssh::{
    ConnectParams, Credentials, HIGH_WATERMARK, KnownHosts, SessionEvent, SessionHandle,
    SessionSink, SshError, Target, TermSize, Timeouts,
};
use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey, PublicKey};
use russh::server::{self, Auth, Msg, Session};
use russh::{Channel, ChannelId};
use secrecy::SecretString;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

const PASSWORD: &str = "mot-de-passe-de-test";
const WAIT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------- serveur de test

#[derive(Clone)]
struct TestServer {
    authorized_key: Option<PublicKey>,
    auth_attempts: Arc<AtomicUsize>,
}

impl server::Handler for TestServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        self.auth_attempts.fetch_add(1, Ordering::SeqCst);
        Ok(if password == PASSWORD {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn auth_publickey_offered(
        &mut self,
        _user: &str,
        offer: server::PublicKeyOffer<'_>,
    ) -> Result<Auth, Self::Error> {
        self.auth_attempts.fetch_add(1, Ordering::SeqCst);
        Ok(self.key_auth(offer.public_key))
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        key: server::VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        Ok(self.key_auth(key.public_key))
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.data(channel, &b"pret\r\n"[..])?;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        term: &str,
        cols: u32,
        rows: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.data(
            channel,
            format!("pty {term} {cols}x{rows}\r\n").into_bytes(),
        )?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        cols: u32,
        rows: u32,
        _pix_width: u32,
        _pix_height: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.data(channel, format!("taille {cols}x{rows}\r\n").into_bytes())?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        match data {
            b"exit\r" => {
                session.exit_status_request(channel, 0)?;
                session.eof(channel)?;
                session.close(channel)?;
            }
            b"flood\r" => {
                // Bien plus que le seuil de contrôle de flux.
                let chunk = vec![b'y'; 32 * 1024];
                for _ in 0..(HIGH_WATERMARK / chunk.len() * 3) {
                    session.data(channel, chunk.clone())?;
                }
            }
            other => session.data(channel, other.to_vec())?,
        }
        Ok(())
    }
}

impl TestServer {
    fn key_auth(&self, key: &PublicKey) -> Auth {
        match &self.authorized_key {
            Some(authorized) if authorized.key_data() == key.key_data() => Auth::Accept,
            _ => Auth::reject(),
        }
    }
}

struct Fixture {
    port: u16,
    host_key: PublicKey,
    auth_attempts: Arc<AtomicUsize>,
    known_hosts: Arc<KnownHosts>,
    _dir: tempfile::TempDir,
}

async fn start_server(authorized_key: Option<PublicKey>) -> Fixture {
    let host_key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let host_public = host_key.public_key().clone();
    let config = Arc::new(server::Config {
        keys: vec![host_key],
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        ..server::Config::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let auth_attempts = Arc::new(AtomicUsize::new(0));
    let handler = TestServer {
        authorized_key,
        auth_attempts: auth_attempts.clone(),
    };
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let config = config.clone();
            let handler = handler.clone();
            tokio::spawn(async move {
                if let Ok(running) = server::run_stream(config, socket, handler).await {
                    let _ = running.await;
                }
            });
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let known_hosts = Arc::new(KnownHosts::new(dir.path().join("known_hosts")));
    Fixture {
        port,
        host_key: host_public,
        auth_attempts,
        known_hosts,
        _dir: dir,
    }
}

// ---------------------------------------------------------------- client de test

#[derive(Debug)]
enum Received {
    Event(SessionEvent),
    Output(Vec<u8>),
}

struct ChannelSink(mpsc::UnboundedSender<Received>);

impl SessionSink for ChannelSink {
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
    output: Vec<u8>,
}

impl Client {
    fn connect(fixture: &Fixture, credentials: Credentials) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let params = ConnectParams {
            target: Target::new("127.0.0.1", u32::from(fixture.port), "testeur").unwrap(),
            credentials,
            size: TermSize::new(80, 24).unwrap(),
            known_hosts: fixture.known_hosts.clone(),
            timeouts: Timeouts {
                host_key_decision: Duration::from_secs(5),
                ..Timeouts::default()
            },
        };
        Self {
            handle: SessionHandle::spawn(params, Arc::new(ChannelSink(tx))),
            rx,
            output: Vec::new(),
        }
    }

    /// Prochain événement d'état ; la sortie reçue entre-temps est accumulée et acquittée.
    async fn next_event(&mut self) -> SessionEvent {
        loop {
            match tokio::time::timeout(WAIT, self.rx.recv()).await {
                Ok(Some(Received::Event(event))) => return event,
                Ok(Some(Received::Output(data))) => {
                    self.handle.ack(data.len());
                    self.output.extend(data);
                }
                Ok(None) => panic!("flux d'événements fermé"),
                Err(_) => panic!("aucun événement reçu à temps"),
            }
        }
    }

    /// Attend que la sortie contienne `needle`.
    async fn wait_output(&mut self, needle: &str) {
        let deadline = tokio::time::Instant::now() + WAIT;
        while !String::from_utf8_lossy(&self.output).contains(needle) {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(left, self.rx.recv()).await {
                Ok(Some(Received::Output(data))) => {
                    self.handle.ack(data.len());
                    self.output.extend(data);
                }
                Ok(Some(Received::Event(event))) => panic!("événement inattendu : {event:?}"),
                _ => panic!(
                    "{needle:?} absent de la sortie : {:?}",
                    String::from_utf8_lossy(&self.output)
                ),
            }
        }
    }

    /// Accepte la clé d'hôte inconnue et attend la connexion.
    async fn accept_and_connect(&mut self) {
        assert!(matches!(
            self.next_event().await,
            SessionEvent::HostKeyUnknown { .. }
        ));
        self.handle.decide_host_key(true).unwrap();
        assert_eq!(self.next_event().await, SessionEvent::Authenticating);
        assert_eq!(self.next_event().await, SessionEvent::Connected);
    }
}

fn password(value: &str) -> Credentials {
    Credentials::password(SecretString::from(value.to_owned())).unwrap()
}

fn disconnected(reason: SshError) -> SessionEvent {
    SessionEvent::Disconnected { reason }
}

// ---------------------------------------------------------------- tests

#[tokio::test]
async fn unknown_host_key_is_shown_then_saved_once_accepted() {
    let fixture = start_server(None).await;
    let mut client = Client::connect(&fixture, password(PASSWORD));

    match client.next_event().await {
        SessionEvent::HostKeyUnknown { key } => {
            assert_eq!(key.algorithm, "ssh-ed25519");
            assert_eq!(key.fingerprint, fingerprint(&fixture.host_key));
        }
        other => panic!("attendu HostKeyUnknown, reçu {other:?}"),
    }
    // Rien n'est envoyé au serveur tant que l'utilisateur n'a pas décidé.
    assert_eq!(fixture.auth_attempts.load(Ordering::SeqCst), 0);
    client.handle.decide_host_key(true).unwrap();
    assert_eq!(client.next_event().await, SessionEvent::Authenticating);
    assert_eq!(client.next_event().await, SessionEvent::Connected);
    client.wait_output("pty xterm-256color 80x24").await;
    client.wait_output("pret").await;

    // Seconde connexion : clé connue, aucune question.
    let mut second = Client::connect(&fixture, password(PASSWORD));
    assert_eq!(second.next_event().await, SessionEvent::Authenticating);
    assert_eq!(second.next_event().await, SessionEvent::Connected);
}

#[tokio::test]
async fn rejected_host_key_aborts_without_authenticating() {
    let fixture = start_server(None).await;
    let mut client = Client::connect(&fixture, password(PASSWORD));
    assert!(matches!(
        client.next_event().await,
        SessionEvent::HostKeyUnknown { .. }
    ));
    client.handle.decide_host_key(false).unwrap();
    assert_eq!(
        client.next_event().await,
        disconnected(SshError::HostKeyRejected)
    );
    assert_eq!(fixture.auth_attempts.load(Ordering::SeqCst), 0);
    // Rien n'a été enregistré.
    assert!(!fixture.known_hosts.path().unwrap().exists());
    // Une seconde décision n'est plus attendue.
    assert_eq!(
        client.handle.decide_host_key(true),
        Err(SshError::NoPendingDecision)
    );
}

#[tokio::test]
async fn changed_host_key_is_refused_before_any_credential_is_sent() {
    let fixture = start_server(None).await;
    // L'hôte est connu avec une autre clé (d'un autre type, en plus).
    let other = PrivateKey::random(
        &mut rand::rng(),
        Algorithm::Ecdsa {
            curve: russh::keys::EcdsaCurve::NistP256,
        },
    )
    .unwrap();
    let line = format!(
        "[127.0.0.1]:{} {}\n",
        fixture.port,
        other.public_key().to_openssh().unwrap()
    );
    std::fs::write(fixture.known_hosts.path().unwrap(), line).unwrap();

    let mut client = Client::connect(&fixture, password(PASSWORD));
    match client.next_event().await {
        SessionEvent::HostKeyChanged { key, known } => {
            assert_eq!(key.fingerprint, fingerprint(&fixture.host_key));
            assert_eq!(known.len(), 1);
            assert_eq!(known[0].fingerprint, fingerprint(other.public_key()));
        }
        other => panic!("attendu HostKeyChanged, reçu {other:?}"),
    }
    assert_eq!(
        client.next_event().await,
        disconnected(SshError::HostKeyChanged)
    );
    // Point clé : aucun identifiant n'a été présenté au serveur suspect.
    assert_eq!(fixture.auth_attempts.load(Ordering::SeqCst), 0);
    // Aucune décision ne peut être forcée.
    assert_eq!(
        client.handle.decide_host_key(true),
        Err(SshError::NoPendingDecision)
    );
}

#[tokio::test]
async fn wrong_password_is_reported() {
    let fixture = start_server(None).await;
    let mut client = Client::connect(&fixture, password("faux"));
    client.accept_and_connect_until_auth().await;
    assert_eq!(
        client.next_event().await,
        disconnected(SshError::AuthFailed)
    );
}

#[tokio::test]
async fn public_key_authentication_works_and_wrong_key_fails() {
    let user_key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    // Passage par un fichier chiffré, comme dans l'application.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cle");
    let encrypted = user_key
        .encrypt(&mut rand::rng(), "phrase secrete")
        .unwrap();
    std::fs::write(
        &path,
        encrypted.to_openssh(LineEnding::LF).unwrap().as_bytes(),
    )
    .unwrap();
    let loaded = core_ssh::auth::load_private_key(
        &path,
        Some(&SecretString::from("phrase secrete".to_owned())),
    )
    .unwrap();

    let fixture = start_server(Some(user_key.public_key().clone())).await;
    let mut client = Client::connect(&fixture, Credentials::PrivateKey(Arc::new(loaded)));
    client.accept_and_connect().await;
    client.wait_output("pret").await;

    let other = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let mut client = Client::connect(&fixture, Credentials::PrivateKey(Arc::new(other)));
    assert_eq!(client.next_event().await, SessionEvent::Authenticating);
    assert_eq!(
        client.next_event().await,
        disconnected(SshError::AuthFailed)
    );
}

#[tokio::test]
async fn input_resize_and_remote_exit() {
    let fixture = start_server(None).await;
    let mut client = Client::connect(&fixture, password(PASSWORD));
    client.accept_and_connect().await;

    client.handle.write(b"echo bonjour".to_vec()).await.unwrap();
    client.wait_output("echo bonjour").await;

    client
        .handle
        .resize(TermSize::new(132, 43).unwrap())
        .await
        .unwrap();
    client.wait_output("taille 132x43").await;

    client.handle.write(b"exit\r".to_vec()).await.unwrap();
    assert_eq!(
        client.next_event().await,
        disconnected(SshError::SessionEnded)
    );
}

#[tokio::test]
async fn oversized_input_is_refused() {
    let fixture = start_server(None).await;
    let client = Client::connect(&fixture, password(PASSWORD));
    let too_big = vec![b'a'; core_ssh::MAX_INPUT_CHUNK + 1];
    assert_eq!(
        client.handle.write(too_big).await,
        Err(SshError::InputTooLarge)
    );
}

#[tokio::test]
async fn output_pauses_until_acknowledged() {
    let fixture = start_server(None).await;
    let mut client = Client::connect(&fixture, password(PASSWORD));
    client.accept_and_connect().await;
    client.wait_output("pret").await;
    client.output.clear();

    client.handle.write(b"flood\r".to_vec()).await.unwrap();
    // Sans acquittement, la session cesse d'émettre peu après le seuil.
    let mut received = 0usize;
    while let Ok(Some(Received::Output(data))) =
        tokio::time::timeout(Duration::from_millis(500), client.rx.recv()).await
    {
        received += data.len();
    }
    assert!(received >= HIGH_WATERMARK, "reçu {received}");
    assert!(
        received < HIGH_WATERMARK + 64 * 1024,
        "la sortie n'est pas bornée : {received}"
    );

    // Une fois acquittée, la suite arrive.
    client.handle.ack(received);
    let mut more = 0usize;
    while let Ok(Some(Received::Output(data))) =
        tokio::time::timeout(Duration::from_millis(500), client.rx.recv()).await
    {
        client.handle.ack(data.len());
        more += data.len();
    }
    assert!(more > 0);
}

#[tokio::test]
async fn closing_the_handle_ends_the_session() {
    let fixture = start_server(None).await;
    let mut client = Client::connect(&fixture, password(PASSWORD));
    client.accept_and_connect().await;
    client.handle.close();
    assert_eq!(
        client.next_event().await,
        disconnected(SshError::SessionClosed)
    );
    assert_eq!(
        client.handle.write(b"x".to_vec()).await,
        Err(SshError::SessionClosed)
    );
}

#[tokio::test]
async fn closing_during_host_key_prompt_aborts() {
    let fixture = start_server(None).await;
    let mut client = Client::connect(&fixture, password(PASSWORD));
    assert!(matches!(
        client.next_event().await,
        SessionEvent::HostKeyUnknown { .. }
    ));
    client.handle.close();
    assert_eq!(
        client.next_event().await,
        disconnected(SshError::SessionClosed)
    );
    assert!(!fixture.known_hosts.path().unwrap().exists());
}

#[tokio::test]
async fn unreachable_port_is_reported() {
    // Port libéré juste après sa réservation : la connexion est refusée.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let fixture = Fixture {
        port,
        ..start_server(None).await
    };
    let mut client = Client::connect(&fixture, password(PASSWORD));
    match client.next_event().await {
        SessionEvent::Disconnected { reason } => assert!(
            matches!(
                reason,
                SshError::ConnectionRefused | SshError::ConnectFailed
            ),
            "{reason:?}"
        ),
        other => panic!("attendu Disconnected, reçu {other:?}"),
    }
}

#[tokio::test]
async fn server_closing_during_handshake_fails_cleanly() {
    // Serveur qui accepte la connexion TCP puis la coupe aussitôt.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        if let Ok((socket, _)) = listener.accept().await {
            drop(socket);
        }
    });
    let fixture = Fixture {
        port,
        ..start_server(None).await
    };
    let mut client = Client::connect(&fixture, password(PASSWORD));
    assert_eq!(
        client.next_event().await,
        disconnected(SshError::HandshakeFailed)
    );
}

impl Client {
    /// Accepte la clé d'hôte et attend le début de l'authentification.
    async fn accept_and_connect_until_auth(&mut self) {
        assert!(matches!(
            self.next_event().await,
            SessionEvent::HostKeyUnknown { .. }
        ));
        self.handle.decide_host_key(true).unwrap();
        assert_eq!(self.next_event().await, SessionEvent::Authenticating);
    }
}

// Clé d'hôte enregistrée au format attendu après acceptation.
#[tokio::test]
async fn accepted_key_is_written_in_openssh_format() {
    let fixture = start_server(None).await;
    let mut client = Client::connect(&fixture, password(PASSWORD));
    client.accept_and_connect().await;
    let written = std::fs::read_to_string(fixture.known_hosts.path().unwrap()).unwrap();
    let encoded = fixture.host_key.to_openssh().unwrap();
    assert_eq!(
        written,
        format!("[127.0.0.1]:{} {}\n", fixture.port, encoded.trim())
    );
}

#[tokio::test]
async fn identity_with_key_and_password_falls_back_to_the_password() {
    // Clé refusée par le serveur, mot de passe correct : connexion par le mot de passe.
    let fixture = start_server(None).await;
    let refused = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let credentials =
        Credentials::KeyThenPassword(Arc::new(refused), SecretString::from(PASSWORD.to_owned()));
    let mut client = Client::connect(&fixture, credentials);
    client.accept_and_connect().await;
    client.wait_output("pret").await;

    // Clé acceptée : le mot de passe (faux ici) n'est jamais essayé.
    let user_key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let fixture = start_server(Some(user_key.public_key().clone())).await;
    let credentials =
        Credentials::KeyThenPassword(Arc::new(user_key), SecretString::from("faux".to_owned()));
    let mut client = Client::connect(&fixture, credentials);
    client.accept_and_connect().await;
    client.wait_output("pret").await;
}
