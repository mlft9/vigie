# Environnement de test (lab Proxmox)

Ce lab sert aux tests manuels de chaque étape et aux tests d'intégration lancés en local.
En CI, un conteneur `sshd` remplace la VM Linux (créé à l'étape 1 dans `tests/docker/`).

## 1. Règles du lab
- **Réseau isolé** : un pont ou un VLAN dédié (ex. `vmbr1`) en NAT, **jamais exposé à Internet**. Les comptes de test ont volontairement des configurations variées, parfois faibles.
- **Identifiants uniquement pour le lab**, jamais réutilisés ailleurs.
- **Snapshot Proxmox** de chaque VM une fois préparée (`base-propre`). Certains tests sont destructifs, comme la régénération des clés d'hôte.
- Les adresses et identifiants de test vont dans `.env.test` (ignoré par git, et que Claude Code n'a pas le droit de lire). Les adresses seules (sans mot de passe) peuvent aller dans `CLAUDE.local.md`.

## 2. VM à préparer

| VM | Système | Sert à | Dès l'étape |
|---|---|---|---|
| `vigie-deb` | Debian 12 ou 13, sans interface graphique | Serveur SSH/SFTP/tunnels principal | 1 |
| `vigie-win` | Windows Server (évaluation 180 j) ou Windows 11 Entreprise (évaluation 90 j) | Cible RDP, serveur OpenSSH Windows, test d'installation sur un Windows vierge | 0 (installeur), 8 (RDP) |
| `vigie-desktop` | Ubuntu Desktop 24.04 | **Tester la version Linux de Vigie** (.deb, AppImage) et xfreerdp | 0 |

> Ton PC est sous Windows : sans `vigie-desktop`, la version Linux de l'application ne serait jamais testée en conditions réelles.
> Pour l'étape 11, prévois une **YubiKey** (ou une autre clé FIDO2), branchée sur ton PC, côté client.

## 3. Préparer `vigie-deb`

```bash
sudo apt update && sudo apt install -y openssh-server libpam-google-authenticator python3

# Comptes de test
sudo useradd -m -s /bin/bash testpwd   && sudo passwd testpwd     # mot de passe
sudo useradd -m -s /bin/bash testkey                             # clé uniquement
sudo useradd -m -s /bin/bash test2fa   && sudo passwd test2fa     # mot de passe + TOTP
sudo groupadd sftponly
sudo useradd -m -g sftponly -s /usr/sbin/nologin testsftp && sudo passwd testsftp
```

Ajoute à la fin de `/etc/ssh/sshd_config` :
```
KbdInteractiveAuthentication yes
UsePAM yes

Match User testkey
    AuthenticationMethods publickey

Match User test2fa
    AuthenticationMethods keyboard-interactive

Match Group sftponly
    ChrootDirectory /srv/sftp/%u
    ForceCommand internal-sftp
    AllowTcpForwarding no
```

```bash
# Clé de test pour testkey : génère une paire DÉDIÉE au lab sur ton PC,
# HORS du dossier du projet (sinon elle pourrait être commitée ou lue par Claude Code) :
#   mkdir ~/.vigie-lab ; ssh-keygen -t ed25519 -f ~/.vigie-lab/vigie_test_ed25519 -C vigie-lab
# puis copie la partie .pub sur la VM :
sudo install -d -m 700 -o testkey -g testkey /home/testkey/.ssh
sudo install -m 600 -o testkey -g testkey vigie_test_ed25519.pub /home/testkey/.ssh/authorized_keys

# 2FA pour test2fa : se connecter en tant que test2fa, puis lancer :
google-authenticator -t -d -f -r 3 -R 30 -w 3   # scanner le QR code dans une appli TOTP
# Dans /etc/pam.d/sshd, ajouter :  auth required pam_google_authenticator.so nullok
# (nullok : les comptes sans TOTP configuré, dont le tien, continuent de se connecter normalement)

# Chroot SFTP
sudo mkdir -p /srv/sftp/testsftp/depot
sudo chown root:root /srv/sftp/testsftp && sudo chmod 755 /srv/sftp/testsftp
sudo chown testsftp:sftponly /srv/sftp/testsftp/depot

sudo systemctl restart ssh
```

Données pour les tests SFTP (dans le dossier de `testpwd`) :
```bash
mkdir -p ~/sftp-tests/{vide,"dossier avec espaces",accentué-éàü}
fallocate -l 1G ~/sftp-tests/gros-1G.bin
touch ~/sftp-tests/{"fichier avec espaces.txt","éàü.txt",".cache-caché"}
ln -s /etc/passwd ~/sftp-tests/lien-vers-passwd
```

Service pour les tests de tunnels :
```bash
python3 -m http.server 8080 --bind 127.0.0.1   # joignable seulement depuis la VM elle-même
```

**Tester le changement de clé d'hôte (HKY-02)** : après un snapshot, lance
`sudo rm /etc/ssh/ssh_host_* && sudo ssh-keygen -A && sudo systemctl restart ssh`, puis reconnecte-toi avec Vigie : la connexion doit être refusée. Restaure ensuite le snapshot.

**Comparer une empreinte** : `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` sur la VM doit afficher la même empreinte `SHA256:…` que Vigie.

## 4. Préparer `vigie-win`
- Activer le Bureau à distance avec NLA. Créer un compte local `testrdp`.
- (Optionnel) Installer le serveur OpenSSH (Paramètres → Fonctionnalités facultatives) pour tester SSH vers Windows et PowerShell à distance.
- Snapshot `base-propre` **avant** d'y installer Vigie, pour tester l'installeur sur un Windows vierge (SmartScreen, WebView2).

## 5. Préparer `vigie-desktop`
- Ubuntu Desktop 24.04 avec `freerdp3-x11` (ou `freerdp2-x11`) installé.
- Sert à installer le `.deb` ou l'`AppImage` produits par `build.yml` et à refaire le plan de test de l'étape sous Linux.

## 6. Tests d'intégration en local
Crée `.env.test` à la racine du projet (ignoré par git) :
```
VIGIE_TEST_SSH_HOST=192.168.x.x
VIGIE_TEST_SSH_PORT=22
VIGIE_TEST_SSH_USER=testpwd
VIGIE_TEST_SSH_PASSWORD=...
VIGIE_TEST_SSH_KEY_USER=testkey
VIGIE_TEST_SSH_KEY_PATH=C:/Users/<toi>/.vigie-lab/vigie_test_ed25519
```
Puis lance `npm run test:integration`. Utilise une **clé dédiée aux tests**, jamais ta clé personnelle.
