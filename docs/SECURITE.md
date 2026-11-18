# Sécurité — principes, modèle de menace et schéma cryptographique

> Document de référence pour toute implémentation qui touche aux secrets, au réseau, à l'IPC, aux fichiers ou à la CI.
> Il n'est modifié qu'avec l'accord du porteur. Claude Code demande avant de l'éditer.

## 1. Ce qu'on protège
| Actif | Pourquoi c'est sensible |
|---|---|
| Mots de passe, clés privées, passphrases, codes OTP | Accès direct aux serveurs |
| Liste des hôtes, utilisateurs, groupes, notes | Cartographie de l'infrastructure, utile à un attaquant |
| Sessions ouvertes et tunnels | Accès en cours, pivot réseau |
| Intégrité des connexions | Une attaque de l'homme du milieu permet de voler des identifiants et de lire ou modifier la session |
| Intégrité de l'application et des mises à jour | Une application piégée a accès à tout |

## 2. Menaces prises en compte
| # | Menace | Parade principale |
|---|---|---|
| M1 | Vol du fichier de coffre (PC volé, sauvegarde qui fuite, malware qui exfiltre le fichier) puis attaque par force brute hors ligne | Chiffrement fort, Argon2id coûteux, mot de passe maître robuste exigé (§3) |
| M2 | Homme du milieu sur le réseau | Vérification des clés d'hôte (TOFU strict), certificats RDP (§4) |
| M3 | Serveur malveillant ou compromis | Validation des noms de fichiers SFTP, filtrage des séquences d'échappement dangereuses, pas d'agent forwarding (§4, §6) |
| M4 | Dépendance npm compromise exécutée dans la WebView | Secrets absents de la WebView, CSP, capabilities minimales, `cargo-deny` / `npm audit` (§5, §8) |
| M5 | Fuite par effet de bord : journaux, liste des processus, messages d'erreur, fichiers temporaires, presse-papiers | Règles du §6 |
| M6 | Mise à jour piégée | Mises à jour signées, clé privée hors du dépôt (§7) |
| M7 | Erreur de manipulation : tunnel exposé, diffusion vers les mauvais serveurs | Valeurs par défaut sûres, avertissements visibles (§6) |

**Hors du modèle** : un malware qui tourne sur la machine avec les droits de l'utilisateur **pendant que le coffre est déverrouillé** (keylogger, lecture de la mémoire). On réduit l'exposition (verrouillage automatique, effacement de la mémoire), mais on ne peut pas s'en protéger complètement. Il faut le dire honnêtement dans la documentation publique.

## 3. Coffre chiffré (étape 2)

### 3.1 Hiérarchie de clés
```
mot de passe maître ──Argon2id(sel)──► KEK (32 octets, jamais stockée)
                                         │
                                         ▼ XChaCha20-Poly1305 (wrap)
DEK (32 octets aléatoires, OsRng) ──────► DEK chiffrée, stockée dans l'en-tête
                                         │
                                         ▼ XChaCha20-Poly1305
données du coffre (sérialisées) ────────► charge utile chiffrée
```
- **Changer le mot de passe maître** : nouveau sel, nouvelle KEK, **nouvelle DEK**, et tout le fichier est rechiffré avec de nouveaux nonces. Deux raisons : l'en-tête (sel compris) sert d'AAD à la charge utile, donc on ne peut pas changer le sel sans la rechiffrer ; et sans rotation de la DEK, une ancienne copie du coffre ouverte avec l'ancien mot de passe donnerait la clé des versions futures. Le déverrouillage par le système (étape 10) doit alors être réactivé.
- **Le déverrouillage par le système** (étape 10) stocke la DEK protégée par le système d'exploitation. Le mot de passe maître reste toujours valable.

