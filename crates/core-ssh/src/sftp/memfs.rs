//! Faux système de fichiers distant en mémoire, pour les tests (serveur « malveillant »
//! compris : noms arbitraires, liens, listages énormes).

use std::collections::BTreeMap;
use std::io::Cursor;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use tokio::io::AsyncWrite;

use super::{EntryKind, RawEntry, RemoteFs, RemoteMeta, RemotePath};
use crate::SshError;

#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub kind: EntryKind,
    pub data: Vec<u8>,
    pub mtime: u32,
    pub perms: u32,
    /// Cible d'un lien symbolique (chemin absolu).
    pub target: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct Tree {
    pub nodes: BTreeMap<String, Node>,
    /// Entrées ajoutées telles quelles aux listages (noms malveillants).
    pub raw_names: BTreeMap<String, Vec<RawEntry>>,
    pub clock: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MemFs {
    pub tree: Arc<Mutex<Tree>>,
}

fn parent_of(path: &str) -> String {
    match path.rfind('/') {
        Some(0) | None => "/".to_owned(),
        Some(i) => path[..i].to_owned(),
    }
}

impl MemFs {
    pub fn new() -> Self {
        let fs = Self::default();
        fs.add_dir("/");
        fs
    }

    fn insert(&self, path: &str, kind: EntryKind, data: Vec<u8>, target: Option<String>) {
        let mut tree = self.tree.lock().unwrap();
        tree.clock += 1;
        let mtime = tree.clock;
        tree.nodes.insert(
            path.to_owned(),
            Node {
                kind,
                data,
                mtime,
                perms: if kind == EntryKind::Dir { 0o755 } else { 0o644 },
                target,
            },
        );
    }

    pub fn add_dir(&self, path: &str) {
        self.insert(path, EntryKind::Dir, Vec::new(), None);
    }

    pub fn add_file(&self, path: &str, data: &[u8]) {
        self.insert(path, EntryKind::File, data.to_vec(), None);
    }

    pub fn add_link(&self, path: &str, target: &str) {
        self.insert(
            path,
            EntryKind::Symlink,
            Vec::new(),
            Some(target.to_owned()),
        );
    }

    /// Ajoute une entrée brute au listage de `dir` (nom non filtré, comme un serveur hostile).
    pub fn add_raw(&self, dir: &str, name: &str, kind: EntryKind) {
        let mut tree = self.tree.lock().unwrap();
        tree.raw_names
            .entry(dir.to_owned())
            .or_default()
            .push(RawEntry {
                name: name.to_owned(),
                meta: RemoteMeta {
                    kind,
                    size: 3,
                    modified: None,
                    permissions: None,
                },
            });
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.tree
            .lock()
            .unwrap()
            .nodes
            .get(path)
            .map(|n| n.data.clone())
    }

    pub fn exists(&self, path: &str) -> bool {
        self.tree.lock().unwrap().nodes.contains_key(path)
    }

    pub fn perms(&self, path: &str) -> Option<u32> {
        self.tree.lock().unwrap().nodes.get(path).map(|n| n.perms)
    }

    fn meta(node: &Node) -> RemoteMeta {
        RemoteMeta {
            kind: node.kind,
            size: node.data.len() as u64,
            modified: Some(node.mtime),
            permissions: Some(node.perms),
        }
    }
}

pub(crate) struct MemWriter {
    tree: Arc<Mutex<Tree>>,
    path: String,
}

impl AsyncWrite for MemWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut tree = self.tree.lock().unwrap();
        tree.clock += 1;
        let clock = tree.clock;
        match tree.nodes.get_mut(&self.path) {
            Some(node) => {
                node.data.extend_from_slice(buf);
                node.mtime = clock;
                Poll::Ready(Ok(buf.len()))
            }
            None => Poll::Ready(Err(std::io::Error::other("supprimé"))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl RemoteFs for MemFs {
    type Reader = Cursor<Vec<u8>>;
    type Writer = MemWriter;

    async fn lstat(&self, path: &RemotePath) -> Result<RemoteMeta, SshError> {
        let tree = self.tree.lock().unwrap();
        tree.nodes
            .get(path.as_str())
            .map(Self::meta)
            .ok_or(SshError::SftpNotFound)
    }

    async fn stat(&self, path: &RemotePath) -> Result<RemoteMeta, SshError> {
        let tree = self.tree.lock().unwrap();
        let mut node = tree
            .nodes
            .get(path.as_str())
            .ok_or(SshError::SftpNotFound)?;
        for _ in 0..8 {
            match &node.target {
                Some(target) => node = tree.nodes.get(target).ok_or(SshError::SftpNotFound)?,
                None => return Ok(Self::meta(node)),
            }
        }
        Err(SshError::SftpFailed)
    }

    async fn read_dir(
        &self,
        path: &RemotePath,
        limit: usize,
    ) -> Result<(Vec<RawEntry>, bool), SshError> {
        let tree = self.tree.lock().unwrap();
        match tree.nodes.get(path.as_str()) {
            Some(node) if node.kind == EntryKind::Dir => {}
            Some(_) => return Err(SshError::SftpFailed),
            None => return Err(SshError::SftpNotFound),
        }
        let mut entries = vec![
            RawEntry {
                name: ".".to_owned(),
                meta: Self::meta(&tree.nodes[path.as_str()]),
            },
            RawEntry {
                name: "..".to_owned(),
                meta: Self::meta(&tree.nodes[path.as_str()]),
            },
        ];
        for (child, node) in &tree.nodes {
            if child != "/" && parent_of(child) == path.as_str() {
                let name = child.rsplit('/').next().unwrap().to_owned();
                entries.push(RawEntry {
                    name,
                    meta: Self::meta(node),
                });
            }
        }
        if let Some(raw) = tree.raw_names.get(path.as_str()) {
            entries.extend(raw.iter().cloned());
        }
        let truncated = entries.len() > limit;
        entries.truncate(limit);
        Ok((entries, truncated))
    }

    async fn mkdir(&self, path: &RemotePath) -> Result<(), SshError> {
        if self.exists(path.as_str()) {
            return Err(SshError::SftpFailed);
        }
        if !self.exists(&parent_of(path.as_str())) {
            return Err(SshError::SftpNotFound);
        }
        self.add_dir(path.as_str());
        Ok(())
    }

    async fn rmdir(&self, path: &RemotePath) -> Result<(), SshError> {
        let mut tree = self.tree.lock().unwrap();
        let has_children = tree
            .nodes
            .keys()
            .any(|k| k != path.as_str() && parent_of(k) == path.as_str());
        if has_children {
            return Err(SshError::SftpFailed);
        }
        tree.nodes
            .remove(path.as_str())
            .map(|_| ())
            .ok_or(SshError::SftpNotFound)
    }

    async fn remove_file(&self, path: &RemotePath) -> Result<(), SshError> {
        let mut tree = self.tree.lock().unwrap();
        match tree.nodes.get(path.as_str()) {
            Some(node) if node.kind == EntryKind::Dir => Err(SshError::SftpFailed),
            Some(_) => {
                tree.nodes.remove(path.as_str());
                Ok(())
            }
            None => Err(SshError::SftpNotFound),
        }
    }

    async fn rename(&self, from: &RemotePath, to: &RemotePath) -> Result<(), SshError> {
        let mut tree = self.tree.lock().unwrap();
        if tree.nodes.contains_key(to.as_str()) {
            return Err(SshError::SftpFailed);
        }
        let node = tree
            .nodes
            .remove(from.as_str())
            .ok_or(SshError::SftpNotFound)?;
        tree.nodes.insert(to.as_str().to_owned(), node);
        Ok(())
    }

    async fn chmod(&self, path: &RemotePath, mode: u32) -> Result<(), SshError> {
        let mut tree = self.tree.lock().unwrap();
        let node = tree
            .nodes
            .get_mut(path.as_str())
            .ok_or(SshError::SftpNotFound)?;
        node.perms = mode;
        Ok(())
    }

    async fn realpath(&self, path: &str) -> Result<String, SshError> {
        Ok(if path == "." {
            "/home/test".to_owned()
        } else {
            path.to_owned()
        })
    }

    async fn open_read(&self, path: &RemotePath) -> Result<Self::Reader, SshError> {
        let tree = self.tree.lock().unwrap();
        match tree.nodes.get(path.as_str()) {
            Some(node) if node.kind == EntryKind::File => Ok(Cursor::new(node.data.clone())),
            Some(node) if node.kind == EntryKind::Symlink => {
                let target = node.target.clone().unwrap_or_default();
                tree.nodes
                    .get(&target)
                    .map(|n| Cursor::new(n.data.clone()))
                    .ok_or(SshError::SftpNotFound)
            }
            Some(_) => Err(SshError::SftpFailed),
            None => Err(SshError::SftpNotFound),
        }
    }

    async fn create(&self, path: &RemotePath, exclusive: bool) -> Result<Self::Writer, SshError> {
        if !self.exists(&parent_of(path.as_str())) {
            return Err(SshError::SftpNotFound);
        }
        if self.exists(path.as_str()) {
            if exclusive {
                return Err(SshError::SftpAlreadyExists);
            }
            let mut tree = self.tree.lock().unwrap();
            if let Some(node) = tree.nodes.get_mut(path.as_str()) {
                node.data.clear();
            }
        } else {
            self.add_file(path.as_str(), b"");
        }
        Ok(MemWriter {
            tree: self.tree.clone(),
            path: path.as_str().to_owned(),
        })
    }
}
