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

### 3.3 Format de fichier v1 (`vault.vigie`)
```
"VIGV" (4) | version u16 = 1 | kdf_id u8 = 1 (Argon2id) | m_kib u32 | t u32 | p u32 | sel (16)
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
- Écriture atomique : fichier temporaire dans le même dossier, `fsync`, puis renommage par-dessus l'original (`tempfile::NamedTempFile::persist`). La version précédente devient `vault.vigie.bak`.
- Permissions : 0600 pour les fichiers et 0700 pour le dossier sous Linux. Sous Windows, le profil utilisateur (`%APPDATA%`) suffit.

### 3.7 Export chiffré (étape 9)
Même schéma, avec la magie `"VIGB"`, un **sel distinct** et une **phrase de passe distincte** du mot de passe maître. Un fichier altéré est rejeté (tag AEAD).

## 4. Connexions

### 4.1 SSH
- **Clés d'hôte** : confiance à la première connexion. On affiche l'algorithme et l'empreinte `SHA256:…`, au format OpenSSH, pour pouvoir la comparer avec `ssh-keygen -lf`. Si la clé d'un hôte connu change, la connexion est **refusée**, sans bouton « continuer quand même ». Pour débloquer, il faut supprimer explicitement l'ancienne clé dans la gestion des clés d'hôte, après un avertissement.
- **Algorithmes** : on garde les défauts modernes de russh, avec l'échange de clés hybride post-quantique s'il est disponible, puis curve25519, AES-GCM / ChaCha20-Poly1305, des MAC *-etm, et des signatures RSA uniquement en SHA-2. SHA-1, CBC et les groupes DH faibles sont **désactivés**. Une option par hôte « algorithmes hérités » (pour les vieux équipements réseau) pourra être ajoutée via un ADR, avec un avertissement visible.
- **Pas d'agent forwarding ni de X11 forwarding** (hors périmètre).
- Les délais d'attente et le keepalive évitent les sessions zombies.

### 4.2 Contenu renvoyé par le serveur
- **Terminal** : la lecture du presse-papiers par séquence OSC 52 est désactivée. L'écriture dans le presse-papiers est désactivée par défaut. Les liens (OSC 8 et liens détectés) ne s'ouvrent qu'en `https:`/`http:`, avec un clic explicite.
- **Collage** : le mode « bracketed paste » est respecté, et un collage de plusieurs lignes ou contenant des caractères de contrôle demande une confirmation avec aperçu (protection contre le « pastejacking »). Les caractères de fin de collage sont retirés du texte collé.
- **Déploiement de clé (KEY-03)** : la clé publique est écrite par SFTP ou transmise par l'entrée standard d'une commande fixe. Elle n'est jamais insérée dans une ligne de commande construite par concaténation.
- **SFTP** : les noms de fichiers distants sont nettoyés avant toute écriture locale : refus de `..`, des séparateurs, des chemins absolus, des caractères de contrôle et des noms réservés de Windows (`CON`, `NUL`…). Le chemin final est canonicalisé et doit rester dans le dossier de destination choisi. Les liens symboliques distants ne sont pas suivis lors d'un téléchargement récursif.
- **Messages du serveur** (bannière, questions keyboard-interactive) : affichés comme du texte, jamais interprétés comme du HTML.

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
- CSP stricte, aucun contenu distant, `withGlobalTauri: false`, capabilities minimales par fenêtre, pas de plugin `shell` exposé à l'interface. Détails dans `.claude/rules/tauri-config.md`.
- **Modèle d'isolation Tauri** (iframe sandboxée qui intercepte l'IPC) : à évaluer à l'étape 0 et à consigner dans un ADR. Il apporte une défense en profondeur contre une dépendance frontend compromise (M4).

## 6. Fuites par effet de bord
- **Journaux** : jamais de secret, jamais le contenu du terminal ni des fichiers transférés. L'hôte, le port et l'utilisateur sont tolérés au niveau `debug`. Les types qui contiennent des secrets ont un `Debug` masqué.
- **Erreurs** : codes stables (`ssh.auth_failed`), sans écho de l'entrée utilisateur ni du secret.
- **Processus externes** : aucun secret dans les arguments ni dans les variables d'environnement visibles.
- **Fichiers temporaires** : noms aléatoires, permissions restrictives, suppression garantie, y compris en cas d'erreur (`Drop`).
- **Crash** : pas de rapport de plantage envoyé (aucune télémétrie).
- **Diffusion multi-sessions** : indicateur visuel impossible à manquer et confirmation à l'activation.

## 7. Mises à jour et signature
- **Mises à jour** (étape 13) : `tauri-plugin-updater` avec une signature ed25519 (`tauri signer generate`). La clé publique est dans `tauri.conf.json`. La clé privée et sa passphrase vivent **uniquement** dans les secrets GitHub (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), avec une copie de secours hors ligne chez le porteur. **Jamais dans le dépôt**, et Claude Code n'y a pas accès.
- **Signature du code Windows** : aucune tant que le projet est privé (avertissement SmartScreen accepté). À la publication, demande auprès de la SignPath Foundation (gratuite pour les projets open source sous licence approuvée par l'OSI).

## 8. Chaîne d'approvisionnement
- `Cargo.lock` et `package-lock.json` sont commités. La CI utilise `npm ci`.
- `cargo-deny` : avis RustSec, licences compatibles GPL-3.0, sources limitées à crates.io, doublons signalés.
- `npm audit --audit-level=high` en CI, Dependabot hebdomadaire (cargo, npm, actions).
- Actions GitHub épinglées par SHA, `permissions` minimales, gitleaks à chaque push.
- Chaque nouvelle dépendance est justifiée : utilité, maintenance active, licence, surface d'attaque.

## 9. Signalement de vulnérabilités
À la publication : fichier `SECURITY.md` à la racine, signalement privé activé sur GitHub (Private vulnerability reporting), engagement de délai de réponse.
