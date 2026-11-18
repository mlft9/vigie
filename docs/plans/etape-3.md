# Plan — Étape 3 : 2FA et générateur de clés

> Rédigé en mode autonome (18/11/2026). Branche `etape/3-2fa-cles`, partie de `main` (étape 2 fusionnée, PR #8).

Exigences : AUTH-03 (keyboard-interactive / 2FA), KEY-01 (générateur de clés), KEY-02 (export de la clé publique), KEY-03 (déploiement idempotent, permissions correctes).
Références : `SECURITE.md` §4.1, §4.2 (« Déploiement de clé (KEY-03) », « Messages du serveur »), §5, §6 ; ADR-019, ADR-020, ADR-026, ADR-030.

API vérifiées dans les sources installées (`russh` 0.65.2, `ssh-key` 0.7.0-rc.12, `getrandom` 0.4.3) :
- `Handle::authenticate_keyboard_interactive_start(user, submethods)` / `authenticate_keyboard_interactive_respond(Vec<String>)` → `KeyboardInteractiveAuthResponse::{Success, Failure { remaining_methods, partial_success }, InfoRequest { name, instructions, prompts: Vec<Prompt { prompt, echo }> }}`.
- `AuthResult::Failure { remaining_methods: MethodSet, partial_success }` (`MethodKind::{Password, PublicKey, KeyboardInteractive…}`).
- `PrivateKey::random(&mut impl CryptoRng, Algorithm)` (RSA : 4096 bits par défaut) ; `getrandom::SysRng` (fonctionnalité `sys_rng`) + `rand_core::UnwrapErr` = générateur du système.
- Serveur russh de test : `Handler::auth_keyboard_interactive` et `Auth::Partial { name, instructions, prompts }`.

## 1. Sous-tâches (dans l'ordre)

| # | Sous-tâche | Fichiers / crates | Commit(s) |
|---|---|---|---|
| 1 | **Keyboard-interactive** dans `core-ssh` : enchaînement clé → mot de passe → keyboard-interactive selon les méthodes restantes annoncées par le serveur (succès partiel compris) ; questions du serveur nettoyées et bornées, envoyées à l'appelant (`SessionEvent::AuthPrompt`) ; réponses par `SessionHandle::answer_prompts` ; délai de réponse propre (`Timeouts::auth_prompt`) ; mot de passe connu proposé **une seule fois** à une question « Password: » sans écho ; nouveau moyen `Credentials::Interactive` (aucun secret fourni d'avance) | `crates/core-ssh/src/{auth,session,config,error}.rs`, `tests/local_server.rs` | `feat(ssh): …` |
| 2 | **Génération de clés** : Ed25519 (défaut), ECDSA P-256/P-384/P-521, RSA 4096, aléa du système ; clé stockée dans le coffre comme une clé importée | `crates/core-ssh/src/keygen.rs` | `feat(ssh): …` |
| 3 | **Déploiement de clé** : nouvelle tâche `SessionHandle::spawn_deploy` (même connexion, même TOFU, même 2FA) qui exécute une **commande fixe** et transmet la clé publique **par l'entrée standard** ; script POSIX idempotent (700 / 600, pas de doublon, fin de ligne manquante gérée) ; résultat `added` / `present` | `crates/core-ssh/src/{deploy,session}.rs`, tests locaux + intégration OpenSSH | `feat(ssh): …` |
| 4 | **IPC** : `ssh_auth_answer`, `key_generate`, `key_export_public` (boîte de dialogue d'enregistrement ouverte par le cœur), `key_deploy` ; `build.rs`, capabilities, liste blanche de l'isolation ; nouveaux codes d'erreur traduits | `src-tauri/src/commands/{ssh,book}.rs`, `src-tauri/build.rs`, `capabilities/main.json`, `isolation/index.js`, `src-tauri/src/error.rs` | `feat(ipc): …` |
| 5 | **Interface** : dialogue des questions du serveur (2FA) dans l'onglet de session ; mode « Clavier interactif (2FA) » dans la connexion rapide ; génération de clé, export `.pub`, déploiement sur un hôte du carnet (avec les mêmes dialogues de clé d'hôte et de 2FA) | `src/features/connect/`, `src/features/terminal/`, `src/features/keys/`, `src/lib/{ipc,types}.ts`, `src/locales/*.json` | `feat(ui): …` |
| 6 | Tests, `npm run check`, revue de sécurité, QA, documentation | — | `test(…)`, `fix(security): …`, `docs: …` |

