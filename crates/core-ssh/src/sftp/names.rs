//! Chemins distants et noms de fichiers venant du serveur (SECURITE.md §4.2, « SFTP »).
//!
//! Un serveur malveillant ou compromis choisit librement les noms qu'il renvoie dans un
//! listage. Avant **toute écriture locale**, chaque nom passe par [`check_local_name`], qui
//! refuse ce qui permettrait de sortir du dossier choisi ou de viser un fichier spécial
//! (`..`, séparateurs, noms réservés de Windows, flux NTFS…). Un nom refusé n'est jamais
//! « corrigé » silencieusement : l'élément est ignoré et compté comme refusé.

use crate::SshError;
use crate::prompt::is_invisible;

/// Longueur maximale d'un chemin distant, en octets.
pub const MAX_REMOTE_PATH: usize = 4096;
/// Longueur maximale d'un nom de fichier, en octets (limite courante des systèmes de fichiers).
pub const MAX_NAME_LEN: usize = 255;

/// Chemin distant absolu, normalisé (séparateurs `/`, sans `.` ni `..`, sans `/` final).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemotePath(String);

impl RemotePath {
    /// Racine du serveur.
    pub fn root() -> Self {
        Self("/".to_owned())
    }

    /// Valide et normalise un chemin venant de l'interface ou du serveur (OSC 7, `realpath`).
    /// Le chemin doit être absolu, sans caractère NUL ni retour à la ligne, et tenir dans
    /// [`MAX_REMOTE_PATH`] octets. `..` est résolu lexicalement (sans remonter au-delà de `/`).
    pub fn parse(raw: &str) -> Result<Self, SshError> {
        if !raw.starts_with('/') || raw.len() > MAX_REMOTE_PATH || raw.contains(['\0', '\n', '\r'])
        {
            return Err(SshError::SftpInvalidPath);
        }
        let mut parts: Vec<&str> = Vec::new();
        for part in raw.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }
        Ok(Self(format!("/{}", parts.join("/"))))
    }

    /// Chemin sous forme de texte.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Vrai pour la racine `/`.
    pub fn is_root(&self) -> bool {
        self.0 == "/"
    }

    /// Chemin d'un élément de ce dossier. Le nom doit être un composant unique : non vide,
    /// différent de `.` et `..`, sans `/` ni caractère NUL.
    pub fn join(&self, name: &str) -> Result<Self, SshError> {
        if !is_single_component(name) {
            return Err(SshError::SftpInvalidPath);
        }
        let joined = if self.is_root() {
            format!("/{name}")
        } else {
            format!("{}/{name}", self.0)
        };
        if joined.len() > MAX_REMOTE_PATH {
            return Err(SshError::SftpInvalidPath);
        }
        Ok(Self(joined))
    }

    /// Dossier parent (`None` pour la racine).
    pub fn parent(&self) -> Option<Self> {
        if self.is_root() {
            return None;
        }
        let index = self.0.rfind('/')?;
        Some(if index == 0 {
            Self::root()
        } else {
            Self(self.0[..index].to_owned())
        })
    }

    /// Dernier composant (`None` pour la racine).
    pub fn file_name(&self) -> Option<&str> {
        if self.is_root() {
            return None;
        }
        self.0.rsplit('/').next()
    }
}

impl std::fmt::Display for RemotePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Un composant de chemin unique (sans séparateur ni NUL, ni `.` ni `..`).
fn is_single_component(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= MAX_NAME_LEN
        && !name.contains(['/', '\0'])
}

/// Caractères interdits dans un nom Windows (en plus des séparateurs et des contrôles).
const WINDOWS_FORBIDDEN: [char; 6] = ['<', '>', '"', '|', '?', '*'];

/// Noms de périphériques réservés par Windows, avec ou sans extension (`CON`, `nul.txt`…).
const WINDOWS_RESERVED: [&str; 28] = [
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM0", "COM1", "COM2", "COM3", "COM4",
    "COM5", "COM6", "COM7", "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
    "LPT7", "LPT8", "LPT9", "COM¹", "LPT¹",
];
const WINDOWS_RESERVED_SUPERSCRIPTS: [&str; 4] = ["COM²", "COM³", "LPT²", "LPT³"];

