# Politique de sécurité / Security policy

*English below.*

## Français

Vigie est un client SSH / SFTP / RDP qui manipule des identifiants : la sécurité est une exigence de premier rang. Merci de nous aider à le garder sûr.

### Versions prises en charge
Le projet est en développement actif et n'a pas encore de version publiée. Seule la branche `main` reçoit des correctifs de sécurité.

### Signaler une vulnérabilité
**N'ouvrez pas d'issue publique.** Utilisez le signalement privé de GitHub :
[Security > Report a vulnerability](https://github.com/mlft9/vigie/security/advisories/new).

Indiquez si possible :
- la version ou le commit concerné, et le système (Windows / Linux) ;
- les étapes pour reproduire, et l'impact estimé ;
- une preuve de concept, **sans données réelles** (aucun vrai mot de passe, aucune vraie clé).

### Délais
Vigie est un projet personnel, maintenu sur le temps libre :
- accusé de réception sous **7 jours** ;
- première évaluation sous **14 jours** ;
- correctif et publication coordonnée avec vous dès que possible. Vous serez crédité si vous le souhaitez.

### Périmètre
Le modèle de menace est décrit dans [`docs/SECURITE.md`](docs/SECURITE.md). Sont notamment **hors périmètre** : un logiciel malveillant qui s'exécute avec les droits de l'utilisateur pendant que le coffre est déverrouillé, et les vulnérabilités des dépendances déjà connues et suivies (signalez-les plutôt en amont).

## English

Vigie is an SSH / SFTP / RDP client that handles credentials: security is a first-class requirement. Thank you for helping keep it safe.

### Supported versions
The project is under active development and has no release yet. Only the `main` branch receives security fixes.

### Reporting a vulnerability
**Do not open a public issue.** Please use GitHub private vulnerability reporting:
[Security > Report a vulnerability](https://github.com/mlft9/vigie/security/advisories/new).

Please include, if possible:
- the affected version or commit, and the OS (Windows / Linux);
- steps to reproduce and the estimated impact;
- a proof of concept **without real data** (no real password or key).

### Timeline
Vigie is a personal project maintained in spare time:
- acknowledgement within **7 days**;
- initial assessment within **14 days**;
- fix and coordinated disclosure with you as soon as possible. You will be credited if you wish.

### Scope
The threat model is described in [`docs/SECURITE.md`](docs/SECURITE.md) (in French). Notably **out of scope**: malware running with the user's privileges while the vault is unlocked, and already known and tracked dependency vulnerabilities (please report those upstream).
