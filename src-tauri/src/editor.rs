//! Ouverture d'une copie de fichier distant dans un **éditeur de texte** (SFTP-05, ADR-034).
//!
//! Jamais l'application associée au type de fichier, ni une détection du type par le contenu :
//! un fichier distant nommé `x.bat`, `x.js`, `x.hta` ou `x.desktop`, ou un `README` contenant
//! du HTML, serait exécuté ou ouvert dans une autre application au lieu d'être édité. L'éditeur
//! est désigné explicitement, par un chemin absolu ; le fichier est passé en argument de
//! processus, sans shell.
//! - Windows : Bloc-notes, depuis le dossier système.
//! - Linux : premier éditeur de texte graphique trouvé dans une liste fixe ([`LINUX_EDITORS`]),
//!   dans les dossiers système ; aucun trouvé : `editor.open_failed`.
//! - macOS : TextEdit (`open -e`).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::AppError;

/// Éditeurs de texte graphiques essayés sous Linux, dans l'ordre.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const LINUX_EDITORS: [&str; 10] = [
    "gnome-text-editor",
    "gedit",
    "kate",
    "kwrite",
    "mousepad",
    "xed",
    "pluma",
    "featherpad",
    "geany",
    "leafpad",
];

/// Dossiers où les éditeurs sont cherchés (jamais le `PATH`, modifiable par l'environnement).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const LINUX_BIN_DIRS: [&str; 3] = ["/usr/bin", "/usr/local/bin", "/bin"];

/// Premier éditeur Linux présent, en chemin absolu.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn find_linux_editor(exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    LINUX_EDITORS.iter().find_map(|name| {
        LINUX_BIN_DIRS
            .iter()
            .map(|dir| Path::new(dir).join(name))
            .find(|candidate| exists(candidate))
    })
}

#[cfg(windows)]
fn editor_command(file: &Path) -> Option<Command> {
    let system_root =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let mut command = Command::new(system_root.join("System32").join("notepad.exe"));
    command.arg(file);
    Some(command)
}

#[cfg(target_os = "macos")]
fn editor_command(file: &Path) -> Option<Command> {
    let mut command = Command::new("/usr/bin/open");
    command.arg("-e").arg(file);
    Some(command)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn editor_command(file: &Path) -> Option<Command> {
    let editor = find_linux_editor(|p| p.is_file())?;
    let mut command = Command::new(editor);
    command.arg(file);
    Some(command)
}

/// Lance l'éditeur sur `file`, sans attendre sa fermeture.
pub fn open_in_editor(file: &Path) -> Result<(), AppError> {
    let Some(mut command) = editor_command(file) else {
        tracing::debug!("aucun éditeur de texte connu trouvé");
        return Err(AppError::EDITOR_FAILED);
    };
    let mut child = command.spawn().map_err(|error| {
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
    use super::*;

    #[test]
    fn linux_editors_are_absolute_and_never_a_launcher_or_shell() {
        let found = find_linux_editor(|p| p == Path::new("/usr/bin/kate"));
        assert_eq!(found, Some(PathBuf::from("/usr/bin/kate")));
        assert_eq!(
            find_linux_editor(|p| p.ends_with("gedit") || p.ends_with("kate")),
            Some(PathBuf::from("/usr/bin/gedit"))
        );
        assert_eq!(find_linux_editor(|_| false), None);
        for name in LINUX_EDITORS {
            for forbidden in ["xdg-open", "gio", "sh", "bash", "sensible-editor", "open"] {
                assert_ne!(name, forbidden);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_uses_notepad_with_the_file_as_argument() {
        let command = editor_command(Path::new("facture.bat")).unwrap();
        assert!(PathBuf::from(command.get_program()).ends_with("notepad.exe"));
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, [std::ffi::OsStr::new("facture.bat")]);
    }
}
