# Journal des décisions d'architecture (ADR)

Chaque décision structurante est consignée ici. On ne réécrit jamais un ADR : si une décision change, on en ajoute un nouveau qui remplace l'ancien.
Format : statut, contexte, décision, alternatives écartées, conséquences.

| ADR | Sujet | Statut |
|---|---|---|
| 001 | Tauri 2 comme socle applicatif | Acceptée |
| 002 | russh pour SSH, SFTP et tunnels | Acceptée |
| 003 | Stack frontend | Acceptée |
| 004 | Logique métier dans des crates indépendantes de Tauri | Acceptée |
| 005 | Schéma cryptographique du coffre | Acceptée |
| 006 | Clés d'hôte : TOFU strict | Acceptée |
| 007 | RDP en deux temps | Acceptée |
| 008 | Dépôt GitHub privé et stratégie CI | Acceptée |
| 009 | Licence GPL-3.0 | Acceptée |
| 010 | Nom de code « Relais » et centralisation du nom | Remplacée par ADR-014 |
| 011 | Pas de synchronisation, export/import chiffré uniquement | Acceptée |
| 012 | Déverrouillage : mot de passe maître + système en option | Acceptée |
| 013 | Développement assisté par Claude Code | Acceptée |
| 014 | Nom définitif : Vigie | Acceptée |
| 015 | Modèle d'isolation Tauri et commandes soumises aux capabilities | Acceptée |

---

## ADR-001 — Tauri 2 comme socle applicatif
**Statut** : Acceptée (17/11/2026)
**Contexte** : il faut une vraie application de bureau Windows + Linux, avec un backend capable de gérer finement SSH, la crypto et la mémoire, et une interface moderne.
**Décision** : Tauri 2 (version stable) : cœur en Rust, interface web dans la WebView du système.
**Alternatives écartées** :
- Electron : très répandu (Tabby), mais lourd (Chromium embarqué), backend Node moins adapté à la gestion des secrets en mémoire.
- Qt / Slint (100 % natif) : interface plus coûteuse à rendre moderne, écosystème terminal moins riche que xterm.js.
- Tauri 3 (alpha) : instable, documentation incomplète.
**Conséquences** : binaires légers ; sécurité renforcée par le modèle capabilities/CSP ; dépendance à WebView2 (Windows) et WebKitGTK (Linux), qui rendent un peu différemment, d'où des tests sur les deux.

