//! Client SSH de Younji (ADR-002, ADR-004) : connexion, authentification, PTY distant et
//! vérification des clés d'hôte. Cette crate ne dépend pas de Tauri.

mod error;
pub mod known_hosts;
mod target;

pub use error::SshError;
pub use known_hosts::{HostKeyStatus, KeyInfo, KnownHosts};
pub use target::{MAX_COLS, MAX_ROWS, Target, TermSize};
