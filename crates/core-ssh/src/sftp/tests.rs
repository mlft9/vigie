//! Tests des opérations, transferts, file et édition, sur le faux système de fichiers.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::EntryKind;
use super::edit::{EditEvent, EditSession, EditSink};
use super::local::LocalRoot;
use super::memfs::MemFs;
use super::names::RemotePath;
use super::ops;
use super::queue::{TransferEvent, TransferQueue, TransferSink};
use super::transfer::{
    ConflictAction, ConflictDecision, ConflictPolicy, Download, LocalDest, Progress,
    TransferReporter, Upload, download, upload,
};
use crate::SshError;

fn p(path: &str) -> RemotePath {
    RemotePath::parse(path).unwrap()
}

/// Rapporteur de test : enregistre la progression, répond aux conflits par une décision fixe
/// et peut annuler le transfert après le premier bloc copié.
struct TestReporter {
    decision: ConflictDecision,
    conflicts: AtomicU32,
    last: Mutex<Progress>,
    cancel_on_progress: Option<CancellationToken>,
}

impl TestReporter {
    fn new(action: ConflictAction, apply_to_all: bool) -> Self {
        Self {
            decision: ConflictDecision {
                action,
                apply_to_all,
            },
            conflicts: AtomicU32::new(0),
            last: Mutex::new(Progress::default()),
            cancel_on_progress: None,
        }
    }
}

impl TransferReporter for TestReporter {
    fn progress(&self, progress: Progress) {
        *self.last.lock().unwrap() = progress;
        // Annulation après le premier bloc copié (le fichier est alors en cours d'écriture).
        if progress.bytes_done > 0
            && let Some(cancel) = &self.cancel_on_progress
        {
            cancel.cancel();
        }
    }

    fn conflict(
        &self,
        _name: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ConflictDecision> + Send + '_>> {
        self.conflicts.fetch_add(1, Ordering::SeqCst);
        let decision = self.decision;
        Box::pin(async move { decision })
    }
}

fn dest(dir: &Path) -> LocalDest {
    let root = LocalRoot::new(dir).unwrap();
    LocalDest {
        dir: root.path().to_path_buf(),
        root,
    }
}

/// Arborescence distante avec accents, espaces et fichiers vides.
fn sample_tree() -> MemFs {
    let fs = MemFs::new();
    fs.add_dir("/home");
    fs.add_dir("/home/test");
    fs.add_dir("/home/test/Projet été");
    fs.add_file("/home/test/Projet été/lisez moi.txt", b"bonjour");
    fs.add_file("/home/test/Projet été/vide", b"");
    fs.add_dir("/home/test/Projet été/sous dossier");
    fs.add_file(
        "/home/test/Projet été/sous dossier/données ü.bin",
        &[7u8; 1000],
    );
    fs.add_dir("/home/test/Projet été/dossier vide");
    fs
}

#[tokio::test]
async fn listing_filters_impossible_names_and_flags_suspicious_ones() {
    let fs = sample_tree();
    fs.add_raw("/home/test", "../../etc", EntryKind::File);
    fs.add_raw("/home/test", "a/b", EntryKind::File);
    fs.add_raw("/home/test", "CON", EntryKind::File);
    fs.add_raw("/home/test", "x\x1b[2J", EntryKind::File);
    fs.add_link("/home/test/lien", "/home/test/Projet été");
    let listing = ops::list(&fs, &p("/home/test")).await.unwrap();
    let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(!names.contains(&"."));
    assert!(!names.contains(&".."));
    assert!(!names.contains(&"../../etc"));
    assert!(!names.contains(&"a/b"));
    let con = listing.entries.iter().find(|e| e.name == "CON").unwrap();
    assert!(con.suspicious);
    let esc = listing
        .entries
        .iter()
        .find(|e| e.name == "x\x1b[2J")
        .unwrap();
    assert_eq!(esc.display_name, "x\u{FFFD}[2J");
    let link = listing.entries.iter().find(|e| e.name == "lien").unwrap();
    assert_eq!(link.link_target, Some(EntryKind::Dir));
    // Dossiers (et liens vers des dossiers) d'abord.
    assert!(matches!(names.first(), Some(&"lien" | &"Projet été")));
}

