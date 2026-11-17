@AGENTS.md

# Instructions propres à Claude Code

## Le porteur du projet et la façon de communiquer
- Réponds **en français**.
- Le porteur est étudiant en coordination de projets informatiques et s'oriente vers la **cybersécurité**. Il ne cherche pas à apprendre le code ligne par ligne : il teste, il donne ses retours et il décide.
- Sois concis. Explique en une ou deux phrases les **choix de sécurité** (c'est ce qui l'intéresse), pas la syntaxe.
- Quand une décision lui revient (ergonomie, compromis sécurité/confort, nouvelle dépendance importante), pose la question avec des options claires plutôt que de trancher seul.

## Méthode de travail
1. Une étape de `docs/ROADMAP.md` à la fois. Le point d'entrée est la commande `/etape <n>`.
2. **Commence chaque étape en mode plan** : sous-tâches, fichiers touchés, dépendances ajoutées (avec leur licence), risques de sécurité, tests prévus. Attends la validation avant de coder.
3. Code par petits commits sur la branche `etape/<n>-<slug>`. Lance `npm run check` avant chaque commit significatif.
4. En fin d'étape, délègue dans cet ordre :
   - `security-reviewer` : revue de sécurité du diff par rapport à `main`. Corrige tout problème bloquant.
   - `qa-validator` : vérifie les critères d'acceptation et rédige le plan de test manuel.
   - `docs-keeper` : met à jour la documentation.
5. Présente le résumé et le plan de test manuel, puis **arrête-toi**. Les retours du porteur arrivent avec `/retour`.

## Ce que tu ne fais jamais sans demander
- `git push`, créer ou fusionner une PR, créer un tag ou une release.
- Ajouter une dépendance (`cargo add`, `npm install <paquet>`).
- Modifier `docs/SECURITE.md`, `docs/DECISIONS.md` sur une décision existante, `src-tauri/capabilities/` ou la CSP.
- Toucher aux workflows de CI qui manipulent des secrets.

## Ce que tu ne fais jamais
- Lire les vraies clés SSH ou la config du porteur (`~/.ssh/`), ni des fichiers `.env`. Pour les tests, génère les clés à l'exécution : aucune clé privée n'est commitée, même de test.
- Lire ou afficher la clé privée de signature des mises à jour.
- Désactiver un test, un lint ou une vérification pour « faire passer » la CI. Si quelque chose bloque, explique pourquoi et propose une solution.
- Affaiblir une règle de sécurité d'`AGENTS.md` pour simplifier une implémentation.

## Environnement
- Le porteur développe **nativement sous Windows** (pas WSL) et teste sur son **lab Proxmox** (VM Linux et Windows). Les infos locales (adresses des VM de test, etc.) sont dans `CLAUDE.local.md`, s'il existe.
- Shell possible : PowerShell ou Git Bash. Écris des scripts npm portables (pas de commandes propres à bash dans `package.json`).
- Les tests d'intégration SSH lisent leur cible dans des variables d'environnement (`VIGIE_TEST_*`). Ne mets jamais d'identifiants en dur.

## Vérifier avant d'affirmer
- Avant d'utiliser une API de `russh`, `tauri`, `ironrdp` ou `xterm`, vérifie la version installée et sa documentation (docs.rs, v2.tauri.app). Ces API évoluent vite.
- Une étape n'est « terminée » que si `npm run check` passe réellement. Montre la sortie en cas de doute.
