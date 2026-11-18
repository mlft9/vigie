//! Dossiers locaux choisis par l'utilisateur (« racines locales »).
//!
//! L'interface ne fournit jamais de chemin local absolu : le cœur ouvre la boîte de dialogue
//! du système, garde le dossier choisi et le désigne par un identifiant (ADR-020, ADR-034).
//! Les chemins relatifs envoyés ensuite par l'interface sont résolus **à l'intérieur** de ce
//! dossier : chaque composant est vérifié, puis le chemin est canonicalisé (liens symboliques
//! résolus) et doit rester sous la racine canonicalisée.

use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::SshError;
use crate::sftp::EntryKind;
use crate::sftp::names::MAX_NAME_LEN;

/// Longueur maximale d'un chemin relatif envoyé par l'interface, en octets.
pub const MAX_RELATIVE_PATH: usize = 4096;
/// Nombre maximal d'entrées renvoyées pour un dossier local.
pub const MAX_LOCAL_ENTRIES: usize = 10_000;

/// Élément d'un dossier local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEntry {
    /// Nom (UTF-8 ; les noms non représentables sont omis du listage).
    pub name: String,
    /// Type, sans suivre les liens symboliques.
    pub kind: EntryKind,
    /// Taille en octets (fichiers).
    pub size: u64,
    /// Date de modification (secondes depuis l'époque Unix).
    pub modified: Option<u64>,
}

/// Contenu d'un dossier local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalListing {
    /// Entrées, dossiers d'abord puis par nom.
    pub entries: Vec<LocalEntry>,
    /// Vrai si le dossier contient plus de [`MAX_LOCAL_ENTRIES`] entrées.
    pub truncated: bool,
}

/// Dossiers qui ne peuvent pas devenir une racine locale (ADR-034). Une racine reste une
/// autorisation durable (jusqu'au verrouillage) : une interface compromise pourrait y lire,
/// créer ou écraser des fichiers. On refuse donc :
/// - la racine d'un disque ou du système de fichiers ;
/// - un dossier **égal à ou ancêtre de** l'un des chemins de `ancestors_forbidden` (dossier
///   personnel, données de Younji) : `~` donnerait accès à `.bashrc`, au dossier Démarrage,
///   à `~/.ssh` et au coffre ;
/// - un dossier **égal à ou situé dans** l'un des chemins de `inside_forbidden` (`~/.ssh`,
///   données de Younji, dont le coffre chiffré).
#[derive(Debug, Clone, Default)]
pub struct RootPolicy {
    ancestors_forbidden: Vec<PathBuf>,
    inside_forbidden: Vec<PathBuf>,
}

impl RootPolicy {
    /// Politique à partir de chemins (canonicalisés ici ; un chemin inexistant est ignoré).
    pub fn new(ancestors_forbidden: &[PathBuf], inside_forbidden: &[PathBuf]) -> Self {
        let canonical = |paths: &[PathBuf]| -> Vec<PathBuf> {
            paths
                .iter()
                .filter_map(|p| std::fs::canonicalize(p).ok())
                .collect()
        };
        Self {
            ancestors_forbidden: canonical(ancestors_forbidden),
            inside_forbidden: canonical(inside_forbidden),
        }
    }

    /// Vérifie un dossier **canonicalisé**.
    pub fn check(&self, canonical: &Path) -> Result<(), SshError> {
        let is_fs_root = canonical.parent().is_none();
        let ancestor_of_protected = self
            .ancestors_forbidden
            .iter()
            .any(|protected| protected.starts_with(canonical));
        let inside_protected = self
            .inside_forbidden
            .iter()
            .any(|protected| canonical.starts_with(protected));
        if is_fs_root || ancestor_of_protected || inside_protected {
            Err(SshError::LocalRootForbidden)
        } else {
            Ok(())
        }
    }
}

/// Dossier local choisi par l'utilisateur, canonicalisé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRoot {
    root: PathBuf,
}

impl LocalRoot {
    /// Racine à partir d'un dossier choisi dans la boîte de dialogue du système.
    pub fn new(path: &Path) -> Result<Self, SshError> {
        let root = std::fs::canonicalize(path).map_err(|_| SshError::LocalIo)?;
        if !root.is_dir() {
            return Err(SshError::LocalIo);
        }
        Ok(Self { root })
    }

    /// Racine soumise à une politique (dossier choisi par l'utilisateur pour y naviguer ou y
    /// télécharger) : voir [`RootPolicy`].
    pub fn with_policy(path: &Path, policy: &RootPolicy) -> Result<Self, SshError> {
        let root = Self::new(path)?;
        policy.check(&root.root)?;
        Ok(root)
    }