#[tokio::test]
async fn remove_is_recursive_and_does_not_follow_links() {
    let fs = sample_tree();
    fs.add_dir("/precieux");
    fs.add_file("/precieux/garder.txt", b"x");
    fs.add_link("/home/test/Projet été/lien", "/precieux");
    ops::remove(&fs, &p("/home/test/Projet été")).await.unwrap();
    assert!(!fs.exists("/home/test/Projet été"));
    assert!(fs.exists("/precieux/garder.txt"));
    assert_eq!(
        ops::remove(&fs, &RemotePath::root()).await,
        Err(SshError::SftpInvalidPath)
    );
}

#[tokio::test]
async fn rename_never_overwrites_and_chmod_is_bounded() {
    let fs = sample_tree();
    let a = p("/home/test/Projet été/lisez moi.txt");
    let b = p("/home/test/Projet été/vide");
    assert_eq!(
        ops::rename(&fs, &a, &b).await,
        Err(SshError::SftpAlreadyExists)
    );
    let c = p("/home/test/Projet été/nouveau nom.txt");
    ops::rename(&fs, &a, &c).await.unwrap();
    assert_eq!(fs.read(c.as_str()).unwrap(), b"bonjour");
    ops::chmod(&fs, &c, 0o600).await.unwrap();
    assert_eq!(fs.perms(c.as_str()), Some(0o600));
    assert_eq!(
        ops::chmod(&fs, &c, 0o10000).await,
        Err(SshError::SftpInvalidMode)
    );
    assert_eq!(
        ops::mkdir(&fs, &p("/home/test/Projet été/vide")).await,
        Err(SshError::SftpAlreadyExists)
    );
    assert_eq!(ops::home(&fs).await.unwrap().as_str(), "/home/test");
}

