//! Opérations distantes de haut niveau (SFTP-03) : listage affichable, suppression récursive,
//! création, renommage, droits.

use super::names::{RemotePath, display_name};
use super::{EntryKind, RemoteFs, RemoteMeta, lstat_opt};
use crate::SshError;

/// Nombre maximal d'entrées affichées pour un dossier distant.
pub const MAX_LIST_ENTRIES: usize = 10_000;
/// Nombre maximal de liens symboliques dont la cible est résolue dans un listage (une requête
/// par lien).
const MAX_RESOLVED_LINKS: usize = 256;
/// Profondeur maximale d'un parcours récursif.
pub const MAX_DEPTH: usize = 64;
/// Nombre maximal d'éléments d'une opération récursive (transfert, suppression).
pub const MAX_TREE_ENTRIES: usize = 100_000;

/// Élément d'un listage distant, prêt pour l'interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    /// Nom exact renvoyé par le serveur (pour désigner l'élément dans les opérations).
    pub name: String,
    /// Nom affichable (caractères de contrôle et invisibles remplacés).
    pub display_name: String,
    /// Nom modifié pour l'affichage ou impropre à une écriture locale.
    pub suspicious: bool,
    /// Attributs (sans suivre les liens).
    pub meta: RemoteMeta,
    /// Type de la cible d'un lien symbolique, si elle a pu être résolue.
    pub link_target: Option<EntryKind>,
}

/// Contenu d'un dossier distant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteListing {
    /// Chemin listé (normalisé).
    pub path: RemotePath,
    /// Entrées, dossiers d'abord puis par nom.
    pub entries: Vec<RemoteEntry>,
    /// Vrai si le dossier contient plus de [`MAX_LIST_ENTRIES`] entrées.
    pub truncated: bool,
}

/// Dossier personnel de l'utilisateur (résolution de `.` par le serveur).
pub async fn home<F: RemoteFs>(fs: &F) -> Result<RemotePath, SshError> {
    let raw = fs.realpath(".").await?;
    RemotePath::parse(&raw)
}

/// Liste un dossier. Les entrées dont le nom ne désigne pas un élément unique (`.`, `..`,
/// nom contenant `/` ou NUL, renvoyés par un serveur malveillant) sont écartées : elles
/// feraient viser un autre fichier que celui affiché.
pub async fn list<F: RemoteFs>(fs: &F, path: &RemotePath) -> Result<RemoteListing, SshError> {
    let (raw, truncated) = fs.read_dir(path, MAX_LIST_ENTRIES).await?;
    let mut entries = Vec::with_capacity(raw.len());
    let mut resolved = 0;
    for entry in raw {
        let Ok(full) = path.join(&entry.name) else {
            continue;
        };
        let link_target = if entry.meta.kind == EntryKind::Symlink && resolved < MAX_RESOLVED_LINKS
        {
            resolved += 1;
            fs.stat(&full).await.ok().map(|m| m.kind)
        } else {
            None
        };
        let (shown, suspicious) = display_name(&entry.name);
        entries.push(RemoteEntry {
            name: entry.name,
            display_name: shown,
            suspicious,
            meta: entry.meta,
            link_target,
        });
    }
    entries.sort_by(|a, b| {
        let dir = |e: &RemoteEntry| {
            e.meta.kind == EntryKind::Dir || e.link_target == Some(EntryKind::Dir)
        };
        (!dir(a)).cmp(&!dir(b)).then_with(|| {
            a.display_name
                .to_lowercase()
                .cmp(&b.display_name.to_lowercase())
        })
    });
    Ok(RemoteListing {
        path: path.clone(),
        entries,
        truncated,
    })
}

/// Crée un dossier (échoue s'il existe déjà).
pub async fn mkdir<F: RemoteFs>(fs: &F, path: &RemotePath) -> Result<(), SshError> {
    if lstat_opt(fs, path).await?.is_some() {
        return Err(SshError::SftpAlreadyExists);
    }
    fs.mkdir(path).await
}

/// Renomme sans jamais écraser : la destination ne doit pas exister.
pub async fn rename<F: RemoteFs>(
    fs: &F,
    from: &RemotePath,
    to: &RemotePath,
) -> Result<(), SshError> {
    if from.is_root() || to.is_root() {
        return Err(SshError::SftpInvalidPath);
    }
    if lstat_opt(fs, to).await?.is_some() {
        return Err(SshError::SftpAlreadyExists);
    }
    fs.rename(from, to).await
}

/// Modifie les droits (0 à 0o7777).
pub async fn chmod<F: RemoteFs>(fs: &F, path: &RemotePath, mode: u32) -> Result<(), SshError> {
    if mode > 0o7777 {
        return Err(SshError::SftpInvalidMode);
    }
    fs.chmod(path, mode).await
}

/// Supprime un fichier, un lien (le lien lui-même, jamais sa cible) ou un dossier et tout son
/// contenu. Le parcours ne suit aucun lien et reste borné ([`MAX_DEPTH`],
/// [`MAX_TREE_ENTRIES`]). La racine `/` est refusée.
pub async fn remove<F: RemoteFs>(fs: &F, path: &RemotePath) -> Result<(), SshError> {
    if path.is_root() {
        return Err(SshError::SftpInvalidPath);
    }
    let meta = fs.lstat(path).await?;
    if meta.kind != EntryKind::Dir {
        return fs.remove_file(path).await;
    }
    // Parcours itératif en profondeur : chaque dossier est vidé puis supprimé.
    let mut budget = MAX_TREE_ENTRIES;
    let mut stack: Vec<(RemotePath, usize, bool)> = vec![(path.clone(), 0, false)];
    while let Some((dir, depth, expanded)) = stack.pop() {
        if expanded {
            fs.rmdir(&dir).await?;
            continue;
        }
        if depth >= MAX_DEPTH {
            return Err(SshError::SftpTooManyEntries);
        }
        stack.push((dir.clone(), depth, true));
        let (entries, truncated) = fs.read_dir(&dir, budget).await?;
        if truncated {
            return Err(SshError::SftpTooManyEntries);
        }
        budget = budget.saturating_sub(entries.len());
        for entry in entries {
            // `.`, `..` et les noms à plusieurs composants sont écartés par `join`.
            let Ok(child) = dir.join(&entry.name) else {
                continue;
            };
            if entry.meta.kind == EntryKind::Dir {
                stack.push((child, depth + 1, false));
            } else {
                fs.remove_file(&child).await?;
            }
        }
    }
    Ok(())
}
