//! Client SSH de Younji (ADR-002, ADR-004) : connexion, authentification, PTY distant et
//! vérification des clés d'hôte. Cette crate ne dépend pas de Tauri.

pub mod auth;
pub mod config;
mod error;
pub mod known_hosts;
mod session;
mod target;

pub use auth::{Credentials, ImportedKey};
pub use config::Timeouts;
pub use error::SshError;
pub use known_hosts::{HostKeyStatus, KeyInfo, KnownHostEntry, KnownHosts, KnownHostsStorage};
pub use session::{
    ConnectParams, HIGH_WATERMARK, MAX_INPUT_CHUNK, SessionEvent, SessionHandle, SessionSink,
};
pub use target::{MAX_COLS, MAX_ROWS, Target, TermSize};