## ADR-002 — russh pour SSH, SFTP et tunnels
**Statut** : Acceptée (17/11/2026)
**Contexte** : il faut une bibliothèque SSH client asynchrone, multiplateforme, qui gère PTY, SFTP, redirection de ports et keyboard-interactive.
**Décision** : `russh` (+ `russh-sftp`), en pur Rust et asynchrone (tokio).
**Alternatives écartées** :
- `ssh2` (bindings libssh2) : synchrone, dépend d'OpenSSL/libssh2 en C, compilation Windows plus pénible, algorithmes modernes en retard.
- Lancer le binaire `ssh` du système : peu de contrôle (clés d'hôte, coffre), dépend de l'installation locale.
**Conséquences** : pas de dépendance C ; API qui évolue souvent (toujours vérifier la version) ; le support FIDO2 (`sk-*`) est à étudier (étape 11, plan B : agent OpenSSH).

## ADR-003 — Stack frontend
**Statut** : Acceptée (17/11/2026)
**Contexte** : le porteur connaît déjà React + TypeScript + Tailwind (projet `gestionnaire-ssh`), et l'interface doit être moderne (style Termius) et bilingue.
**Décision** : React + TypeScript strict + Vite + Tailwind CSS + shadcn/ui, zustand pour l'état, i18next pour les traductions, xterm.js pour le terminal.
**Alternatives écartées** : Svelte (plus léger mais moins répandu), Vue.
**Conséquences** : écosystème riche ; attention à la surface de dépendances npm (voir SECURITE.md, M4).

## ADR-004 — Logique métier dans des crates indépendantes de Tauri
**Statut** : Acceptée (17/11/2026)
**Contexte** : il faut pouvoir tester SSH, le coffre et les imports sans lancer d'interface ni installer WebKitGTK en CI.
**Décision** : workspace Cargo avec `crates/core-*` sans dépendance à Tauri ; `src-tauri` ne fait que relier les commandes IPC aux crates.
**Conséquences** : tests rapides et ciblés ; frontière claire ; il faut un peu de code de conversion (erreurs, types) dans `src-tauri`.

## ADR-005 — Schéma cryptographique du coffre
**Statut** : Acceptée (17/11/2026)
**Contexte** : protéger identifiants et cartographie contre le vol du fichier (attaque hors ligne).
**Décision** : Argon2id (64 Mio, t=3, p=4) → KEK ; DEK aléatoire chiffrée par la KEK ; données chiffrées par la DEK en XChaCha20-Poly1305 ; en-tête authentifié en AAD. Détails : `SECURITE.md` §3.
**Alternatives écartées** :
- AES-256-GCM : sûr, mais ses nonces de 96 bits rendent les nonces aléatoires plus risqués à long terme ; XChaCha20 (192 bits) supprime ce souci.
- SQLCipher : dépendance C, et chiffrement au niveau de la base plutôt que des données.
- Trousseau du système seul : pas de portabilité, pas d'export.
**Conséquences** : changer le mot de passe maître renouvelle la DEK et rechiffre tout le coffre (voir `SECURITE.md` §3.1) ; la DEK permet le déverrouillage par le système sans y stocker le mot de passe ; les paramètres Argon2 peuvent évoluer, car ils sont stockés dans l'en-tête.

## ADR-006 — Clés d'hôte : TOFU strict
**Statut** : Acceptée (17/11/2026)
**Décision** : acceptation explicite à la première connexion (empreinte SHA-256) ; si la clé change, la connexion est refusée, sans contournement. Fichier `known_hosts` propre à l'application, au format OpenSSH.
**Alternatives écartées** : mode strict seul (trop contraignant au quotidien) ; configuration par hôte (complexité jugée inutile pour l'instant).
**Conséquences** : comportement familier pour un utilisateur d'OpenSSH ; l'emplacement final de `known_hosts` (fichier ou coffre) sera tranché à l'étape 2.

## ADR-007 — RDP en deux temps
**Statut** : Acceptée (17/11/2026)
**Décision** : étape 8, lancement du client du système (mstsc / xfreerdp) sans exposer le mot de passe ; étape 12, RDP natif dans un onglet avec IronRDP.
**Conséquences** : RDP utilisable tôt ; le plus gros risque technique est repoussé en fin de parcours, sans bloquer le reste.

## ADR-008 — Dépôt GitHub privé et stratégie CI
**Statut** : Acceptée (17/11/2026)
**Contexte** : le dépôt reste privé au début. Les minutes GitHub Actions sont limitées et les runners Windows comptent double.
**Décision** : `ci.yml` sous Linux à chaque push ; `build.yml` (Windows + Linux) en manuel ou sur tag. Passage en public à l'étape 13. Gitea abandonné, car le Gitea du porteur n'est pas joignable depuis les environnements cloud.
**Conséquences** : coûts maîtrisés ; les installeurs se demandent explicitement quand il y a une version à tester.

## ADR-009 — Licence GPL-3.0
**Statut** : Acceptée (17/11/2026)
**Décision** : GPL-3.0 à la publication. `cargo-deny` vérifie que les dépendances sont compatibles (MIT, Apache-2.0, BSD, ISC, MPL-2.0…).
**Conséquences** : les forks restent open source ; licence approuvée par l'OSI, donc éligible à la signature gratuite de la SignPath Foundation.

## ADR-010 — Nom de code « Relais » et centralisation du nom
**Statut** : Remplacée par ADR-014 (17/11/2026)
**Décision** : le nom provisoire « Relais » (identifiant de bundle `io.github.mlft9.relais`) est défini en un seul endroit par couche : `productName` / `identifier` dans `tauri.conf.json`, clé i18n `app.name`, constante Rust `APP_NAME`. Les crates portent des noms neutres (`core-*`).
**Conséquences** : le changement de nom à l'étape 13 se limite à quelques fichiers, plus le nom du fichier de coffre. **Attention** : changer l'`identifier` déplace le dossier de données, donc il faudra prévoir une migration.

## ADR-011 — Pas de synchronisation, export/import chiffré uniquement
**Statut** : Acceptée (17/11/2026)
**Décision** : aucune synchronisation automatique. Un export/import chiffré avec une phrase de passe distincte (étape 9).
**Conséquences** : aucune infrastructure à maintenir, aucune surface d'attaque réseau supplémentaire.

## ADR-012 — Déverrouillage : mot de passe maître + système en option
**Statut** : Acceptée (17/11/2026)
**Décision** : mot de passe maître obligatoire à la création et toujours valable. En option (étape 10), déverrouillage rapide par Windows Hello ou le trousseau Linux. Verrouillage automatique après inactivité.
**Point ouvert (étape 10)** : la façon de lier Windows Hello à la DEK. Un simple consentement (`UserConsentVerifier`) n'est qu'une barrière d'interface. Une clé `KeyCredentialManager` dont la signature sert à dériver une clé de chiffrement offre une vraie liaison cryptographique. À trancher dans un nouvel ADR.

## ADR-013 — Développement assisté par Claude Code
**Statut** : Acceptée (17/11/2026)
**Décision** : le code est écrit par Claude Code, étape par étape, selon `CLAUDE.md` / `AGENTS.md`, avec trois agents spécialisés (sécurité, QA, documentation) et des commandes de processus (`/etape`, `/retour`, `/release`, `/revue-securite`). Le porteur valide chaque plan, teste chaque étape et prend les décisions.
**Conséquences** : rythme soutenu ; la revue de sécurité systématique et les tests compensent le risque d'erreurs plausibles mais fausses ; le processus est documenté et assumé dans le README.

## ADR-014 — Nom définitif : Vigie
**Statut** : Acceptée (17/11/2026) — remplace ADR-010
**Contexte** : le nom de code « Relais » devait être remplacé avant la publication. Le choisir avant d'écrire la moindre ligne de code évite toute migration du dossier de données.
**Décision** : le projet s'appelle **Vigie** (le guetteur en haut du mât, qui surveille l'horizon), en cohérence avec l'orientation sécurité. Dépôt `mlft9/vigie`, identifiant de bundle `io.github.mlft9.vigie`. La règle de centralisation d'ADR-010 est conservée : `productName` / `identifier` dans `tauri.conf.json`, clé i18n `app.name`, constante Rust `APP_NAME`. Fichiers de données : `vault.vigie`, sauvegardes `.vigie-backup`. Variables de test : `VIGIE_TEST_*`.
**Alternatives écartées** : Poterne, Amarre, Escale, Nacre (libres mais moins parlants) ; Conque/Conch, Pagure, Carapace, Bastion (déjà utilisés par des logiciels proches).
**Vérification faite** : aucun client SSH ni outil d'accès distant connu ne porte ce nom (recherche web du 17/11/2026).
**Reste à faire (étape 13)** : vérifier les marques déposées dans le domaine logiciel (INPI, EUIPO) avant la publication.
**Conséquences** : aucune migration à prévoir. Le mot est le même en français et proche en anglais (*vigil*), facile à prononcer.

## ADR-015 — Modèle d'isolation Tauri et commandes soumises aux capabilities
**Statut** : Acceptée (17/11/2026), décision du porteur
**Contexte** : `SECURITE.md` §5 demandait d'évaluer le modèle d'isolation de Tauri à l'étape 0. La menace visée est M4 : une dépendance npm compromise qui s'exécute dans la WebView et tente d'appeler le cœur Rust.
**Décision** :
- **Isolation activée** : `app.security.pattern` en mode `isolation`, application d'isolation dans `isolation/`. Le script est classique (pas de module ES), car les modules ne se chargent pas dans l'iframe sandboxée sous WebView2. Il applique une **liste blanche de commandes**. Une commande refusée est remplacée par `vigie_isolation_denied` (commande inexistante), sans sa charge utile : l'appel échoue avec une erreur. Lever une exception dans le hook laisserait la promesse de l'appel en attente indéfiniment.
- **Commandes applicatives soumises aux capabilities** : chaque commande est déclarée dans `src-tauri/build.rs` (`AppManifest`). Tauri génère une permission `allow-<commande>`, qu'une capability doit accorder explicitement (`allow-app-info` dans `capabilities/main.json`).
- **Garde-fou automatique** : `tests/security/tauri-config.test.ts` impose la cohérence entre `build.rs`, la liste blanche de l'isolation et les capabilities, et compare les permissions accordées à une liste exacte.

**Limite constatée** (test sur un build réel piloté par CDP, 17/11/2026) : un script de la fenêtre principale qui intercepte `fetch` récupère l'en-tête `Tauri-Invoke-Key`, puis appelle `http://ipc.localhost/<commande>` directement avec un corps vide, sans passer par l'iframe. L'appel `plugin:app|version` n'a été arrêté que par l'ACL (capabilities). L'isolation est donc une défense en profondeur « au mieux », pas une barrière : les vraies barrières sont les **capabilities** et la **validation côté Rust**.
**Règles qui en découlent** : ne jamais accorder une permission en comptant sur le filtrage de l'isolation ; ne jamais confier à l'isolation la validation d'une charge utile.
**Alternatives écartées** :
- Pas d'isolation (CSP + capabilities seules) : écarté, l'isolation reste une couche de plus à coût faible.
- Ajouter à la CSP le hash de la balise `<style>` injectée par Tauri : écarté, on préfère ne pas élargir la CSP pour un besoin purement visuel.

**Conséquences** :
- La balise `<style>` que Tauri injecte pour masquer l'iframe d'isolation est bloquée par notre CSP (`style-src 'self'`). L'iframe est masquée depuis `src/index.css`, sans assouplir la CSP. Deux avertissements CSP apparaissent dans la console : ils sont attendus.
- Chaque message IPC est chiffré (AES-GCM) entre l'iframe et le cœur : coût négligeable.
- Toute nouvelle commande doit être ajoutée à trois endroits : `src-tauri/build.rs`, `isolation/index.js`, une capability (et la liste attendue du test). Le test échoue si l'un d'eux est oublié.