### 3.2 Primitives et paramètres
| Élément | Choix |
|---|---|
| Dérivation | **Argon2id**, m = 64 Mio, t = 3, p = 4 (paramètres stockés dans l'en-tête pour pouvoir les augmenter). Refuser d'ouvrir un fichier dont les paramètres sont sous ce plancher **ou au-dessus d'un plafond** (m ≤ 1 Gio, t ≤ 10, p ≤ 16) : un coffre ou une sauvegarde piégés ne doivent pas pouvoir saturer la mémoire. |
| Sel | 16 octets aléatoires, nouveau à chaque changement de mot de passe |
| Chiffrement | **XChaCha20-Poly1305** (crate `chacha20poly1305`), nonce aléatoire de 24 octets, **nouveau à chaque écriture** |
| Données associées (AAD) | L'en-tête complet (magie, version, paramètres KDF, sel) est authentifié, ce qui empêche de modifier les paramètres ou la version sans être détecté |
| Aléa | `rand::rngs::OsRng` uniquement |
| Sérialisation | JSON via `serde`, avec un champ `schema_version` (simple à faire évoluer et à déboguer, la taille n'est pas un enjeu) |

### 3.3 Format de fichier v1 (`vault.younji`)
```
"YNJV" (4) | version u16 = 1 | kdf_id u8 = 1 (Argon2id) | m_kib u32 | t u32 | p u32 | sel (16)
| nonce_dek (24) | DEK chiffrée + tag (48)
| nonce_data (24) | longueur u64 | données chiffrées + tag (16)
```
Entiers en little-endian. La longueur annoncée est vérifiée **avant toute allocation** : elle doit correspondre à la taille réelle du fichier et rester sous 64 Mio. Toute erreur de déchiffrement donne le même message (`vault.wrong_password_or_corrupted`), sans distinguer les deux cas.

### 3.4 Mot de passe maître
- Minimum 12 caractères, et estimation de robustesse avec `zxcvbn` (score ≥ 3 exigé, avec une explication si le mot de passe est refusé).
- Aucune récupération possible si on l'oublie. Il faut le dire clairement à la création, et proposer l'export chiffré (étape 9) comme sauvegarde.
- Délai progressif après plusieurs échecs dans l'interface. C'est surtout du confort : la vraie protection contre M1 reste Argon2id.

### 3.5 Cycle de vie en mémoire
- DEK et secrets déchiffrés dans `SecretBox` / `SecretString`, effacés (`zeroize`) au verrouillage et à la fermeture.
- Argon2 tourne dans `spawn_blocking` pour ne pas figer l'application.
- **Verrouillage** : la DEK et tous les secrets déchiffrés sont supprimés. Le comportement des sessions déjà ouvertes est à décider à l'étape 2 (ADR).
- Limite connue : Rust ne garantit pas l'absence de copies (swap, réallocations). On limite les copies, sans promettre l'impossible.

### 3.6 Écriture
- Écriture atomique : fichier temporaire dans le même dossier, `fsync`, puis renommage par-dessus l'original (`tempfile::NamedTempFile::persist`). La version précédente devient `vault.younji.bak`.
- Permissions : 0600 pour les fichiers et 0700 pour le dossier sous Linux. Sous Windows, le profil utilisateur (`%APPDATA%`) suffit.

### 3.7 Export chiffré (étape 9)
Même schéma, avec la magie `"YNJB"`, un **sel distinct** et une **phrase de passe distincte** du mot de passe maître. Un fichier altéré est rejeté (tag AEAD).

## 4. Connexions

### 4.1 SSH
- **Clés d'hôte** : confiance à la première connexion. On affiche l'algorithme et l'empreinte `SHA256:…`, au format OpenSSH, pour pouvoir la comparer avec `ssh-keygen -lf`. Règle stricte (ADR-023) : si l'hôte est déjà connu et que la clé présentée n'est pas dans la liste enregistrée, **même si elle est d'un autre type**, la connexion est **refusée**, sans bouton « continuer quand même » et avant tout envoi d'identifiant. Cette règle empêche un homme du milieu de proposer un autre algorithme pour retomber sur une simple demande de confiance. Une ligne illisible qui nomme l'hôte compte comme une clé connue : elle bloque au lieu de redemander confiance. Une clé `@revoked` est refusée. La clé acceptée n'est enregistrée qu'après la vérification, par russh, de la signature du serveur sur l'échange de clés : le serveur a prouvé qu'il détient la clé privée. Pour débloquer, il faut supprimer explicitement l'ancienne clé : à la main dans `known_hosts` jusqu'à l'étape 2 (l'alerte indique le fichier et la ligne), puis dans la gestion des clés d'hôte (HKY-04), après un avertissement.
- **Algorithmes** : liste explicite dans `crates/core-ssh/src/config.rs`, sans dépendre des défauts de russh (ADR-019) : échange de clés hybride post-quantique `mlkem768x25519-sha256` en tête, puis curve25519, ECDH NIST et DH groupes 14/16/18 en SHA-2, avec « strict kex » (parade à Terrapin) ; ChaCha20-Poly1305 et AES-GCM, AES-CTR en repli avec MAC *-etm SHA-2 ; signatures RSA uniquement en SHA-2. SHA-1, CBC, 3DES, DSA, DH group exchange, la compression et `none` sont **désactivés** ; un test échoue s'ils réapparaissent. Une option par hôte « algorithmes hérités » (pour les vieux équipements réseau) pourra être ajoutée via un ADR, avec un avertissement visible.
- **Pas d'agent forwarding ni de X11 forwarding** (hors périmètre).
- Les délais d'attente et le keepalive évitent les sessions zombies.

### 4.2 Contenu renvoyé par le serveur
- **Terminal** : la lecture et l'écriture du presse-papiers par séquence OSC 52 sont sans effet (aucun addon presse-papiers chargé). Les liens détectés et les liens OSC 8 ne s'ouvrent qu'en `https:`/`http:`, avec un hôte et sans identifiants intégrés, et seulement au **Ctrl+clic**. Un lien OSC 8, dont le texte affiché peut différer de la cible, passe par une confirmation qui montre l'adresse réelle. Le cœur revalide l'adresse avant de l'ouvrir dans le navigateur du système, en argument de processus et jamais via un shell (ADR-022).
- **Collage** : le mode « bracketed paste » est respecté, et un collage de plusieurs lignes ou contenant des caractères de contrôle demande une confirmation avec un aperçu où chaque caractère de contrôle est rendu visible (protection contre le « pastejacking »). Les marqueurs de début et de fin de collage (`ESC[200~`, `ESC[201~`) sont retirés **de façon répétée, jusqu'à disparition** : un marqueur imbriqué en reformerait un autre et permettrait de sortir du mode collage.
- **Déploiement de clé (KEY-03)** : la clé publique est écrite par SFTP ou transmise par l'entrée standard d'une commande fixe. Elle n'est jamais insérée dans une ligne de commande construite par concaténation.
- **SFTP** : les noms de fichiers distants sont nettoyés avant toute écriture locale : refus de `..`, des séparateurs, des chemins absolus, des caractères de contrôle et des noms réservés de Windows (`CON`, `NUL`…). Le chemin final est canonicalisé et doit rester dans le dossier de destination choisi. Les liens symboliques distants ne sont pas suivis lors d'un téléchargement récursif.
- **Messages du serveur** (bannière, questions keyboard-interactive) : affichés comme du texte, jamais interprétés comme du HTML. Les bannières sont nettoyées de toute séquence de contrôle (échappements ANSI, OSC) avant affichage, plafonnées à 8 Kio chacune et 64 Kio par session, et comptées dans le contrôle de flux : un serveur ne peut ni piloter le terminal ni inonder l'interface avant l'authentification.

### 4.3 RDP
- **v1 (client système)** : le mot de passe n'est **jamais** passé en argument de ligne de commande, car il serait visible dans la liste des processus.
  - Windows : identifiant écrit via l'API `CredWriteW` (cible `TERMSRV/<hôte>`), jamais via `cmdkey /pass:`. Il est supprimé à la fin de la session `mstsc` ou après un court délai. Le fichier `.rdp` temporaire, créé avec un nom aléatoire, est supprimé après usage.
  - Linux : `xfreerdp` reçoit le mot de passe par l'entrée standard (`/from-stdin`).
- **v2 (IronRDP)** : le certificat du serveur est vérifié en TOFU, avec la même logique que les clés d'hôte SSH. NLA (CredSSP) est exigé par défaut.

### 4.4 Tunnels
Écoute sur `127.0.0.1` par défaut. Écouter sur `0.0.0.0` demande une confirmation explicite et affiche un indicateur permanent dans la liste des tunnels.

## 5. Durcissement de l'application Tauri
- La WebView est **non fiable**. Toute entrée IPC est validée côté Rust : types, longueurs, ports (1–65535), chemins canonicalisés, identifiants existants.
- **Les secrets ne transitent qu'une fois**, de l'interface vers le cœur au moment de la saisie. Ensuite, l'interface manipule des identifiants (`identity_id`). Aucune commande ne renvoie un secret.
- CSP stricte, aucun contenu distant, `withGlobalTauri: false`, capabilities minimales par fenêtre, pas de plugin `shell` exposé à l'interface. Seule exception aux styles en ligne : `style-src-elem 'self' 'unsafe-inline'`, pour les balises `<style>` que xterm.js injecte (ADR-018). `style-src 'self'` reste strict en production : les attributs `style` restent interdits. Risque jugé faible, car `img-src`, `font-src` et `connect-src` n'autorisent aucune origine distante, donc une feuille de style injectée ne peut rien exfiltrer. Un test vérifie que `'unsafe-inline'` n'apparaît que dans les directives de style. Détails dans `.claude/rules/tauri-config.md`.
- **`freezePrototype`** : Tauri gèle `Object.prototype` avant tout script, contre la pollution de prototype par une dépendance compromise. Toute dépendance de l'interface doit le supporter : un test charge l'application et un terminal avec le prototype gelé (`src/frozen-prototype.test.tsx`). C'est ce qui a conduit à garder xterm.js 5.5 (ADR-025).
- **Modèle d'isolation Tauri** (iframe sandboxée qui intercepte l'IPC) : **activé** depuis l'étape 0 (ADR-015), avec une liste blanche de commandes (`isolation/index.js`). C'est une défense en profondeur **« au mieux »**, pas une barrière : un test sur build réel (17/11/2026) a montré qu'un script de la fenêtre principale qui intercepte `fetch` récupère la clé d'invocation et appelle une commande directement, sans passer par l'iframe (le nom de commande est dans l'URL, non authentifié). Les vraies barrières restent les **capabilities** et la **validation côté Rust** de chaque entrée. On n'accorde jamais une permission en comptant sur le filtrage de l'isolation, et on ne lui confie aucune validation de charge utile. Seule commande interne de Tauri admise par l'isolation : `plugin:__TAURI_CHANNEL__|fetch`, nécessaire pour récupérer les messages de `Channel` de plus de 1 Kio (sortie du terminal). Tauri l'exempte d'ACL ; elle ne renvoie qu'un message déjà destiné à la fenêtre, une seule fois, désigné par un identifiant aléatoire (ADR-024). Un test vérifie qu'aucune autre commande interne n'est admise.
- **Navigation** : la fenêtre principale ne peut naviguer que vers l'origine de l'application (et le serveur Vite en développement) ; l'ouverture de nouvelles fenêtres est refusée (`src-tauri/src/navigation.rs`). La CSP ne couvre pas la navigation de premier niveau.
- **Rechargement de l'interface** : au début de chaque chargement de page, le cœur ferme toutes les sessions SSH. Les `Channel` de l'ancienne page sont perdus : les sessions n'auraient plus personne pour les piloter, et leurs secrets resteraient en mémoire. Hors du terminal, F5 et Ctrl+R sont bloqués pour éviter une fermeture accidentelle ; dans le terminal, xterm.js les transmet au shell.
- **Sélection de fichiers** : la boîte de dialogue est ouverte par le cœur (`tauri-plugin-dialog` utilisé depuis Rust, aucune permission du plugin accordée à la WebView). Le chemin choisi reste dans le cœur ; l'interface ne reçoit qu'un identifiant opaque et le nom du fichier (ADR-020).
- **Journaux** : `YOUNJI_LOG` ne règle que le niveau du code de Younji ; les dépendances restent plafonnées à `warn`, pour qu'un niveau `trace` ne fasse jamais journaliser des paquets ou des arguments IPC.
- **Commandes applicatives soumises aux capabilities** : chaque commande est déclarée dans `src-tauri/build.rs` et doit être accordée explicitement (`allow-<commande>`) dans une capability. Un test (`tests/security/tauri-config.test.ts`) vérifie la cohérence entre `build.rs`, la liste blanche de l'isolation et les capabilities, ainsi que les invariants de la CSP.