## 2. Dépendances à ajouter

| Dépendance | Rôle | Licence | Version vérifiée | Remarque |
|---|---|---|---|---|
| `getrandom` (crate `core-ssh`) avec `sys_rng` | Générateur du système (`SysRng`, équivalent d'`OsRng`) pour la génération de clés | MIT OR Apache-2.0 | 0.4.3 (déjà dans l'arbre, utilisée par `core-vault`) | `sys_rng` réexporte `rand_core` 0.10, déjà dans l'arbre via `ssh-key` |

Aucune dépendance npm. Pas de `russh-sftp` à cette étape : le déploiement passe par l'entrée standard d'une commande fixe (`SECURITE.md` §4.2 l'autorise), SFTP arrive à l'étape 4.

## 3. Risques de sécurité et parades

| Risque | Parade |
|---|---|
| Serveur malveillant qui pilote l'interface par ses questions (séquences de contrôle, texte énorme, centaines de questions, boucle infinie de tours) | Texte nettoyé (caractères de contrôle retirés), bornes : nom 256, instructions 2 Kio, question 512 octets, 8 questions par tour, 16 tours au plus ; affichage en texte brut (React), jamais en HTML |
| Hameçonnage : un serveur demande « Mot de passe maître Younji » | Le dialogue indique clairement que les questions **viennent du serveur** (nom de l'hôte affiché) et rappelle que Younji ne demande jamais le mot de passe maître à cet endroit |
| Fuite du mot de passe enregistré vers une question choisie par le serveur | Remplissage automatique limité : une seule fois par connexion, question unique **sans écho** dont le texte est exactement « Password » / « Mot de passe » (casse, espaces et « : » ignorés), et seulement si le mot de passe n'a pas déjà été refusé par la méthode `password`. Le serveur a déjà prouvé son identité (TOFU) : c'est le même destinataire que l'authentification par mot de passe |
| Réponses (codes TOTP, mots de passe) dans les journaux ou renvoyées à l'interface | `SecretString` dès la désérialisation ; jamais journalisées ; jamais renvoyées ; champs vidés dès l'envoi ; nombre de réponses et longueur vérifiés côté Rust |
| Interface compromise qui répond à la place de l'utilisateur ou à une session qui n'attend rien | Réponse acceptée seulement si une question est en attente (`ssh.no_pending_prompt`), avec exactement le bon nombre de réponses |
| Session bloquée indéfiniment sur une question | Délai `auth_prompt` (2 min) par tour ; fermeture de l'onglet ou verrouillage = annulation ; le délai réseau de l'authentification ne compte plus le temps de saisie |
| Injection de commande au déploiement | Commande **constante** (aucune donnée concaténée) ; la ligne de clé passe par l'entrée standard, re-dérivée de la clé privée du coffre et revalidée (`algo base64`, caractères autorisés) ; un test vérifie que la commande ne contient aucune donnée de la clé |
| Déploiement qui casse `authorized_keys` (doublon, ligne collée à la précédente, permissions trop larges) | Script POSIX testé localement avec `sh` : `umask 077`, `chmod 700 ~/.ssh`, `chmod 600 authorized_keys`, recherche exacte (`grep -F`) avant ajout, retour à la ligne ajouté si le fichier n'en a pas ; sortie attendue strictement reconnue (`YOUNJI_ADDED` / `YOUNJI_PRESENT`), sinon échec |
| Faiblesse de l'aléa à la génération | `getrandom::SysRng` (aléa du système) ; aucune graine maison ; RSA 4096 minimum, pas de DSA |
| Export de la clé publique qui écrit n'importe où | Chemin choisi dans la boîte de dialogue du système **ouverte par le cœur** ; l'interface ne fournit pas de chemin (ADR-020) |
| Clé privée qui sort du coffre | Aucun export de clé privée (ADR-030 maintenu) ; la clé générée ne quitte jamais le cœur |

## 4. Tests prévus
- **core-ssh (unitaires)** : nettoyage et bornes des questions ; reconnaissance stricte d'une question « mot de passe » ; validation de la ligne de clé publique ; commande de déploiement constante ; script de déploiement exécuté avec `sh` dans un dossier temporaire (ajout, second passage sans doublon, fichier sans fin de ligne, permissions 700/600 sous Unix) — ignoré proprement si `sh` est absent ; génération de chaque algorithme (Ed25519, ECDSA ×3, RSA 4096) relisible et ré-encodable ; `Debug` sans secret.
- **core-ssh (serveur russh local)** : keyboard-interactive seul (question → réponse → succès) ; mot de passe + code (succès partiel, le mot de passe est rempli automatiquement, seul le code est demandé) ; mauvaise réponse → `ssh.auth_failed` ; nombre de réponses incorrect refusé ; annulation ; délai de réponse dépassé ; serveur qui envoie trop de questions → refus ; déploiement : la commande reçue est la constante, la clé arrive par l'entrée standard.
- **core-ssh (intégration OpenSSH, ignorée par défaut)** : déployer une clé générée sur `testpwd` (mot de passe), redéployer (pas de doublon), se connecter par clé, vérifier `stat` 700/600, puis nettoyer la ligne ajoutée.
- **src-tauri** : désérialisation stricte des nouvelles requêtes ; DTO d'événement `authPrompt` / `keyDeployed` ; traduction de tous les nouveaux codes.
- **Interface (Vitest)** : dialogue des questions (écho / sans écho, champs vidés, annulation, avertissement d'hameçonnage) ; connexion rapide en mode interactif ; génération de clé ; export ; déploiement (succès, déjà présente, erreur) ; garde-fous de configuration (`tauri-config.test.ts`).

## 5. Choix tranchés en autonomie (règle « option la plus sûre »)
1. **Passphrase optionnelle de KEY-01** : la clé générée est stockée **dans le coffre**, sans passphrase, comme une clé importée (ADR-030) ; aucun export de la clé privée n'est proposé. Une passphrase ne protège qu'un fichier de clé, or la clé ne quitte jamais le cœur ; le coffre (Argon2id) la protège mieux que le chiffrement OpenSSH (bcrypt-pbkdf). Ouvrir un export de clé privée ajouterait un chemin d'exfiltration. **Question pour le porteur** : faut-il un export de la clé privée chiffrée par passphrase (obligatoire) ?
2. **Remplissage automatique du mot de passe en keyboard-interactive** : oui, mais limité (voir §3). Sans lui, un hôte du carnet configuré « mot de passe + TOTP » via PAM redemanderait à chaque fois le mot de passe déjà enregistré (HST-03 : « sans rien redemander »).
3. **Déploiement par l'entrée standard d'une commande `sh` fixe** plutôt que par SFTP (pas encore disponible) : ne vise que les serveurs Unix ; un serveur Windows OpenSSH (shell `cmd`) échoue proprement (`ssh.deploy_failed`). Limite documentée.
4. **Ligne déployée sans commentaire** (`algo base64`), comme la clé publique stockée (ADR-030) : pas de nom d'utilisateur ni de machine envoyé au serveur.
5. **Identification de l'hôte cible du déploiement** : un hôte du carnet (désigné par identifiant, rien ne passe par l'interface). S'il n'a pas d'identité, un mot de passe peut être saisi une fois pour ce déploiement, sinon le keyboard-interactive prend le relais.
6. **Déploiement = session du gestionnaire** : compté dans la limite de sessions, fermé au verrouillage (ADR-026), mêmes dialogues de clé d'hôte et de 2FA.
7. **Pas d'association automatique** de la clé déployée à l'identité de l'hôte : l'utilisateur garde la main (message d'aide après le déploiement).
8. **Algorithmes proposés** : Ed25519 (défaut), ECDSA P-256/P-384/P-521, RSA 4096 ; pas de RSA 2048 ni de DSA.