    /// Chemin canonique de la racine.
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Nom affichable de la racine (dernier composant).
    pub fn display_name(&self) -> String {
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.to_string_lossy().into_owned())
    }

    /// Résout un chemin relatif (`""` = la racine, composants séparés par `/`) en chemin
    /// absolu existant, confiné à la racine. Refuse les composants vides, `.`, `..`, les
    /// séparateurs `\`, `:` et NUL, puis vérifie après canonicalisation (un lien qui sort de
    /// la racine est refusé).
    pub fn resolve(&self, relative: &str) -> Result<PathBuf, SshError> {
        let joined = self.join_unchecked(relative)?;
        let canonical = std::fs::canonicalize(&joined).map_err(|_| SshError::LocalIo)?;
        self.confine(&canonical)?;
        Ok(canonical)
    }

    /// Vérifie qu'un chemin **déjà canonicalisé** est sous la racine.
    pub fn confine(&self, canonical: &Path) -> Result<(), SshError> {
        if canonical.starts_with(&self.root) {
            Ok(())
        } else {
            Err(SshError::LocalOutsideRoot)
        }
    }

    fn join_unchecked(&self, relative: &str) -> Result<PathBuf, SshError> {
        if relative.len() > MAX_RELATIVE_PATH {
            return Err(SshError::LocalOutsideRoot);
        }
        let mut path = self.root.clone();
        if relative.is_empty() {
            return Ok(path);
        }
        for part in relative.split('/') {
            if part.is_empty()
                || part == "."
                || part == ".."
                || part.len() > MAX_NAME_LEN
                || part.contains(['\\', ':', '\0'])
            {
                return Err(SshError::LocalOutsideRoot);
            }
            // Ceinture et bretelles : le composant doit rester un nom simple pour `Path`.
            let mut components = Path::new(part).components();
            if !matches!(
                (components.next(), components.next()),
                (Some(Component::Normal(_)), None)
            ) {
                return Err(SshError::LocalOutsideRoot);
            }
            path.push(part);
        }
        Ok(path)
    }

    /// Chemin relatif (séparateurs `/`) d'un chemin canonicalisé sous la racine.
    pub fn relative(&self, canonical: &Path) -> Option<String> {
        let rest = canonical.strip_prefix(&self.root).ok()?;
        let parts: Option<Vec<&str>> = rest.components().map(|c| c.as_os_str().to_str()).collect();
        Some(parts?.join("/"))
    }

    /// Liste un dossier de la racine (bloquant : à appeler dans `spawn_blocking`).
    pub fn list(&self, relative: &str) -> Result<LocalListing, SshError> {
        let dir = self.resolve(relative)?;
        let reader = std::fs::read_dir(&dir).map_err(|_| SshError::LocalIo)?;
        let mut entries = Vec::new();
        let mut truncated = false;
        for entry in reader {
            let Ok(entry) = entry else { continue };
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if entries.len() >= MAX_LOCAL_ENTRIES {
                truncated = true;
                break;
            }
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            entries.push(LocalEntry {
                name,
                kind: EntryKind::of_std(&meta),
                size: if meta.is_file() { meta.len() } else { 0 },
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs()),
            });
        }
        entries.sort_by(|a, b| {
            (a.kind != EntryKind::Dir)
                .cmp(&(b.kind != EntryKind::Dir))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(LocalListing { entries, truncated })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> (tempfile::TempDir, LocalRoot) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sous dossier/été")).unwrap();
        std::fs::write(dir.path().join("sous dossier/fichier.txt"), b"abc").unwrap();
        let root = LocalRoot::new(dir.path()).unwrap();
        (dir, root)
    }

    #[test]
    fn resolves_inside_the_root() {
        let (_dir, root) = root();
        assert_eq!(root.resolve("").unwrap(), root.path());
        let sub = root.resolve("sous dossier/été").unwrap();
        assert!(sub.starts_with(root.path()));
        assert_eq!(root.relative(&sub).as_deref(), Some("sous dossier/été"));
    }

    #[test]
    fn refuses_escapes() {
        let (_dir, root) = root();
        for bad in [
            "..",
            "../x",
            "sous dossier/../..",
            "/etc",
            "a//b",
            ".",
            "C:\\Windows",
            "C:",
            "sous dossier\\..\\..",
            "a\0b",
        ] {
            let result = root.resolve(bad);
            assert!(
                matches!(result, Err(SshError::LocalOutsideRoot | SshError::LocalIo)),
                "{bad:?} : {result:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlink_that_leaves_the_root() {
        let (dir, root) = root();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("lien")).unwrap();
        assert_eq!(root.resolve("lien"), Err(SshError::LocalOutsideRoot));
    }

    #[test]
    fn sensitive_folders_cannot_become_roots() {
        let home = tempfile::tempdir().unwrap();
        let ssh = home.path().join(".ssh");
        let data = home.path().join("AppData/Younji");
        let docs = home.path().join("Documents/projet");
        for dir in [&ssh, &data, &docs] {
            std::fs::create_dir_all(dir.join("sous")).unwrap();
        }
        let policy = RootPolicy::new(
            &[home.path().to_path_buf(), data.clone()],
            &[ssh.clone(), data.clone()],
        );
        let refused = [
            home.path().to_path_buf(),
            home.path().join("AppData"),
            ssh.clone(),
            ssh.join("sous"),
            data.clone(),
            data.join("sous"),
        ];
        for dir in refused {
            assert_eq!(
                LocalRoot::with_policy(&dir, &policy),
                Err(SshError::LocalRootForbidden),
                "{dir:?}"
            );
        }
        // Racine du système de fichiers (ou du lecteur sous Windows).
        let fs_root = std::fs::canonicalize(home.path())
            .unwrap()
            .ancestors()
            .last()
            .unwrap()
            .to_path_buf();
        assert_eq!(
            LocalRoot::with_policy(&fs_root, &policy),
            Err(SshError::LocalRootForbidden)
        );
        assert!(LocalRoot::with_policy(&docs, &policy).is_ok());
        assert!(LocalRoot::with_policy(&home.path().join("Documents"), &policy).is_ok());
    }

    #[test]
    fn lists_directories_first() {
        let (_dir, root) = root();
        let listing = root.list("sous dossier").unwrap();
        let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["été", "fichier.txt"]);
        assert_eq!(listing.entries[0].kind, EntryKind::Dir);
        assert_eq!(listing.entries[1].size, 3);
        assert!(!listing.truncated);
    }
}
