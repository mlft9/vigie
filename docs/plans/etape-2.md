# Plan — Étape 2 : coffre chiffré et carnet d'hôtes

> Rédigé en mode autonome (18/11/2026). Branche `etape/2-coffre-carnet`, partie de `main` (étape 1 fusionnée, PR #6).
> Les deux commits de processus `chore(claude)` restés sur `etape/1-terminal-ssh` sont reportés en tête de branche.

Exigences : VLT-01 à VLT-04, HST-01 à HST-03, HKY-04, KEY-02 (import + clé publique), APP-05, APP-04 (raccourci « verrouiller »).
Références : `SECURITE.md` §3 (schéma, format v1, cycle de vie), ADR-005, ADR-006, ADR-012, ADR-021, ADR-023.

## 1. Sous-tâches (dans l'ordre)

| # | Sous-tâche | Fichiers / crates | Commit(s) |
|---|---|---|---|
| 1 | Crate `core-vault` : en-tête v1, Argon2id (plancher/plafond), enveloppe de la DEK, chiffrement XChaCha20-Poly1305 avec l'en-tête en AAD, contrôle de longueur avant allocation | `crates/core-vault/src/{format,crypto}.rs` | `feat(vault): …` |
| 2 | Fichier du coffre : création (`create_new`), déverrouillage, sauvegarde atomique + `.bak`, changement de mot de passe (nouveau sel, nouvelle DEK), permissions 0600/0700 | `crates/core-vault/src/store.rs` | `feat(vault): …` |
| 3 | Modèle : hôtes, groupes, identités, clés, clés d'hôte connues, réglages du coffre ; validation des champs et intégrité référentielle ; politique du mot de passe maître (12 caractères + zxcvbn ≥ 3) | `crates/core-vault/src/{model,policy}.rs` | `feat(vault): …` |
| 4 | `core-ssh` : stockage de `known_hosts` abstrait (fichier ou coffre), liste et suppression des clés d'hôte (HKY-04), identité « clé puis mot de passe », import d'une clé (ré-encodée sans passphrase) | `crates/core-ssh/src/{known_hosts,auth,session}.rs` | `feat(ssh): …` |
| 5 | `src-tauri` : `VaultState` (verrouillé/déverrouillé, délai progressif, verrouillage automatique côté cœur), commandes `vault_*`, `host_*`, `group_*`, `identity_*`, `key_*`, `known_host_*`, `ssh_connect_host`, `settings_*` ; capabilities + liste blanche de l'isolation | `src-tauri/src/{vault_state,settings}.rs`, `commands/{vault,book,settings}.rs`, `build.rs`, `capabilities/main.json`, `isolation/index.js` | `feat(ipc): …`, `feat(security): …` |
| 6 | Interface : écran de création (indicateur de robustesse) et de déverrouillage, verrouillage manuel (bouton + Ctrl+Maj+L) et automatique | `src/features/vault/`, `src/stores/vault.ts`, `src/lib/ipc.ts` | `feat(ui): …` |
| 7 | Interface : carnet d'hôtes (groupes, recherche, CRUD, double-clic pour se connecter), section Clés (clés importées, identités, clés d'hôte connues) | `src/features/hosts/`, `src/features/keys/` | `feat(ui): …` |
| 8 | Paramètres persistants (`settings.json` géré par le cœur, migration depuis `localStorage`), taille de police, délai de verrouillage, changement du mot de passe maître | `src/features/settings/`, `src/stores/settings.ts` | `feat(ui): …` |
| 9 | Tests, `npm run check`, revue de sécurité, QA, documentation | — | `test(…)`, `fix(security): …`, `docs: …` |

## 2. Dépendances à ajouter

