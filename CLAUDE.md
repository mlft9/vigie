@AGENTS.md

# Instructions propres à Claude Code

## Le porteur du projet et la façon de communiquer
- Réponds **en français**.
- Le porteur est étudiant en coordination de projets informatiques et s'oriente vers la **cybersécurité**. Il ne cherche pas à apprendre le code ligne par ligne : il teste, il donne ses retours et il décide.
- Sois concis. Explique en une ou deux phrases les **choix de sécurité** (c'est ce qui l'intéresse), pas la syntaxe.
- **Mode autonome** : le porteur travaille sur plusieurs projets et n'est pas disponible pour valider en cours de route. Ne lui pose **aucune question** pendant une étape. Quand une décision lui reviendrait (ergonomie, compromis sécurité/confort, dépendance), tranche toi-même selon la règle ci-dessous, trace-la et continue. Il relit tout à la fin.

## Règle de décision en autonomie
1. En cas de compromis, prends **l'option la plus sûre** (sécurité > confort > rapidité de livraison), puis la plus simple et la plus réversible.
2. Si le cahier des charges ou un ADR tranche déjà, suis-le.
3. Note chaque décision non triviale dans la section « Décisions prises en autonomie » du rapport de fin d'étape. Si elle est structurante, ajoute un ADR (statut « Acceptée — prise en autonomie, à relire par le porteur »).
4. Si un point est vraiment impossible à trancher sans le porteur (ex. une exigence du cahier des charges contradictoire), implémente la variante la plus prudente et liste-le dans « Questions pour le porteur » du rapport final. Ne t'arrête pas pour autant.

## Méthode de travail
1. Une étape de `docs/ROADMAP.md` à la fois. Le point d'entrée est la commande `/etape <n>`. L'étape se déroule **de bout en bout sans pause** : plan, code, contrôles, docs, rapport.
2. Commence par rédiger le plan (sous-tâches, fichiers touchés, dépendances avec leur licence, risques de sécurité, tests prévus) dans `docs/plans/etape-<n>.md`, commite-le, puis **enchaîne directement** sur le code, sans attendre de validation.
3. Code par petits commits sur la branche `etape/<n>-<slug>`. Lance `npm run check` avant chaque commit significatif.
4. En fin d'étape, délègue dans cet ordre :
   - `security-reviewer` : revue de sécurité du diff par rapport à la base de la branche. Corrige les problèmes Critique, Élevée et Moyenne, puis relance la revue jusqu'au verdict `OK`.
   - `qa-validator` : vérifie les critères d'acceptation et rédige le plan de test manuel.
   - `docs-keeper` : met à jour la documentation.
5. Pousse la branche d'étape et ouvre une PR vers `main` (sans la fusionner). Présente le rapport final et le plan de test manuel, puis **arrête-toi**. Les retours du porteur arrivent avec `/retour`.

## Ce que tu peux faire seul, à condition de le tracer dans le rapport final
- Ajouter une dépendance : rôle justifié, maintenance active, licence compatible GPL-3.0, `cargo deny check` / `npm audit` propres. Préfère RustCrypto et l'écosystème Tauri officiel.
- Compléter `docs/SECURITE.md` pour documenter ce que l'étape implémente, **sans jamais affaiblir** une règle ou un paramètre existant.
- Ajouter un ADR dans `docs/DECISIONS.md`. Pour revenir sur une décision existante, ajoute un nouvel ADR qui la remplace, sans réécrire l'ancien.
- Modifier `src-tauri/capabilities/` ou la CSP **uniquement** pour ajouter le strict minimum nécessaire à l'étape ou pour durcir. Chaque modification passe par `security-reviewer` et figure dans le rapport.
- Pousser une branche `etape/*` et ouvrir ou mettre à jour sa PR.

## Ce qui reste réservé au porteur (tu ne le fais pas, et tu ne le demandes pas non plus : tu le signales dans le rapport)
- Fusionner une PR ou pousser sur `main`, réécrire l'historique publié (`push --force`).
- Créer un tag ou une release (sauf via `/release`, que seul le porteur lance).
- Toucher aux workflows de CI qui manipulent des secrets.

## Ce que tu ne fais jamais
- Lire les vraies clés SSH ou la config du porteur (`~/.ssh/`), ni des fichiers `.env`. Pour les tests, génère les clés à l'exécution : aucune clé privée n'est commitée, même de test.
- Lire ou afficher la clé privée de signature des mises à jour.
- Désactiver un test, un lint ou une vérification pour « faire passer » la CI. Si quelque chose bloque, explique pourquoi et propose une solution.
- Affaiblir une règle de sécurité d'`AGENTS.md` pour simplifier une implémentation.

## Environnement
- Le porteur développe **nativement sous Windows** (pas WSL) et teste sur son **lab Proxmox** (VM Linux et Windows). Les infos locales (adresses des VM de test, etc.) sont dans `CLAUDE.local.md`, s'il existe.
- Shell possible : PowerShell ou Git Bash. Écris des scripts npm portables (pas de commandes propres à bash dans `package.json`).
- Les tests d'intégration SSH lisent leur cible dans des variables d'environnement (`YOUNJI_TEST_*`). Ne mets jamais d'identifiants en dur.

## Vérifier avant d'affirmer
- Avant d'utiliser une API de `russh`, `tauri`, `ironrdp` ou `xterm`, vérifie la version installée et sa documentation (docs.rs, v2.tauri.app). Ces API évoluent vite.
- Une étape n'est « terminée » que si `npm run check` passe réellement. Montre la sortie en cas de doute.
