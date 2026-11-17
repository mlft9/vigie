//! Session SSH interactive : connexion, clé d'hôte, authentification, PTY et shell
//! (SSH-01, SSH-02, SSH-04, SSH-06).
//!
//! Chaque session vit dans une tâche tokio propriétaire, annulable. L'appelant la pilote via
//! un [`SessionHandle`] et reçoit les événements et la sortie du terminal via un
//! [`SessionSink`]. La crate ne connaît pas Tauri : `src-tauri` fournit le `SessionSink`.
//!
//! **Contrôle de flux** : la sortie est envoyée par lots (quelques millisecondes ou 32 Kio).
//! L'appelant acquitte les octets une fois affichés ([`SessionHandle::ack`]). Au-delà de
//! [`HIGH_WATERMARK`] octets non acquittés, la session cesse de lire le canal SSH : la
//! fenêtre SSH se remplit et le serveur s'arrête d'émettre. La mémoire reste bornée et
//! l'interface n'est jamais submergée, même avec `yes` ou un `cat` de plusieurs gigaoctets.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKey};
use russh::{ChannelMsg, ChannelWriteHalf, Disconnect};
use secrecy::ExposeSecret;
use tokio::net::TcpStream;
use tokio::sync::{Notify, mpsc, oneshot};
use tokio::time::{Instant, sleep_until, timeout};
use tokio_util::sync::CancellationToken;

use crate::auth::Credentials;
use crate::config::{Timeouts, client_config};
use crate::known_hosts::{HostKeyStatus, KeyInfo, KnownHosts};
use crate::{SshError, Target, TermSize};

/// Type de terminal annoncé au serveur.
const TERM: &str = "xterm-256color";
/// Taille maximale d'un envoi de données vers le serveur en un seul appel.
pub const MAX_INPUT_CHUNK: usize = 64 * 1024;
/// Délai maximal de regroupement de la sortie avant envoi.
const BATCH_DELAY: Duration = Duration::from_millis(8);
/// Taille maximale d'un lot de sortie.
const BATCH_MAX: usize = 32 * 1024;
/// Octets envoyés mais non acquittés au-delà desquels la lecture est suspendue.
pub const HIGH_WATERMARK: usize = 2 * 1024 * 1024;
/// Commandes en attente vers la tâche de session (frappes, redimensionnements).
const CONTROL_CAPACITY: usize = 256;
/// Taille maximale d'une bannière serveur affichée.
const MAX_BANNER_LEN: usize = 8 * 1024;

/// Événement d'état d'une session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    /// Clé d'hôte inconnue : l'appelant doit appeler [`SessionHandle::decide_host_key`].
    HostKeyUnknown {
        /// Clé présentée par le serveur.
        key: KeyInfo,
    },
    /// Clé d'hôte différente de celle enregistrée : la connexion est refusée.
    HostKeyChanged {
        /// Clé présentée par le serveur.
        key: KeyInfo,
        /// Clés enregistrées pour cet hôte.
        known: Vec<KeyInfo>,
    },
    /// Clé d'hôte acceptée, authentification en cours.
    Authenticating,
    /// Shell ouvert, la session est utilisable.
    Connected,
    /// Session terminée. Dernier événement émis.
    Disconnected {
        /// Cause, sous forme de code stable.
        reason: SshError,
    },
}

/// Destinataire des événements et de la sortie d'une session.
pub trait SessionSink: Send + Sync + 'static {
    /// Changement d'état.
    fn event(&self, event: SessionEvent);
    /// Lot d'octets produits par le terminal distant, à acquitter une fois affiché.
    fn output(&self, data: Vec<u8>);
}

/// Paramètres d'ouverture d'une session.
#[derive(Debug)]
pub struct ConnectParams {
    /// Cible validée.
    pub target: Target,
    /// Moyen d'authentification. Il est détruit dès la fin de l'authentification.
    pub credentials: Credentials,
    /// Taille initiale du terminal.
    pub size: TermSize,
    /// Fichier des clés d'hôte connues.
    pub known_hosts: Arc<KnownHosts>,
    /// Délais.
    pub timeouts: Timeouts,
}

