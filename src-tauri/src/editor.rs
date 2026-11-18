//! Ouverture d'une copie de fichier distant dans un **éditeur de texte** (SFTP-05, ADR-034).
//!
//! Jamais l'application associée au type de fichier : un fichier distant nommé `x.bat`,
//! `x.js`, `x.hta` ou `x.desktop` serait exécuté au lieu d'être ouvert. Le chemin est passé en
//! argument de processus, sans shell.
//! - Windows : Bloc-notes, depuis le dossier système.
//! - Linux : `xdg-open` sur une copie suffixée `.txt` (type `text/plain`), voir
//!   [`EDIT_SUFFIX`].
//! - macOS : TextEdit (`open -e`).

use std::path::Path;
use std::process::Command;

use crate::error::AppError;

/// Suffixe ajouté au nom de la copie locale : sous Linux, il impose le type `text/plain` à
/// `xdg-open`. Inutile ailleurs, l'éditeur étant désigné explicitement.
pub const EDIT_SUFFIX: &str = if cfg!(target_os = "linux") {
    ".txt"
} else {
    ""
};

#[cfg(windows)]
fn editor_command(file: &Path) -> Command {
    use std::path::PathBuf;
    let system_root =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let mut command = Command::new(system_root.join("System32").join("notepad.exe"));
    command.arg(file);
    command
}

#[cfg(target_os = "macos")]
fn editor_command(file: &Path) -> Command {
    let mut command = Command::new("/usr/bin/open");
    command.arg("-e").arg(file);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn editor_command(file: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(file);
    command
}

/// Lance l'éditeur sur `file`, sans attendre sa fermeture.
pub fn open_in_editor(file: &Path) -> Result<(), AppError> {
    let mut child = editor_command(file).spawn().map_err(|error| {
        tracing::debug!(%error, "lancement de l'éditeur impossible");
        AppError::EDITOR_FAILED
    })?;
    // Le processus est attendu à part, pour ne pas laisser de processus zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn the_editor_is_never_the_associated_application() {
        let command = editor_command(Path::new("facture.bat"));
        let program = PathBuf::from(command.get_program());
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(
            args.last().copied(),
            Some(std::ffi::OsStr::new("facture.bat"))
        );
        if cfg!(windows) {
            assert!(program.ends_with("notepad.exe"), "{program:?}");
        }
        if cfg!(target_os = "linux") {
            assert_eq!(EDIT_SUFFIX, ".txt");
        }
        // Jamais un interpréteur de commandes.
        let name = program
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_lowercase();
        for shell in ["cmd.exe", "powershell.exe", "sh", "bash"] {
            assert_ne!(name, shell);
        }
    }
}