## 6. Fuites par effet de bord
- **Journaux** : jamais de secret, jamais le contenu du terminal ni des fichiers transférés. L'hôte, le port et l'utilisateur sont tolérés au niveau `debug`. Les types qui contiennent des secrets ont un `Debug` masqué.
- **Erreurs** : codes stables (`ssh.auth_failed`), sans écho de l'entrée utilisateur ni du secret.
- **Processus externes** : aucun secret dans les arguments ni dans les variables d'environnement visibles.
- **Fichiers temporaires** : noms aléatoires, permissions restrictives, suppression garantie, y compris en cas d'erreur (`Drop`).
- **Crash** : pas de rapport de plantage envoyé (aucune télémétrie).
- **Diffusion multi-sessions** : indicateur visuel impossible à manquer et confirmation à l'activation.
- **Limites mémoire connues (étape 1)** : russh reçoit le mot de passe sous forme de `String` ordinaire pendant l'authentification, et cette copie n'est pas effacée par Younji. Le corps JSON d'un appel IPC qui transporte un secret (`ssh_connect`) passe par les tampons de la WebView et de Tauri, qu'on ne peut pas effacer. Le cœur place le secret dans un `SecretString` dès la désérialisation, détruit les identifiants dès la fin de l'authentification, et l'interface vide le champ dès l'envoi. Ces copies résiduelles relèvent de la limite déjà décrite au §3.5 et du hors-modèle du §2 (lecture de la mémoire par un malware local).