enum Control {
    Input(Vec<u8>),
    Resize(TermSize),
}

type DecisionSlot = Arc<Mutex<Option<oneshot::Sender<bool>>>>;

/// Compteur des octets envoyés à l'appelant et pas encore acquittés.
#[derive(Default)]
struct Flow {
    unacked: AtomicUsize,
    acked: Notify,
}

impl Flow {
    fn unacked(&self) -> usize {
        self.unacked.load(Ordering::Acquire)
    }

    fn sent(&self, bytes: usize) {
        self.unacked.fetch_add(bytes, Ordering::AcqRel);
    }

    fn ack(&self, bytes: usize) {
        // Un acquittement excessif (interface bogguée ou malveillante) ne fait pas déborder le
        // compteur : il le ramène à zéro.
        let mut current = self.unacked.load(Ordering::Acquire);
        loop {
            let next = current.saturating_sub(bytes);
            match self.unacked.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
        self.acked.notify_one();
    }
}

/// Pilotage d'une session. La détruire ferme la session.
pub struct SessionHandle {
    control: mpsc::Sender<Control>,
    decision: DecisionSlot,
    flow: Arc<Flow>,
    cancel: CancellationToken,
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandle")
            .field("closed", &self.cancel.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl SessionHandle {
    /// Démarre une session dans une nouvelle tâche tokio (à appeler dans un contexte tokio).
    pub fn spawn(params: ConnectParams, sink: Arc<dyn SessionSink>) -> Self {
        let (control, control_rx) = mpsc::channel(CONTROL_CAPACITY);
        let decision = DecisionSlot::default();
        let flow = Arc::new(Flow::default());
        let cancel = CancellationToken::new();
        let task = SessionTask {
            control_rx,
            decision: decision.clone(),
            flow: flow.clone(),
            cancel: cancel.clone(),
            sink,
        };
        tokio::spawn(task.run(params));
        Self {
            control,
            decision,
            flow,
            cancel,
        }
    }

    /// Envoie des octets au shell distant (frappes, collage).
    pub async fn write(&self, data: Vec<u8>) -> Result<(), SshError> {
        if data.len() > MAX_INPUT_CHUNK {
            return Err(SshError::InputTooLarge);
        }
        if data.is_empty() {
            return Ok(());
        }
        self.control
            .send(Control::Input(data))
            .await
            .map_err(|_| SshError::SessionClosed)
    }

    /// Répercute un redimensionnement du terminal sur le serveur.
    pub async fn resize(&self, size: TermSize) -> Result<(), SshError> {
        self.control
            .send(Control::Resize(size))
            .await
            .map_err(|_| SshError::SessionClosed)
    }

    /// Acquitte des octets de sortie affichés par l'appelant.
    pub fn ack(&self, bytes: usize) {
        self.flow.ack(bytes);
    }

    /// Transmet la décision de l'utilisateur sur une clé d'hôte inconnue.
    pub fn decide_host_key(&self, accept: bool) -> Result<(), SshError> {
        let sender = self
            .decision
            .lock()
            .map_err(|_| SshError::NoPendingDecision)?
            .take()
            .ok_or(SshError::NoPendingDecision)?;
        sender.send(accept).map_err(|_| SshError::NoPendingDecision)
    }

    /// Ferme la session (sans effet si elle l'est déjà).
    pub fn close(&self) {
        self.cancel.cancel();
    }

    /// Vrai si la session a été fermée par l'appelant.
    pub fn is_closed(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

struct SessionTask {
    control_rx: mpsc::Receiver<Control>,
    decision: DecisionSlot,
    flow: Arc<Flow>,
    cancel: CancellationToken,
    sink: Arc<dyn SessionSink>,
}

impl SessionTask {
    async fn run(mut self, params: ConnectParams) {
        let target = params.target.clone();
        tracing::debug!(%target, "ouverture d'une session SSH");
        let cancel = self.cancel.clone();
        let established = tokio::select! {
            biased;
            () = cancel.cancelled() => Err(SshError::SessionClosed),
            result = self.establish(params) => result,
        };
        let reason = match established {
            Ok((handle, writer, reader)) => self.io_loop(&handle, writer, reader).await,
            Err(reason) => reason,
        };
        tracing::debug!(%target, code = reason.code(), "fin de session SSH");
        self.sink.event(SessionEvent::Disconnected { reason });
    }

    /// Connexion, clé d'hôte, authentification, PTY et shell.
    async fn establish(
        &mut self,
        params: ConnectParams,
    ) -> Result<
        (
            Handle<ClientHandler>,
            ChannelWriteHalf<client::Msg>,
            russh::ChannelReadHalf,
        ),
        SshError,
    > {
        let ConnectParams {
            target,
            credentials,
            size,
            known_hosts,
            timeouts,
        } = params;

        let stream = timeout(timeouts.connect, connect_tcp(target.host(), target.port()))
            .await
            .map_err(|_| SshError::Timeout)??;

        let verdict = Arc::new(Mutex::new(None));
        let handler = ClientHandler {
            known_hosts,
            host: target.host().to_owned(),
            port: target.port(),
            sink: self.sink.clone(),
            decision: self.decision.clone(),
            decision_timeout: timeouts.host_key_decision,
            verdict: verdict.clone(),
        };
        let config = client_config(&timeouts);
        let handshake = client::connect_stream(config, stream, handler);
        let take_verdict = || verdict.lock().ok().and_then(|mut v| v.take());
        let mut handle =
            match timeout(timeouts.handshake + timeouts.host_key_decision, handshake).await {
                Ok(Ok(handle)) => handle,
                Ok(Err(error)) => {
                    return Err(take_verdict().unwrap_or_else(|| {
                        tracing::debug!(%error, "échec de la poignée de main SSH");
                        SshError::HandshakeFailed
                    }));
                }
                Err(_) => return Err(take_verdict().unwrap_or(SshError::Timeout)),
            };

        self.sink.event(SessionEvent::Authenticating);
        let username = target.username().to_owned();
        let opened = timeout(timeouts.auth, async {
            authenticate(&mut handle, username, credentials).await?;
            open_shell(&handle, size).await
        })
        .await
        .map_err(|_| SshError::Timeout)??;

        self.sink.event(SessionEvent::Connected);
        Ok((handle, opened.0, opened.1))
    }

    /// Boucle d'entrée/sortie du shell. Renvoie la cause de la fin de session.
    async fn io_loop(
        &mut self,
        handle: &Handle<ClientHandler>,
        writer: ChannelWriteHalf<client::Msg>,
        mut reader: russh::ChannelReadHalf,
    ) -> SshError {
        let mut pending: Vec<u8> = Vec::new();
        let mut deadline: Option<Instant> = None;
        let mut ended_by_server = false;

        let reason = loop {
            // Le lot en attente compte aussi : la sortie retenue ne dépasse jamais le seuil
            // de plus d'un paquet SSH.
            let can_read = self.flow.unacked() + pending.len() < HIGH_WATERMARK;
            tokio::select! {
                biased;
                () = self.cancel.cancelled() => {
                    let _ = writer.close().await;
                    let _ = handle.disconnect(Disconnect::ByApplication, "", "").await;
                    break SshError::SessionClosed;
                }
                () = sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => {
                    self.flush(&mut pending);
                    deadline = None;
                }
                control = self.control_rx.recv() => match control {
                    Some(Control::Input(data)) => {
                        if writer.data(&data[..]).await.is_err() {
                            break SshError::ConnectionLost;
                        }
                    }
                    Some(Control::Resize(size)) => {
                        if writer.window_change(size.cols(), size.rows(), 0, 0).await.is_err() {
                            break SshError::ConnectionLost;
                        }
                    }
                    // Le pilote a disparu : on ferme comme une annulation.
                    None => self.cancel.cancel(),
                },
                () = self.flow.acked.notified(), if !can_read => {}
                message = reader.wait(), if can_read => match message {
                    Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                        pending.extend_from_slice(&data);
                        if pending.len() >= BATCH_MAX {
                            self.flush(&mut pending);
                            deadline = None;
                        } else if deadline.is_none() {
                            deadline = Some(Instant::now() + BATCH_DELAY);
                        }
                    }
                    Some(ChannelMsg::ExitStatus { .. } | ChannelMsg::ExitSignal { .. } | ChannelMsg::Close) => {
                        ended_by_server = true;
                    }
                    Some(_) => {}
                    // Canal fermé : fin normale si le serveur l'a annoncée, coupure sinon
                    // (keepalives sans réponse, TCP rompu, serveur arrêté).
                    None => break if ended_by_server { SshError::SessionEnded } else { SshError::ConnectionLost },
                },
            }
        };
        self.flush(&mut pending);
        reason
    }

    fn flush(&self, pending: &mut Vec<u8>) {
        if pending.is_empty() {
            return;
        }
        let batch = std::mem::take(pending);
        self.flow.sent(batch.len());
        self.sink.output(batch);
    }
}

/// Résolution DNS puis connexion TCP à la première adresse joignable.
async fn connect_tcp(host: &str, port: u16) -> Result<TcpStream, SshError> {
    let addrs: Vec<_> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| {
            tracing::debug!(%error, "résolution DNS impossible");
            SshError::DnsFailed
        })?
        .collect();
    if addrs.is_empty() {
        return Err(SshError::DnsFailed);
    }
    let mut last = SshError::ConnectFailed;
    for addr in addrs {
        match TcpStream::connect(addr).await {
            Ok(stream) => {
                // Terminal interactif : chaque frappe part immédiatement.
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(error) => {
                tracing::debug!(%error, "connexion TCP impossible");
                last = if error.kind() == std::io::ErrorKind::ConnectionRefused {
                    SshError::ConnectionRefused
                } else {
                    SshError::ConnectFailed
                };
            }
        }
    }
    Err(last)
}

async fn authenticate(
    handle: &mut Handle<ClientHandler>,
    username: String,
    credentials: Credentials,
) -> Result<(), SshError> {
    let auth_error = |error: russh::Error| {
        tracing::debug!(%error, "erreur pendant l'authentification");
        SshError::ConnectionLost
    };
    let result = match credentials {
        Credentials::Password(password) => handle
            .authenticate_password(username, password.expose_secret())
            .await
            .map_err(auth_error)?,
        Credentials::PrivateKey(key) => {
            // Signature RSA toujours en SHA-2, même si le serveur n'annonce rien : jamais SHA-1.
            let hash = handle
                .best_supported_rsa_hash()
                .await
                .map_err(auth_error)?
                .flatten()
                .or(Some(HashAlg::Sha512));
            let key = PrivateKeyWithHashAlg::new(key, hash);
            handle
                .authenticate_publickey(username, key)
                .await
                .map_err(auth_error)?
        }
    };
    if result.success() {
        Ok(())
    } else {
        Err(SshError::AuthFailed)
    }
}

async fn open_shell(
    handle: &Handle<ClientHandler>,
    size: TermSize,
) -> Result<(ChannelWriteHalf<client::Msg>, russh::ChannelReadHalf), SshError> {
    let failed = |error: russh::Error| {
        tracing::debug!(%error, "ouverture du shell impossible");
        SshError::ChannelFailed
    };
    let channel = handle.channel_open_session().await.map_err(failed)?;
    channel
        .request_pty(false, TERM, size.cols(), size.rows(), 0, 0, &[])
        .await
        .map_err(failed)?;
    channel.request_shell(false).await.map_err(failed)?;
    let (reader, writer) = channel.split();
    Ok((writer, reader))
}

/// Gestionnaire russh : vérification de la clé d'hôte et bannière.
struct ClientHandler {
    known_hosts: Arc<KnownHosts>,
    host: String,
    port: u16,
    sink: Arc<dyn SessionSink>,
    decision: DecisionSlot,
    decision_timeout: Duration,
    /// Cause précise d'un refus, russh ne renvoyant qu'une erreur générique.
    verdict: Arc<Mutex<Option<SshError>>>,
}

impl ClientHandler {
    fn refuse(&self, reason: SshError) -> Result<bool, russh::Error> {
        if let Ok(mut verdict) = self.verdict.lock() {
            verdict.get_or_insert(reason);
        }
        Ok(false)
    }

    async fn ask_user(&self, key: &PublicKey) -> Result<bool, russh::Error> {
        let (sender, receiver) = oneshot::channel();
        match self.decision.lock() {
            Ok(mut slot) => *slot = Some(sender),
            Err(_) => return self.refuse(SshError::HostKeyRejected),
        }
        self.sink.event(SessionEvent::HostKeyUnknown {
            key: KeyInfo::of(key),
        });
        let accepted = match timeout(self.decision_timeout, receiver).await {
            Ok(Ok(accepted)) => accepted,
            Ok(Err(_)) => false,
            Err(_) => {
                if let Ok(mut slot) = self.decision.lock() {
                    slot.take();
                }
                return self.refuse(SshError::HostKeyTimeout);
            }
        };
        if !accepted {
            return self.refuse(SshError::HostKeyRejected);
        }
        match self.known_hosts.add(&self.host, self.port, key) {
            Ok(()) => {
                tracing::info!("nouvelle clé d'hôte enregistrée");
                Ok(true)
            }
            Err(error) => self.refuse(error),
        }
    }
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } = server_public_key else {
            // Les certificats d'hôte ne sont pas annoncés (hors périmètre) : refus.
            return self.refuse(SshError::HostKeyRejected);
        };
        match self.known_hosts.check(&self.host, self.port, key) {
            Ok(HostKeyStatus::Trusted) => Ok(true),
            Ok(HostKeyStatus::Unknown) => self.ask_user(key).await,
            Ok(HostKeyStatus::Changed { known }) => {
                tracing::warn!("clé d'hôte modifiée : connexion refusée");
                self.sink.event(SessionEvent::HostKeyChanged {
                    key: KeyInfo::of(key),
                    known,
                });
                self.refuse(SshError::HostKeyChanged)
            }
            Ok(HostKeyStatus::Revoked) => {
                tracing::warn!("clé d'hôte révoquée : connexion refusée");
                self.refuse(SshError::HostKeyRevoked)
            }
            Err(error) => self.refuse(error),
        }
    }

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let text = sanitize_banner(banner);
        if !text.is_empty() {
            self.sink.output(text.into_bytes());
        }
        Ok(())
    }
}

/// Bannière affichée comme du texte brut : toute séquence de contrôle (échappements ANSI,
/// OSC…) est retirée pour qu'un serveur ne puisse pas piloter le terminal avant
/// l'authentification. Les fins de ligne sont normalisées en `\r\n`.
pub(crate) fn sanitize_banner(banner: &str) -> String {
    let mut out = String::new();
    for c in banner.chars() {
        if out.len() >= MAX_BANNER_LEN {
            break;
        }
        match c {
            '\n' => out.push_str("\r\n"),
            '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_is_stripped_of_control_sequences() {
        let banner = "Bienvenue\x1b[31m rouge\x1b]52;c;ZXZpbA==\x07\r\nfin\t!\n";
        assert_eq!(
            sanitize_banner(banner),
            "Bienvenue[31m rouge]52;c;ZXZpbA==\r\nfin\t!\r\n"
        );
    }

    #[test]
    fn banner_is_bounded() {
        let banner = "a".repeat(MAX_BANNER_LEN * 2);
        assert!(sanitize_banner(&banner).len() <= MAX_BANNER_LEN);
    }

    #[test]
    fn flow_ack_saturates() {
        let flow = Flow::default();
        flow.sent(100);
        flow.ack(40);
        assert_eq!(flow.unacked(), 60);
        flow.ack(1_000);
        assert_eq!(flow.unacked(), 0);
    }
}