/// Vérifie qu'un nom venant du serveur peut servir de nom de fichier **local**, sur Windows
/// comme sur Linux (même règle partout, pour qu'un comportement testé sur l'un vaille sur
/// l'autre). Refuse :
/// - le vide, `.`, `..`, plus de [`MAX_NAME_LEN`] octets ;
/// - les séparateurs `/` et `\`, `:` (lecteur, flux NTFS `fichier:flux`) ;
/// - les caractères de contrôle et les caractères invisibles ou bidirectionnels (qui
///   permettent de déguiser une extension : `facture\u{202E}fdp.exe`) ;
/// - `< > " | ? *` ;
/// - un point ou une espace final (Windows les retire, ce qui ferait viser un autre fichier) ;
/// - les noms réservés de Windows (`CON`, `NUL`, `COM1`…), avec ou sans extension.
pub fn check_local_name(name: &str) -> Result<(), SshError> {
    if !is_single_component(name)
        || name.ends_with(['.', ' '])
        || name.chars().any(|c| {
            c == '\\'
                || c == ':'
                || c.is_control()
                || is_invisible(c)
                || WINDOWS_FORBIDDEN.contains(&c)
        })
    {
        return Err(SshError::SftpUnsafeName);
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    let upper = stem.to_uppercase();
    if WINDOWS_RESERVED.contains(&upper.as_str())
        || WINDOWS_RESERVED_SUPERSCRIPTS.contains(&upper.as_str())
    {
        return Err(SshError::SftpUnsafeName);
    }
    Ok(())
}

/// Nom affichable : les caractères de contrôle et invisibles sont remplacés par `\u{FFFD}`,
/// pour qu'un nom ne puisse ni piloter l'affichage ni se déguiser. Le second élément est vrai
/// si le nom a été modifié ou s'il ne peut pas être écrit localement (l'interface le signale).
pub fn display_name(name: &str) -> (String, bool) {
    let mut changed = false;
    let shown: String = name
        .chars()
        .map(|c| {
            if c.is_control() || is_invisible(c) {
                changed = true;
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect();
    let suspicious = changed || check_local_name(name).is_err();
    (shown, suspicious)
}

/// Nom de remplacement en cas de conflit : `rapport.txt` → `rapport (1).txt`. L'extension est
/// conservée ; un nom qui commence par un point (`.bashrc`) n'a pas d'extension.
pub fn numbered_name(name: &str, n: u32) -> String {
    match name.rfind('.') {
        Some(index) if index > 0 => format!("{} ({n}){}", &name[..index], &name[index..]),
        _ => format!("{name} ({n})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_paths_are_absolute_and_normalized() {
        assert_eq!(RemotePath::parse("/").unwrap().as_str(), "/");
        assert_eq!(
            RemotePath::parse("/home//alice/./docs/").unwrap().as_str(),
            "/home/alice/docs"
        );
        assert_eq!(RemotePath::parse("/a/b/../c").unwrap().as_str(), "/a/c");
        assert_eq!(RemotePath::parse("/../../etc").unwrap().as_str(), "/etc");
        assert_eq!(
            RemotePath::parse("/home/été à/x y").unwrap().as_str(),
            "/home/été à/x y"
        );
        for bad in ["", "relative", "~/x", "/a\0b", "/a\nb"] {
            assert_eq!(
                RemotePath::parse(bad),
                Err(SshError::SftpInvalidPath),
                "{bad:?}"
            );
        }
        let long = format!("/{}", "a".repeat(MAX_REMOTE_PATH));
        assert_eq!(RemotePath::parse(&long), Err(SshError::SftpInvalidPath));
    }

    #[test]
    fn remote_path_join_parent_and_name() {
        let root = RemotePath::root();
        let home = root.join("home").unwrap();
        assert_eq!(home.as_str(), "/home");
        let file = home.join("a b.txt").unwrap();
        assert_eq!(file.as_str(), "/home/a b.txt");
        assert_eq!(file.file_name(), Some("a b.txt"));
        assert_eq!(file.parent(), Some(home.clone()));
        assert_eq!(home.parent(), Some(root.clone()));
        assert_eq!(root.parent(), None);
        assert_eq!(root.file_name(), None);
        for bad in ["", ".", "..", "a/b", "a\0"] {
            assert_eq!(home.join(bad), Err(SshError::SftpInvalidPath), "{bad:?}");
        }
    }

    #[test]
    fn malicious_names_are_refused_for_local_writes() {
        let bad = [
            "",
            ".",
            "..",
            "../x",
            "..\\x",
            "a/b",
            "a\\b",
            "C:",
            "C:\\Windows",
            "fichier:flux",
            "/etc/passwd",
            "nul\0.txt",
            "ligne\nsuivante",
            "esc\x1b[31m",
            "facture\u{202E}fdp.exe",
            "zéro\u{200B}largeur",
            "a<b",
            "a>b",
            "a\"b",
            "a|b",
            "a?b",
            "a*b",
            "point.",
            "espace ",
            "CON",
            "con",
            "NUL.txt",
            "Aux.tar.gz",
            "COM1",
            "lpt9.log",
            "CON .txt",
            "COM¹",
            "CONIN$",
        ];
        for name in bad {
            assert_eq!(
                check_local_name(name),
                Err(SshError::SftpUnsafeName),
                "{name:?}"
            );
        }
        let long = "a".repeat(MAX_NAME_LEN + 1);
        assert_eq!(check_local_name(&long), Err(SshError::SftpUnsafeName));
    }

    #[test]
    fn legitimate_names_are_accepted() {
        let good = [
            "rapport.txt",
            "Été 2026.pdf",
            "fichier vide",
            ".bashrc",
            "archive.tar.gz",
            "🙂 emoji",
            "CONTRAT.pdf",
            "console",
            "COM10",
            "a..b",
            "日本語",
            "-tiret",
        ];
        for name in good {
            assert_eq!(check_local_name(name), Ok(()), "{name:?}");
        }
    }

    #[test]
    fn display_names_hide_control_and_invisible_characters() {
        assert_eq!(display_name("normal.txt"), ("normal.txt".to_owned(), false));
        let (shown, suspicious) = display_name("a\x1b[2Jb");
        assert_eq!(shown, "a\u{FFFD}[2Jb");
        assert!(suspicious);
        let (shown, suspicious) = display_name("x\u{202E}txt.exe");
        assert_eq!(shown, "x\u{FFFD}txt.exe");
        assert!(suspicious);
        // Affichable tel quel, mais pas écrivable localement : signalé quand même.
        assert_eq!(display_name("CON"), ("CON".to_owned(), true));
    }

    #[test]
    fn numbered_names_keep_the_extension() {
        assert_eq!(numbered_name("rapport.txt", 1), "rapport (1).txt");
        assert_eq!(numbered_name("archive.tar.gz", 2), "archive.tar (2).gz");
        assert_eq!(numbered_name(".bashrc", 1), ".bashrc (1)");
        assert_eq!(numbered_name("dossier", 3), "dossier (3)");
    }
}
