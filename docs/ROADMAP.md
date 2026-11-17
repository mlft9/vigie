# Feuille de route

Chaque étape produit une application **testable**. On ne passe à l'étape suivante qu'après les retours de test du porteur.
Les identifiants (SSH-01, VLT-02…) renvoient au `docs/CAHIER_DES_CHARGES.md`.

**Légende** : ⬜ à faire · 🟨 en cours · ✅ terminée (testée et validée par le porteur)

| # | Étape | Statut |
|---|---|---|
| 0 | Fondations | ⬜ |
| 1 | Terminal SSH multi-onglets | ⬜ |
| 2 | Coffre chiffré et carnet d'hôtes | ⬜ |
| 3 | 2FA et générateur de clés | ⬜ |
| 4 | SFTP | ⬜ |
| 5 | Split, snippets, diffusion multi-sessions | ⬜ |
| 6 | Tunnels SSH | ⬜ |
| 7 | Terminal local | ⬜ |
| 8 | RDP v1 (client système) | ⬜ |
| 9 | Import OpenSSH, export/import chiffré | ⬜ |
| 10 | Déverrouillage par le système | ⬜ |
| 11 | Clés FIDO2 | ⬜ |
| 12 | RDP natif (IronRDP) | ⬜ |
| 13 | Finitions et publication | ⬜ |

