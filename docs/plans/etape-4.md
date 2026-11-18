# Plan — Étape 4 : SFTP

> Rédigé en mode autonome (18/11/2026). Branche `etape/4-sftp`, partie de `main` (étape 3 fusionnée, PR #10).

Exigences : SFTP-01 (panneau qui suit le dossier du shell), SFTP-02 (onglet double panneau, glisser-déposer), SFTP-03 (opérations), SFTP-04 (file de transferts, progression, annulation, conflits), SFTP-05 (édition avec l'éditeur local, renvoi à l'enregistrement).
Références : `SECURITE.md` §2 (M3, M4), §4.2 (« SFTP »), §5 (sélection de fichiers), §6 (fichiers temporaires, journaux) ; ADR-004, ADR-020, ADR-024, ADR-026.

API vérifiées dans les sources installées (`russh` 0.65.2, `russh-sftp` 3.0.1) :
- `Channel::request_subsystem(want_reply, "sftp")`, `Channel::into_stream()` (`AsyncRead + AsyncWrite`) ; `client::Handle` n'est pas `Clone` (partagé par `Arc`).
- `russh_sftp::client::SftpSession::new_with_config(stream, Config)` : `open`/`open_with_flags` → `File` (`AsyncRead`/`AsyncWrite`, lectures et écritures **en pipeline**, 16 requêtes en vol) ; paquets reçus bornés par `Config::max_packet_len` (vérifié avant allocation).
- `russh_sftp::client::RawSftpSession` : `init`, `opendir`/`readdir`/`close`, `lstat`, `stat`, `mkdir`, `rmdir`, `remove`, `rename`, `setstat`, `realpath`.
- **Piège relevé** : `SftpSession::read_dir` accumule **toutes** les entrées sans limite (et en temps quadratique) : un serveur malveillant pourrait saturer la mémoire. Les listages passent donc par `RawSftpSession` avec nos propres bornes.

## 1. Architecture retenue

- **Le SFTP réutilise la connexion SSH du terminal** : un canal `sftp` est ouvert sur la session déjà authentifiée (pas de nouvelle authentification, pas de 2FA redemandée, même clé d'hôte vérifiée). Le SFTP vit et meurt avec la session : fermer l'onglet, perdre la connexion ou verrouiller le coffre le ferme.
- Deux canaux par client SFTP : un `RawSftpSession` (métadonnées, listages bornés) et un `SftpSession` (contenu des fichiers en pipeline).
- **Côté local, l'interface ne manipule jamais de chemin absolu** (prolongement d'ADR-020) : un dossier local est choisi dans la boîte de dialogue du système **ouverte par le cœur**, qui renvoie un identifiant opaque (« racine locale »). Le panneau local ne navigue qu'à l'intérieur de cette racine (chemins relatifs revalidés et canonicalisés) ; un téléchargement n'écrit qu'à l'intérieur d'une racine ou d'un dossier choisi dans la boîte de dialogue. Le panneau local ne propose ni suppression ni renommage : une interface compromise ne peut pas détruire de fichiers locaux.
- La logique (noms, racines locales, transferts, conflits, édition) vit dans `core-ssh` (`src/sftp/`), testée sans Tauri via un trait `RemoteFs` (implémenté par le client SFTP réel et par un faux système de fichiers en mémoire pour les tests).

## 2. Sous-tâches (dans l'ordre)

| # | Sous-tâche | Fichiers / crates | Commit(s) |
|---|---|---|---|
| 1 | **Noms et chemins** : chemin distant validé (absolu, ≤ 4 Kio, sans NUL) ; nom distant **sûr pour une écriture locale** (refus de `.`/`..`, séparateurs `/` `\`, `:` (flux NTFS), caractères de contrôle et invisibles/bidirectionnels, `<>"|?*`, noms réservés Windows avec ou sans extension, point ou espace final, > 255 octets) ; nom affichable (caractères suspects remplacés, entrée signalée) ; racine locale (résolution d'un chemin relatif, canonicalisation, confinement) | `crates/core-ssh/src/sftp/{names,local}.rs` | `feat(sftp): …` |
| 2 | **Client SFTP** : ouverture du sous-système sur une session ouverte (`SessionHandle::open_sftp`), listage borné (10 000 entrées affichées, 100 000 parcourues), `stat`, création de dossier, renommage, suppression récursive bornée (liens supprimés, jamais suivis), chmod (0–0o7777), dossier personnel ; erreurs SFTP traduites en codes stables | `crates/core-ssh/src/sftp/{mod,client}.rs`, `session.rs`, `error.rs` | `feat(sftp): …` |
| 3 | **Transferts** : envoi et téléchargement de fichiers et de dossiers récursifs, en flux (tampon fixe), progression limitée à 10 messages/s, annulation, file séquentielle par client, conflits (demander / écraser / ignorer / renommer, « pour tous ») ; téléchargement dans un fichier temporaire du dossier de destination puis renommage (sans écraser si non demandé) ; liens symboliques distants non suivis en récursif ; bornes (profondeur 64, 100 000 éléments) | `crates/core-ssh/src/sftp/transfer.rs` | `feat(sftp): …` |
| 4 | **Édition locale** (SFTP-05) : copie dans un dossier temporaire privé (nom aléatoire, 0700), ouverture dans un **éditeur de texte** (Bloc-notes sous Windows, `xdg-open` sur une copie suffixée `.txt` sous Linux), surveillance des enregistrements, renvoi automatique ; renvoi refusé si le fichier distant a changé entre-temps (conflit signalé) ; 16 Mio max ; dossier supprimé à la fermeture, à la fin de session et au verrouillage | `crates/core-ssh/src/sftp/edit.rs`, `src-tauri/src/editor.rs` | `feat(sftp): …` |
| 5 | **IPC** : `sftp_open`, `sftp_close`, `sftp_home`, `sftp_list`, `sftp_mkdir`, `sftp_rename`, `sftp_remove`, `sftp_chmod`, `sftp_download`, `sftp_upload`, `sftp_transfer_cancel`, `sftp_conflict_decision`, `sftp_edit`, `sftp_edit_close`, `sftp_edit_force`, `local_pick_root`, `local_list` ; `SftpManager` (64 clients, racines locales, éditions) ; fermeture au verrouillage et au rechargement ; `build.rs`, capability, isolation, test de configuration | `src-tauri/src/{sftp_state,commands/sftp}.rs`, `lib.rs`, `build.rs`, `capabilities/main.json`, `isolation/index.js`, `tests/security/` | `feat(ipc): …` |
| 6 | **Interface** : suivi du dossier par OSC 7 (`registerOscHandler(7)`), bouton « Activer le suivi » qui envoie un script constant (bash/zsh) ; panneau latéral repliable dans l'onglet de terminal ; onglet double panneau (local / distant) ouvert depuis un terminal connecté ou la section SFTP ; glisser-déposer entre les panneaux ; dialogues (nom, chmod, conflit, suppression) ; file de transferts | `src/features/sftp/`, `src/stores/{sftp,sessions}.ts`, `src/features/terminal/`, `src/components/layout/`, `src/lib/{ipc,types}.ts`, `src/locales/*.json` | `feat(ui): …` |
| 7 | **Tests d'intégration** contre OpenSSH (dossier récursif avec accents, espaces, fichiers vides ; aller-retour ; annulation) ; ajout de `openssh-sftp-server` à l'image de test | `crates/core-ssh/tests/openssh.rs`, `tests/docker/sshd/Dockerfile` | `test(sftp): …` |
| 8 | `npm run check`, revue de sécurité, QA, documentation | — | `fix(security): …`, `docs: …` |

## 3. Dépendances à ajouter

| Dépendance | Rôle | Licence | Version vérifiée | Remarque |
|---|---|---|---|---|
| `russh-sftp` (crate `core-ssh`) | Protocole SFTP v3 côté client | Apache-2.0 (compatible GPL-3.0) | 3.0.1 (28/09/2026), prévue par le cahier des charges et ADR-002 | Indépendante de russh (travaille sur un flux) ; dépendances : `bytes`, `chrono`, `dashmap`, `bitflags`, `serde`, `log` (journaux non relayés : pas de pont `log` → `tracing`) |
| `tokio` : fonctionnalités `fs`, `io-util` (déjà présent) | Lecture/écriture locale en flux | MIT | 1.54 | |

Aucune dépendance npm : le glisser-déposer utilise l'API HTML5 native.

## 4. Risques de sécurité et parades

| Risque | Parade |
|---|---|
| Nom distant malveillant (`../../x`, `C:\x`, `CON`, `a:flux`, caractères de contrôle) qui fait écrire hors du dossier choisi | Nom validé **avant toute écriture** (liste du §2.1) ; chemin final construit composant par composant puis **canonicalisé et confiné** à la racine ; un nom refusé fait ignorer l'élément (compté en erreur), jamais « corriger » silencieusement |
| Lien symbolique local dans la destination (écriture à travers le lien) | Écriture dans un fichier temporaire du dossier de destination puis **renommage**, qui remplace le lien lui-même ; dossiers créés un par un et revérifiés par canonicalisation |
| Lien symbolique distant (boucle, fuite vers `/etc`) | Non suivi en récursif (attributs `lstat`) ; seul un lien sélectionné explicitement est résolu |
| Écrasement involontaire | Politique de conflit « demander » par défaut ; renommage local sans écrasement (`persist_noclobber`) quand l'utilisateur n'a pas choisi « écraser » |
| Serveur qui inonde un listage ou un parcours récursif (mémoire, CPU) | Listage par `RawSftpSession` avec bornes ; paquets ≤ 256 Kio ; profondeur ≤ 64 ; 100 000 éléments par transfert ; délai par requête |
| Interface compromise qui lit ou écrit des fichiers locaux arbitraires (M4) | Aucun chemin local absolu n'est accepté de l'IPC : racines locales choisies dans la boîte de dialogue du cœur, désignées par identifiant, oubliées au verrouillage ; pas de suppression ni de renommage locaux |
| Fichier distant ouvert par l'application associée (`.bat`, `.js`, `.desktop` exécutés) | Édition **toujours dans un éditeur de texte** : Bloc-notes (chemin système) sous Windows ; sous Linux, copie suffixée `.txt` ouverte par `xdg-open` ; argument de processus, jamais de shell |
| Fichier temporaire d'édition lisible ou laissé sur disque | Dossier temporaire aléatoire 0700 (Unix), supprimé à la fermeture de l'édition, à la fin de la session, au verrouillage (`Drop`) |
| Renvoi d'une édition qui écrase une modification distante | Taille et date distantes comparées avant chaque renvoi : en cas de différence, pas de renvoi, conflit affiché, renvoi forcé seulement sur demande |
| Contenu de fichiers ou noms dans les journaux | Aucun nom ni contenu journalisé ; seuls des compteurs et des codes d'erreur |
| Chemin OSC 7 envoyé par le serveur | Traité comme une donnée non fiable : décodage strict, chemin absolu exigé, revalidé par le cœur ; il ne sert qu'à lister le serveur lui-même |
| Script d'intégration au prompt | Texte **constant**, envoyé seulement sur clic explicite, précédé d'une espace (hors historique), sans donnée de l'utilisateur ni du serveur |
| Glisser-déposer de fichiers du système dans la fenêtre (chemins exposés à la WebView) | Gestionnaire natif de Tauri désactivé pour permettre le glisser-déposer HTML5 interne ; les dépôts venant du système sont ignorés |

## 5. Tests prévus
- **core-ssh (unitaires)** : noms sûrs (cas malveillants et légitimes : accents, espaces, emoji) ; confinement des racines locales (`..`, absolu, lien sortant sous Unix) ; transferts sur faux système de fichiers : dossier récursif (accents, espaces, fichiers vides), nom malveillant ignoré sans écriture hors racine, lien distant non suivi, conflits (chaque politique, « pour tous »), annulation (aucun fichier partiel laissé), progression, bornes ; édition (conflit distant détecté).
- **core-ssh (intégration OpenSSH, ignorés par défaut)** : ouverture SFTP sur une session, listage, envoi puis téléchargement d'un dossier récursif, comparaison du contenu, renommage, chmod, suppression récursive.
- **src-tauri** : désérialisation stricte des requêtes, DTO d'événements, traduction de tous les nouveaux codes.
- **Interface (Vitest)** : décodage OSC 7 ; panneau qui suit le dossier ; liste distante (navigation, sélection) ; file de transferts (progression, annulation) ; dialogue de conflit ; chmod ; garde-fous de configuration.
- **Manuel** : 1 Go avec progression et annulation, interface fluide (plan de test de QA).

## 6. Choix tranchés en autonomie (règle « option la plus sûre »)
1. **SFTP sur la connexion du terminal** plutôt qu'une connexion séparée : aucune authentification supplémentaire, une seule clé d'hôte vérifiée, fermeture garantie avec la session. L'onglet double panneau dépend donc d'un terminal connecté. (ADR)
2. **Racines locales choisies par le cœur**, pas de chemins locaux absolus depuis l'IPC ; panneau local en lecture (navigation) + destination/source des transferts, sans suppression ni renommage. (ADR)
3. **Éditeur imposé** (Bloc-notes / `xdg-open` sur `.txt`) plutôt que l'application associée ; éditeur personnalisé reporté. (ADR)
4. **Fichiers téléchargés en 0600 sous Linux** (permissions de `tempfile`), sans reprendre les droits distants (exécution comprise).
5. **Envoi** : écriture directe du fichier distant (pas de renommage atomique, l'extension `posix-rename` n'est pas exposée par la crate) ; en cas d'annulation, un fichier distant **créé** par le transfert est supprimé ; un fichier écrasé ne peut pas être restauré (limite documentée).
6. **Suivi du dossier** : OSC 7 seulement, avec intégration au prompt sur demande pour bash et zsh ; pas d'analyse du prompt.
7. **Suppression distante d'un dossier** : récursive après confirmation explicite, bornée, sans suivre les liens.