## 7. Mises à jour et signature
- **Mises à jour** (étape 13) : `tauri-plugin-updater` avec une signature ed25519 (`tauri signer generate`). La clé publique est dans `tauri.conf.json`. La clé privée et sa passphrase vivent **uniquement** dans les secrets GitHub (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), avec une copie de secours hors ligne chez le porteur. **Jamais dans le dépôt**, et Claude Code n'y a pas accès.
- **Signature du code Windows** : aucune pour l'instant (avertissement SmartScreen accepté). Le dépôt étant public depuis l'étape 0 (ADR-016), la demande auprès de la SignPath Foundation (gratuite pour les projets open source sous licence approuvée par l'OSI) peut être faite dès qu'une première version est publiée.

## 8. Chaîne d'approvisionnement
- `Cargo.lock` et `package-lock.json` sont commités. La CI utilise `npm ci`.
- `cargo-deny` : avis RustSec, licences compatibles GPL-3.0, sources limitées à crates.io, doublons signalés.
- `npm audit --audit-level=high` en CI, Dependabot hebdomadaire (cargo, npm, actions).
  - **Exception en cours (17/11/2026, décision du porteur)** : l'avis [GHSA-fc5q-39xc-vjpf](https://github.com/advisories/GHSA-fc5q-39xc-vjpf) (ReDoS dans `picomatch` ≤ 4.0.7) n'a aucune version corrigée. `picomatch` n'est utilisé que par l'outillage de développement (Vite, Vitest, ESLint), avec nos propres motifs de fichiers, et n'est pas embarqué dans l'application. En attendant un correctif : l'audit des dépendances **embarquées** (`npm audit --omit=dev --audit-level=high`) reste bloquant, et l'audit complet est affiché en CI sans bloquer. **À retirer dès qu'une version corrigée de `picomatch` est publiée** : rétablir l'audit complet bloquant dans `ci.yml` et dans `npm run audit`.
- Actions GitHub épinglées par SHA, `permissions` minimales, gitleaks à chaque push.
- Chaque nouvelle dépendance est justifiée : utilité, maintenance active, licence, surface d'attaque.

## 9. Signalement de vulnérabilités
En place depuis l'étape 0 (ADR-016) : fichier [`SECURITY.md`](../SECURITY.md) à la racine (FR/EN), signalement via le signalement privé de GitHub (*Private vulnerability reporting*, à activer dans les paramètres du dépôt), accusé de réception sous 7 jours et première évaluation sous 14 jours.