| Dépendance | Rôle | Licence | Version vérifiée | Remarque |
|---|---|---|---|---|
| `argon2` (RustCrypto) | Dérivation Argon2id | MIT OR Apache-2.0 | 0.6.0 (déjà dans l'arbre via `ssh-key`) | sans fonctionnalités par défaut, `alloc` + `zeroize` |
| `chacha20poly1305` (RustCrypto) | XChaCha20-Poly1305 | Apache-2.0 OR MIT | 0.11.0 | `alloc`, `zeroize` ; pas de `getrandom` intégré (aléa fourni par nous) |
| `getrandom` | Aléa du système (équivalent `OsRng` ; `rand` 0.10 l'expose sous le nom `SysRng`) | MIT OR Apache-2.0 | 0.4.3 (déjà dans l'arbre) | appel direct à `getrandom::fill` |
| `zxcvbn` | Robustesse du mot de passe maître, contrôle faisant foi côté cœur | MIT | 3.1.1 (avril 2026, 14 M de téléchargements) | sans `builder` (`derive_builder`) |
| `uuid` | Identifiants opaques des entrées du coffre | Apache-2.0 OR MIT | 1.29.3 (déjà dans l'arbre via Tauri) | `v4` |
| `serde_json`, `secrecy`, `zeroize`, `tempfile`, `thiserror`, `tracing` | déjà présentes | — | — | — |
| `@zxcvbn-ts/core` + `@zxcvbn-ts/language-common` + `@zxcvbn-ts/language-en` (npm) | Indicateur de robustesse **en direct** dans l'interface | MIT | 4.2.0 / 4.1.3 / 4.1.1 | versions de plus de 7 jours (la 4.3.0 date du 14/11 : délai `cooldown` respecté) ; à vérifier avec `freezePrototype` |

## 3. Risques de sécurité et parades

| Risque | Parade |
|---|---|
| Force brute hors ligne (M1) | Argon2id m=64 Mio, t=3, p=4 ; mot de passe ≥ 12 caractères et zxcvbn ≥ 3 vérifiés **dans le cœur** |
| Coffre piégé (paramètres KDF énormes, longueur annoncée géante) | Plancher et plafond des paramètres ; longueur vérifiée contre la taille réelle et ≤ 64 Mio **avant** allocation ; lecture bornée |
| Altération du fichier (version, sel, paramètres) | En-tête complet en AAD des deux chiffrements ; un octet modifié = refus |
| Oracle « mauvais mot de passe / fichier abîmé » | Code unique `vault.wrong_password_or_corrupted` pour toute erreur de déchiffrement |
| Réutilisation de nonce | Nonce aléatoire de 24 octets à chaque écriture ; nouvelle DEK + nouveau sel au changement de mot de passe |
| Secrets en mémoire | DEK dans `SecretBox`, mots de passe et clés privées dans `SecretString`, tampons JSON dans `Zeroizing` ; tout est détruit au verrouillage |
| Fuite vers la WebView | DTO sans secret (`hasPassword: bool`, clé publique et empreinte seulement) ; un test vérifie qu'aucun DTO ne sérialise de secret |
| Force brute en ligne via l'interface | Tentatives sérialisées (mutex) et délai progressif côté cœur après 3 échecs (1 s, 2 s, 4 s… 30 s max) |
| Verrouillage contourné par une interface compromise | Minuteur côté cœur : l'interface ne peut que **signaler** une activité (`vault_touch`) ; elle ne peut pas empêcher le verrouillage au-delà du délai sans activité réelle simulée (hors modèle) |
| Écriture interrompue | Fichier temporaire + `fsync` + renommage ; `.bak` écrit lui aussi de façon atomique avant le remplacement |
| Chemin de clé fourni par la WebView | Import par identifiant opaque de `ssh_pick_key_file` (ADR-020) |
| Saisie IPC non fiable | `deny_unknown_fields`, identifiants UUID validés, longueurs et nombres d'entrées bornés, ports 1–65535 |

## 4. Tests prévus
- **core-vault** : aller-retour création/déverrouillage ; mauvais mot de passe ; un octet modifié à chaque position de l'en-tête et de la charge utile → refus ; paramètres sous le plancher / au-dessus du plafond → refus ; longueur annoncée incohérente → refus sans allocation ; nonces différents à chaque sauvegarde ; changement de mot de passe (ancien refusé, sel et DEK changés) ; `.bak` présent ; aucun nom d'hôte lisible dans le fichier ; politique de mot de passe ; validation du modèle et intégrité référentielle ; `Debug` sans secret.
- **core-ssh** : stockage `known_hosts` en mémoire, liste et suppression ; identité clé + mot de passe contre le serveur russh local ; ré-encodage d'une clé importée.
- **src-tauri** : désérialisation stricte des requêtes, DTO sans secret, délai progressif, verrouillage automatique (horloge simulée), traductions de tous les codes.
- **Interface (Vitest)** : écran de création (refus d'un mot de passe faible, confirmation différente), déverrouillage (erreur générique), verrouillage (onglets fermés), carnet (recherche, groupes, formulaire, double-clic), clés et identités, paramètres ; garde-fous de configuration (`tauri-config.test.ts`) mis à jour ; test `freezePrototype` étendu à zxcvbn-ts.

## 5. Choix tranchés en autonomie (règle « option la plus sûre »)
1. **Sessions ouvertes au verrouillage** (question posée par la feuille de route) : **toutes les sessions SSH sont fermées et les onglets de terminal détruits** (tampon de défilement compris). Un poste laissé sans surveillance ne doit pas exposer un shell ouvert ni l'historique affiché. ADR à créer. Réversible : une option « garder les sessions » pourrait être ajoutée plus tard.
2. **`known_hosts` déplacé dans le coffre** (point ouvert d'ADR-006/023) : lignes au format OpenSSH stockées chiffrées et authentifiées. Gain : confidentialité de la cartographie (M1) et **intégrité** (un fichier en clair pouvait être modifié pour y glisser la clé d'un attaquant). Coût nul, puisque l'application exige le déverrouillage. Le fichier en clair existant est importé à la création du coffre, puis supprimé. ADR à créer (remplace la partie « fichier en clair » d'ADR-023).
3. **Toute l'application derrière le déverrouillage** : connexion rapide comprise (elle a besoin des clés d'hôte connues).
4. **Délai de verrouillage automatique stocké dans le coffre** (authentifié) plutôt que dans `settings.json` (modifiable en clair) ; valeurs 1, 5, 10, 15, 30, 60 min, défaut 15 min, **pas d'option « jamais »**.
5. **Clé importée stockée sans sa passphrase d'origine** : déchiffrée à l'import puis ré-encodée au format OpenSSH dans le coffre (chiffré). Stocker la passphrase à côté n'apporterait aucune protection supplémentaire et la connexion doit se faire sans rien redemander.
6. **Suppression d'une clé ou d'une identité utilisée** : refusée (`vault.key_in_use`, `vault.identity_in_use`) plutôt que de casser silencieusement des hôtes. Supprimer un groupe laisse ses hôtes « sans groupe ».
7. **Indicateur de robustesse** : calculé en direct dans l'interface (zxcvbn-ts) pour ne pas envoyer le mot de passe au cœur à chaque frappe (règle « un secret ne transite qu'une fois ») ; le cœur refait le contrôle avec zxcvbn (Rust) et fait foi.
8. **Rechargement de l'interface = verrouillage** (en plus de la fermeture des sessions déjà en place).
9. **Événement de verrouillage** par un `Channel` (`vault_watch`) plutôt que par les événements globaux de Tauri, pour ne pas accorder `core:event:*` à la fenêtre.
10. **Groupes à un seul niveau** (dossiers simples) : suffisant pour HST-02, plus simple à valider.
11. **Paramètres d'apparence** migrés de `localStorage` vers `settings.json` (géré par le cœur, validé, écriture atomique), avec reprise automatique des anciennes valeurs.