**Définition de « terminé », valable pour toutes les étapes :**
- `npm run check` passe en local et la CI est verte.
- Les nouvelles fonctions ont des tests (unitaires et, côté SSH, d'intégration).
- Toutes les chaînes de l'interface existent en FR et en EN.
- L'agent `security-reviewer` n'a relevé aucun problème bloquant.
- La documentation est à jour : `ARCHITECTURE.md`, `DECISIONS.md` si une décision a été prise, `CHANGELOG.md`, le statut dans ce fichier.
- Un plan de test manuel a été fourni et le porteur a validé l'étape.

---

## Étape 0 — Fondations
**But** : un squelette solide, durci et outillé, sur lequel tout le reste viendra se greffer.

Contenu :
- Projet Tauri 2 (stable) + React + TypeScript strict + Vite + Tailwind + shadcn/ui + Zustand.
- Workspace Cargo : `src-tauri/` (application, fine) et `crates/` (logique métier). Voir `docs/ARCHITECTURE.md`.
- i18n FR/EN (i18next) : détection de la langue du système, sélecteur, fichiers `src/locales/{fr,en}.json`. APP-02.
- Mise en page vide type Termius : barre latérale, zone d'onglets, thème clair/sombre. APP-01, APP-03.
- Durcissement Tauri : CSP stricte, capabilities minimales, pas d'API globale `window.__TAURI__`.
- Journalisation (`tracing`) vers un fichier rotatif. APP-08 (base).
- Outillage : rustfmt, clippy (`-D warnings`), `cargo-deny` (licences compatibles GPL-3.0, vulnérabilités, sources), ESLint, Prettier, `tsc --noEmit`, Vitest.
- Scripts npm : `check`, `lint`, `test`, `check:rust`, `test:rust`, `audit` (voir `AGENTS.md`).
- CI :
  - `ci.yml` : à chaque push et PR, sous Linux : formatage, lint, types, tests, `cargo-deny`, `npm audit`, gitleaks.
  - `build.yml` : manuel (`workflow_dispatch`) et sur tag `v*` : installeurs Windows (NSIS + MSI) et Linux (deb + AppImage) en artefacts.
  - Dependabot (cargo, npm, actions). Actions épinglées par SHA, permissions minimales.

**Critères d'acceptation**
- [ ] `npm run tauri dev` ouvre la fenêtre sous Windows.
- [ ] La langue bascule FR ↔ EN à chaud.
- [ ] Le thème suit le système et peut être forcé.
- [ ] `npm run check` passe et la CI est verte.
- [ ] Le workflow `build.yml` lancé à la main produit des installeurs Windows et Linux qui s'installent et se lancent.

---

## Étape 1 — Terminal SSH multi-onglets
**But** : se connecter en SSH et travailler confortablement dans plusieurs onglets.

Contenu :
- Crate `core-ssh` : connexion russh, authentification par mot de passe ou par clé (Ed25519/ECDSA/RSA-SHA2, avec passphrase), PTY `xterm-256color`, shell, flux d'entrée/sortie, redimensionnement, fermeture propre, keepalive, délais d'attente. SSH-01, SSH-02, SSH-04, SSH-06, AUTH-01, AUTH-02.
- Vérification des clés d'hôte TOFU avec `known_hosts` propre à l'application. HKY-01, HKY-02, HKY-03.
- Commandes Tauri fines + flux de données via `tauri::ipc::Channel` (pas d'événements globaux pour les données du terminal).
- Interface : formulaire de connexion rapide, onglets (ouvrir, fermer, renommer, réordonner), xterm.js (fit, search, web-links), copier/coller, raccourcis. SSH-03, SSH-05, APP-04.
- Dialogue de clé d'hôte inconnue (type + empreinte SHA-256) et alerte bloquante en cas de changement.
- Tests d'intégration contre un `sshd` en conteneur (CI) ou une VM du lab (en local).

**Critères d'acceptation**
- [ ] Connexion à la VM Debian du lab, par mot de passe puis par clé avec passphrase.
- [ ] vim, htop, tmux et les couleurs s'affichent correctement. Le redimensionnement de la fenêtre est répercuté.
- [ ] Trois onglets ouverts en parallèle sur des hôtes différents.
- [ ] Première connexion : l'empreinte est affichée et doit être acceptée.
- [ ] Après régénération des clés d'hôte de la VM : connexion **bloquée** avec une alerte claire.
- [ ] Couper le réseau de la VM : l'onglet passe à « déconnecté » et propose de se reconnecter, sans planter.
- [ ] `cat` d'un gros fichier : l'interface reste réactive.

---

## Étape 2 — Coffre chiffré et carnet d'hôtes
**But** : enregistrer ses hôtes et identités en sécurité.

Contenu :
- Crate `core-vault` : schéma cryptographique de `docs/SECURITE.md` (Argon2id → KEK, DEK aléatoire, XChaCha20-Poly1305, en-tête authentifié), écriture atomique + `.bak`, format versionné. VLT-01, VLT-02, VLT-04.
- Modèle : hôtes, groupes, identités, clés, clés d'hôte connues. HST-01 à HST-03, HKY-04, KEY-02 (import).
- Interface : création/déverrouillage du coffre, carnet d'hôtes (groupes, recherche, CRUD, double-clic pour se connecter), section Clés, paramètres. APP-05.
- Verrouillage manuel et automatique. VLT-03. Décision à consigner : que deviennent les sessions ouvertes au verrouillage ?

**Critères d'acceptation**
- [ ] Au premier lancement, création du coffre. Aux lancements suivants, il est verrouillé.
- [ ] Un mauvais mot de passe est refusé, sans indice sur la cause.
- [ ] Le fichier du coffre ouvert dans un éditeur ne montre aucune donnée lisible (ni nom d'hôte, ni utilisateur).
- [ ] Le verrouillage automatique fonctionne avec le délai configuré.
- [ ] Le changement de mot de passe maître fonctionne, et l'ancien mot de passe ne marche plus.
- [ ] Une connexion depuis le carnet utilise l'identité enregistrée sans rien redemander.

---

## Étape 3 — 2FA et générateur de clés
Contenu : keyboard-interactive (AUTH-03), générateur de clés (KEY-01), export de la clé publique, déploiement sur un hôte (KEY-03).

**Critères d'acceptation**
- [ ] Connexion à une VM configurée avec mot de passe + TOTP (PAM google-authenticator).
- [ ] Générer une clé Ed25519, la déployer sur la VM, puis se connecter par clé. Un second déploiement ne duplique pas la ligne.
- [ ] Les permissions de `~/.ssh` (700) et de `authorized_keys` (600) sont correctes après le déploiement.

---

## Étape 4 — SFTP
Contenu : `russh-sftp` ; panneau latéral qui suit le shell (OSC 7, intégration au prompt en option) ; onglet double panneau ; opérations, file de transferts, conflits ; édition d'un fichier distant avec l'éditeur local. SFTP-01 à SFTP-05.
Point de sécurité : noms de fichiers distants malveillants (`../`, caractères de contrôle), liens symboliques, écrasements.

**Critères d'acceptation**
- [ ] Un `cd` dans le terminal fait suivre le panneau (avec un shell compatible OSC 7 ou l'intégration activée).
- [ ] Envoi et téléchargement d'un dossier récursif contenant des accents, des espaces et des fichiers vides.
- [ ] Un fichier de 1 Go se transfère avec progression, peut être annulé, et l'interface reste fluide.
- [ ] Un fichier distant nommé de façon malveillante ne peut pas écrire en dehors du dossier local choisi.

---

## Étape 5 — Split, snippets, diffusion multi-sessions
Contenu : split horizontal/vertical (SSH-07), snippets avec variables (SNP-01 à SNP-03), diffusion de la saisie (SSH-08) avec bordure d'alerte et confirmation à l'activation.

**Critères d'acceptation**
- [ ] Quatre terminaux en grille dans un onglet, redimensionnables, avec navigation au clavier.
- [ ] Un snippet avec `{{service}}` demande la valeur avant envoi.
- [ ] La diffusion vers trois sessions envoie les mêmes frappes, le mode est impossible à manquer visuellement, et il se désactive en un clic.

---

## Étape 6 — Tunnels SSH
Contenu : `-L`, `-R`, `-D` (SOCKS5), gestionnaire de tunnels, `127.0.0.1` par défaut avec avertissement pour `0.0.0.0`. TUN-01 à TUN-03.

**Critères d'acceptation**
- [ ] `-L` : un service web qui écoute sur `127.0.0.1:8080` dans la VM est joignable depuis le PC.
- [ ] `-D` : Firefox configuré avec le proxy SOCKS navigue via la VM.
- [ ] `-R` : un service du PC est joignable depuis la VM.
- [ ] L'état et le nombre de connexions sont visibles. L'arrêt libère le port.

---

## Étape 7 — Terminal local
Contenu : crate `core-term` (`portable-pty`, ConPTY sous Windows), profils détectés (PowerShell, pwsh, CMD, WSL ; bash/zsh sous Linux). LOC-01, LOC-02.

**Critères d'acceptation**
- [ ] Les profils installés sont détectés. PowerShell et une distribution WSL s'ouvrent dans un onglet.
- [ ] Redimensionnement et couleurs corrects. Les applications plein écran fonctionnent.

---

## Étape 8 — RDP v1 (client système)
Contenu : hôtes RDP dans le carnet (RDP-01). Sous Windows : fichier `.rdp` temporaire + identifiants via le gestionnaire d'identifiants Windows, supprimés après usage → `mstsc`. Sous Linux : détection de `xfreerdp`/`xfreerdp3`, mot de passe transmis par l'entrée standard. RDP-02.

**Critères d'acceptation**
- [ ] Connexion à la VM Windows du lab depuis Windows et depuis Linux.
- [ ] Le mot de passe n'apparaît **jamais** dans la liste des processus (vérifier avec `ps aux` ou le Gestionnaire des tâches).
- [ ] Les fichiers et identifiants temporaires sont supprimés après le lancement.

---

## Étape 9 — Import OpenSSH, export/import chiffré
Contenu : crate `core-import`, import de `~/.ssh/config` avec rapport (IMP-01), de `known_hosts` (IMP-02), sauvegarde chiffrée avec une phrase de passe distincte (VLT-06).

**Critères d'acceptation**
- [ ] Un `~/.ssh/config` réel (avec `Include` et des jokers) est importé. Les entrées ignorées sont listées avec leur raison.
- [ ] Export, puis import sur une autre machine ou un profil vierge : données identiques.
- [ ] Une sauvegarde modifiée d'un seul octet est rejetée (intégrité).

---

## Étape 10 — Déverrouillage par le système
Contenu : VLT-05. Windows Hello + protection de la DEK par le système (gestionnaire d'identifiants / DPAPI), trousseau Secret Service sous Linux (`keyring`). Opt-in. Le modèle de menace est à documenter dans `SECURITE.md`. Attention : un simple consentement Windows Hello n'est qu'une barrière d'interface, la protection réelle vient du stockage protégé par le système.

**Critères d'acceptation**
- [ ] Une fois l'option activée, le coffre se déverrouille avec Windows Hello (PIN, empreinte, visage).
- [ ] Désactiver l'option efface la DEK du stockage système.
- [ ] Le mot de passe maître reste toujours utilisable en secours.

---

## Étape 11 — Clés FIDO2
Contenu : **étude de faisabilité d'abord** (support `sk-*` dans russh ou via une crate CTAP2, compatibilité Windows), à consigner dans `DECISIONS.md`. Ensuite, implémentation native ou plan B via l'agent OpenSSH. AUTH-04.

**Critères d'acceptation**
- [ ] Génération d'une clé `sk-ssh-ed25519` sur une YubiKey (ou utilisation d'une clé existante).
- [ ] Connexion qui demande le toucher de la clé.

---

## Étape 12 — RDP natif (IronRDP)
Contenu : crate `core-rdp`, rendu dans un onglet (canvas, envoi de régions modifiées), clavier/souris, presse-papiers texte, redimensionnement, NLA/CredSSP, vérification TOFU du certificat. RDP-03, RDP-04.

**Critères d'acceptation**
- [ ] Session Windows utilisable dans un onglet : fluidité acceptable sur le réseau local, clavier AZERTY correct.
- [ ] Copier-coller de texte dans les deux sens.
- [ ] Un changement de certificat du serveur déclenche une alerte.

---

## Étape 13 — Finitions et publication
Contenu : version portable (APP-06), mises à jour signées (APP-07, clés dans les secrets GitHub), README illustré (captures, GIF), `ARCHITECTURE.md` finalisé, `LICENSE` GPL-3.0, `SECURITY.md` (politique de signalement), passage du dépôt en public, demande de signature à la SignPath Foundation, vérification que le nom « Vigie » n'est pas une marque déposée dans le domaine logiciel (INPI, EUIPO) avant la publication.

**Critères d'acceptation**
- [ ] La version portable fonctionne depuis une clé USB, sans rien écrire dans le profil utilisateur.
- [ ] Une mise à jour publiée est détectée, vérifiée (signature) et installée.
- [ ] Le README permet à un inconnu d'installer et de comprendre le projet en 5 minutes.