#[tokio::test]
async fn downloads_a_recursive_folder_with_accents_spaces_and_empty_files() {
    let fs = sample_tree();
    let local = tempfile::tempdir().unwrap();
    let reporter = TestReporter::new(ConflictAction::Skip, false);
    let summary = download(
        &fs,
        Download {
            sources: vec![p("/home/test/Projet été")],
            dest: dest(local.path()),
            policy: ConflictPolicy::Ask,
        },
        &reporter,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(summary.error, None, "{summary:?}");
    assert_eq!(summary.transferred, 3);
    let base = local.path().join("Projet été");
    assert_eq!(
        std::fs::read(base.join("lisez moi.txt")).unwrap(),
        b"bonjour"
    );
    assert_eq!(std::fs::read(base.join("vide")).unwrap(), b"");
    assert_eq!(
        std::fs::read(base.join("sous dossier/données ü.bin")).unwrap(),
        vec![7u8; 1000]
    );
    assert!(base.join("dossier vide").is_dir());
    let last = *reporter.last.lock().unwrap();
    assert_eq!(last.bytes_done, 1007);
    assert_eq!(last.files_done, 3);
    // Aucun fichier temporaire laissé.
    let leftovers: Vec<_> = walk(local.path())
        .into_iter()
        .filter(|p| p.to_string_lossy().contains(".younji-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        }
        out.push(path);
    }
    out
}

#[tokio::test]
async fn malicious_remote_names_never_write_outside_the_destination() {
    let fs = MemFs::new();
    fs.add_dir("/piege");
    fs.add_file("/piege/normal.txt", b"ok");
    for name in [
        "../evade.txt",
        "..",
        "..\\evade.txt",
        "/etc/evade",
        "C:\\evade.txt",
        "evade:flux",
        "CON",
        "nul.txt",
        "a\nb",
        "facture\u{202E}fdp.exe",
        "point.",
    ] {
        fs.add_raw("/piege", name, EntryKind::File);
    }
    let parent = tempfile::tempdir().unwrap();
    let target = parent.path().join("destination");
    std::fs::create_dir(&target).unwrap();
    let reporter = TestReporter::new(ConflictAction::Skip, false);
    let summary = download(
        &fs,
        Download {
            sources: vec![p("/piege")],
            dest: dest(&target),
            policy: ConflictPolicy::Overwrite,
        },
        &reporter,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(summary.transferred, 1);
    assert!(summary.refused >= 9, "{summary:?}");
    // Le parent ne contient que le dossier de destination ; celle-ci, que le dossier copié.
    let outside: Vec<_> = std::fs::read_dir(parent.path())
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(outside.len(), 1);
    let copied: Vec<_> = walk(&target);
    assert_eq!(copied.len(), 2, "{copied:?}");
    assert_eq!(
        std::fs::read(target.join("piege/normal.txt")).unwrap(),
        b"ok"
    );
}

#[tokio::test]
async fn remote_symlinks_are_not_followed_in_recursive_downloads() {
    let fs = sample_tree();
    fs.add_dir("/etc");
    fs.add_file("/etc/shadow", b"secret");
    fs.add_link("/home/test/Projet été/vers etc", "/etc");
    fs.add_link("/home/test/Projet été/boucle", "/home/test/Projet été");
    let local = tempfile::tempdir().unwrap();
    let reporter = TestReporter::new(ConflictAction::Skip, false);
    let summary = download(
        &fs,
        Download {
            sources: vec![p("/home/test/Projet été")],
            dest: dest(local.path()),
            policy: ConflictPolicy::Ask,
        },
        &reporter,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(summary.ignored, 2);
    assert!(!local.path().join("Projet été/vers etc").exists());
    assert!(!local.path().join("Projet été/boucle").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn a_local_symlink_in_the_destination_is_replaced_not_followed() {
    let fs = sample_tree();
    let local = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let victim = outside.path().join("victime.txt");
    std::fs::write(&victim, b"intact").unwrap();
    std::os::unix::fs::symlink(&victim, local.path().join("lisez moi.txt")).unwrap();
    let reporter = TestReporter::new(ConflictAction::Overwrite, true);
    let summary = download(
        &fs,
        Download {
            sources: vec![p("/home/test/Projet été/lisez moi.txt")],
            dest: dest(local.path()),
            policy: ConflictPolicy::Ask,
        },
        &reporter,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(summary.transferred, 1);
    assert_eq!(std::fs::read(&victim).unwrap(), b"intact");
    let written = local.path().join("lisez moi.txt");
    assert!(!written.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read(written).unwrap(), b"bonjour");
}

#[tokio::test]
async fn conflict_policies_are_applied() {
    let fs = sample_tree();
    let local = tempfile::tempdir().unwrap();
    std::fs::write(local.path().join("lisez moi.txt"), b"ancien").unwrap();
    std::fs::write(local.path().join("vide"), b"ancien").unwrap();
    let job = |policy| Download {
        sources: vec![
            p("/home/test/Projet été/lisez moi.txt"),
            p("/home/test/Projet été/vide"),
        ],
        dest: dest(local.path()),
        policy,
    };
    let cancel = CancellationToken::new();

    // Ignorer : rien n'est écrasé.
    let reporter = TestReporter::new(ConflictAction::Skip, false);
    let summary = download(&fs, job(ConflictPolicy::Skip), &reporter, &cancel).await;
    assert_eq!((summary.transferred, summary.skipped), (0, 2));
    assert_eq!(std::fs::read(local.path().join("vide")).unwrap(), b"ancien");

    // Demander, « renommer pour tous » : une seule question, deux copies renommées.
    let reporter = TestReporter::new(ConflictAction::Rename, true);
    let summary = download(&fs, job(ConflictPolicy::Ask), &reporter, &cancel).await;
    assert_eq!(summary.transferred, 2);
    assert_eq!(reporter.conflicts.load(Ordering::SeqCst), 1);
    assert_eq!(
        std::fs::read(local.path().join("lisez moi (1).txt")).unwrap(),
        b"bonjour"
    );
    assert_eq!(std::fs::read(local.path().join("vide (1)")).unwrap(), b"");

    // Demander sans « pour tous » : une question par conflit.
    let reporter = TestReporter::new(ConflictAction::Overwrite, false);
    let summary = download(&fs, job(ConflictPolicy::Ask), &reporter, &cancel).await;
    assert_eq!(summary.transferred, 2);
    assert_eq!(reporter.conflicts.load(Ordering::SeqCst), 2);
    assert_eq!(
        std::fs::read(local.path().join("lisez moi.txt")).unwrap(),
        b"bonjour"
    );
}

#[tokio::test]
async fn a_cancelled_download_leaves_no_partial_file() {
    let fs = MemFs::new();
    fs.add_file("/gros.bin", &vec![1u8; 3 * 1024 * 1024]);
    let local = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let mut reporter = TestReporter::new(ConflictAction::Skip, false);
    reporter.cancel_on_progress = Some(cancel.clone());
    let summary = download(
        &fs,
        Download {
            sources: vec![p("/gros.bin")],
            dest: dest(local.path()),
            policy: ConflictPolicy::Ask,
        },
        &reporter,
        &cancel,
    )
    .await;
    assert_eq!(summary.error, Some(SshError::TransferCancelled));
    assert!(walk(local.path()).is_empty(), "{:?}", walk(local.path()));
}

#[tokio::test]
async fn uploads_a_recursive_folder_and_handles_conflicts() {
    let fs = MemFs::new();
    fs.add_dir("/srv");
    let local = tempfile::tempdir().unwrap();
    let src = local.path().join("Dossier à envoyer");
    std::fs::create_dir_all(src.join("sous dossier")).unwrap();
    std::fs::create_dir_all(src.join("vide")).unwrap();
    std::fs::write(src.join("fichier vide"), b"").unwrap();
    std::fs::write(src.join("sous dossier/é è.txt"), b"contenu").unwrap();
    let cancel = CancellationToken::new();
    let job = |policy| Upload {
        sources: vec![std::fs::canonicalize(&src).unwrap()],
        dest: p("/srv"),
        policy,
    };

    let reporter = TestReporter::new(ConflictAction::Skip, false);
    let summary = upload(&fs, job(ConflictPolicy::Ask), &reporter, &cancel).await;
    assert_eq!(summary.error, None, "{summary:?}");
    assert_eq!(summary.transferred, 2);
    assert_eq!(fs.read("/srv/Dossier à envoyer/fichier vide").unwrap(), b"");
    assert_eq!(
        fs.read("/srv/Dossier à envoyer/sous dossier/é è.txt")
            .unwrap(),
        b"contenu"
    );
    assert!(fs.exists("/srv/Dossier à envoyer/vide"));

    // Second envoi : tout est en conflit ; « ignorer » ne touche à rien.
    let reporter = TestReporter::new(ConflictAction::Skip, true);
    let summary = upload(&fs, job(ConflictPolicy::Ask), &reporter, &cancel).await;
    assert_eq!((summary.transferred, summary.skipped), (0, 2));
    assert_eq!(reporter.conflicts.load(Ordering::SeqCst), 1);

    // « Renommer » : nouvelle copie à côté.
    let reporter = TestReporter::new(ConflictAction::Rename, true);
    let summary = upload(&fs, job(ConflictPolicy::Rename), &reporter, &cancel).await;
    assert_eq!(summary.transferred, 2);
    assert!(fs.exists("/srv/Dossier à envoyer/sous dossier/é è (1).txt"));
}

#[cfg(unix)]
#[tokio::test]
async fn local_symlinks_are_not_followed_in_uploads() {
    let fs = MemFs::new();
    fs.add_dir("/srv");
    let local = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), b"x").unwrap();
    let src = local.path().join("envoi");
    std::fs::create_dir(&src).unwrap();
    std::os::unix::fs::symlink(outside.path(), src.join("lien")).unwrap();
    let reporter = TestReporter::new(ConflictAction::Skip, false);
    let summary = upload(
        &fs,
        Upload {
            sources: vec![src],
            dest: p("/srv"),
            policy: ConflictPolicy::Ask,
        },
        &reporter,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(summary.ignored, 1);
    assert!(!fs.exists("/srv/envoi/lien"));
}

#[tokio::test]
async fn a_cancelled_upload_removes_the_partial_remote_file() {
    let fs = MemFs::new();
    fs.add_dir("/srv");
    let local = tempfile::tempdir().unwrap();
    let file = local.path().join("gros.bin");
    std::fs::write(&file, vec![2u8; 3 * 1024 * 1024]).unwrap();
    let cancel = CancellationToken::new();
    let mut reporter = TestReporter::new(ConflictAction::Skip, false);
    reporter.cancel_on_progress = Some(cancel.clone());
    let summary = upload(
        &fs,
        Upload {
            sources: vec![file],
            dest: p("/srv"),
            policy: ConflictPolicy::Ask,
        },
        &reporter,
        &cancel,
    )
    .await;
    assert_eq!(summary.error, Some(SshError::TransferCancelled));
    assert!(!fs.exists("/srv/gros.bin"));
}

#[tokio::test]
async fn huge_remote_trees_are_refused() {
    let fs = MemFs::new();
    fs.add_dir("/profond");
    let mut path = "/profond".to_owned();
    for i in 0..(ops::MAX_DEPTH + 2) {
        path.push_str(&format!("/d{i}"));
        fs.add_dir(&path);
    }
    let local = tempfile::tempdir().unwrap();
    let reporter = TestReporter::new(ConflictAction::Skip, false);
    let summary = download(
        &fs,
        Download {
            sources: vec![p("/profond")],
            dest: dest(local.path()),
            policy: ConflictPolicy::Ask,
        },
        &reporter,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(summary.error, Some(SshError::SftpTooManyEntries));
    assert!(walk(local.path()).is_empty());
}

// ---------------------------------------------------------------------- file de transferts

#[derive(Default)]
struct QueueSink(Mutex<Vec<TransferEvent>>);

impl TransferSink for QueueSink {
    fn event(&self, event: TransferEvent) {
        self.0.lock().unwrap().push(event);
    }
}

async fn wait_for(sink: &QueueSink, predicate: impl Fn(&TransferEvent) -> bool) -> TransferEvent {
    for _ in 0..500 {
        if let Some(event) = sink.0.lock().unwrap().iter().find(|e| predicate(e)) {
            return event.clone();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("événement attendu non reçu");
}

#[tokio::test]
async fn the_queue_asks_about_conflicts_and_can_cancel() {
    let fs = Arc::new(sample_tree());
    let local = tempfile::tempdir().unwrap();
    std::fs::write(local.path().join("lisez moi.txt"), b"ancien").unwrap();
    let sink = Arc::new(QueueSink::default());
    let queue = TransferQueue::start(fs.clone(), sink.clone());

    let job = queue
        .submit(super::queue::JobSpec::Download(Download {
            sources: vec![p("/home/test/Projet été/lisez moi.txt")],
            dest: dest(local.path()),
            policy: ConflictPolicy::Ask,
        }))
        .unwrap();
    let conflict = wait_for(&sink, |e| matches!(e, TransferEvent::Conflict { .. })).await;
    assert_eq!(
        conflict,
        TransferEvent::Conflict {
            job,
            name: "lisez moi.txt".to_owned()
        }
    );
    // Une décision pour un autre transfert est refusée.
    let decision = ConflictDecision {
        action: ConflictAction::Overwrite,
        apply_to_all: false,
    };
    assert_eq!(
        queue.decide(job + 1000, decision),
        Err(SshError::TransferNotFound)
    );
    queue.decide(job, decision).unwrap();
    let finished = wait_for(&sink, |e| matches!(e, TransferEvent::Finished { .. })).await;
    let TransferEvent::Finished { summary, .. } = finished else {
        unreachable!()
    };
    assert_eq!(summary.transferred, 1);
    assert_eq!(
        std::fs::read(local.path().join("lisez moi.txt")).unwrap(),
        b"bonjour"
    );

    // Annulation pendant une question de conflit.
    let job = queue
        .submit(super::queue::JobSpec::Download(Download {
            sources: vec![p("/home/test/Projet été/lisez moi.txt")],
            dest: dest(local.path()),
            policy: ConflictPolicy::Ask,
        }))
        .unwrap();
    wait_for(
        &sink,
        |e| matches!(e, TransferEvent::Conflict { job: j, .. } if *j == job),
    )
    .await;
    queue.cancel(job).unwrap();
    let finished = wait_for(
        &sink,
        |e| matches!(e, TransferEvent::Finished { job: j, .. } if *j == job),
    )
    .await;
    let TransferEvent::Finished { summary, .. } = finished else {
        unreachable!()
    };
    assert_eq!(summary.error, Some(SshError::TransferCancelled));
    assert_eq!(queue.cancel(job), Err(SshError::TransferNotFound));
}

// ---------------------------------------------------------------------- édition

#[derive(Default)]
struct EditEvents(Mutex<Vec<EditEvent>>);

impl EditSink for EditEvents {
    fn event(&self, event: EditEvent) {
        self.0.lock().unwrap().push(event);
    }
}

async fn wait_edit(events: &EditEvents, expected: EditEvent) {
    for _ in 0..300 {
        if events.0.lock().unwrap().contains(&expected) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("événement d'édition attendu : {expected:?}");
}

#[tokio::test]
async fn edits_are_uploaded_on_save_and_conflicts_are_detected() {
    let fs = Arc::new(sample_tree());
    let remote = p("/home/test/Projet été/lisez moi.txt");
    let events = Arc::new(EditEvents::default());
    let (session, local) = EditSession::start(
        fs.clone(),
        remote.clone(),
        ".txt",
        Duration::from_millis(20),
        events.clone(),
    )
    .await
    .unwrap();
    assert!(local.ends_with("lisez moi.txt.txt"));
    assert_eq!(std::fs::read(&local).unwrap(), b"bonjour");

    std::fs::write(&local, b"bonjour modifie").unwrap();
    wait_edit(&events, EditEvent::Uploaded).await;
    assert_eq!(fs.read(remote.as_str()).unwrap(), b"bonjour modifie");

    // Quelqu'un modifie le fichier distant : le renvoi suivant est suspendu.
    fs.add_file(remote.as_str(), b"modifie par un autre");
    std::fs::write(&local, b"ma version, plus longue").unwrap();
    wait_edit(&events, EditEvent::Conflict).await;
    assert_eq!(fs.read(remote.as_str()).unwrap(), b"modifie par un autre");

    // Renvoi forcé.
    events.0.lock().unwrap().clear();
    session.force_upload();
    wait_edit(&events, EditEvent::Uploaded).await;
    assert_eq!(
        fs.read(remote.as_str()).unwrap(),
        b"ma version, plus longue"
    );

    // Fermeture : la copie locale disparaît.
    let dir = local.parent().unwrap().to_path_buf();
    session.close();
    for _ in 0..100 {
        if !dir.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!dir.exists());
}

#[tokio::test]
async fn edit_refuses_unsafe_names_and_huge_files() {
    let fs = Arc::new(MemFs::new());
    fs.add_file("/CON", b"x");
    fs.add_file(
        "/gros",
        &vec![0u8; (super::edit::MAX_EDIT_SIZE + 1) as usize],
    );
    fs.add_dir("/dossier");
    let events: Arc<dyn EditSink> = Arc::new(EditEvents::default());
    let start = |path: &str| {
        EditSession::start(
            fs.clone(),
            p(path),
            "",
            Duration::from_millis(20),
            events.clone(),
        )
    };
    assert_eq!(start("/CON").await.err(), Some(SshError::SftpUnsafeName));
    assert_eq!(start("/gros").await.err(), Some(SshError::EditTooLarge));
    assert_eq!(start("/dossier").await.err(), Some(SshError::SftpNotFile));
}
