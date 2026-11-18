---
name: etape
description: Réalise en autonomie une étape de la feuille de route de Younji (docs/ROADMAP.md), du plan jusqu'au plan de test manuel, sans pause de validation intermédiaire.
argument-hint: [numéro-étape]
disable-model-invocation: true
---

# Étape $0 de la feuille de route (mode autonome)

Le porteur n'est pas disponible pendant l'étape : **aucune question, aucune pause** avant le rapport final. Applique la « Règle de décision en autonomie » de `CLAUDE.md` à chaque choix.

## 1. Comprendre
- Lis l'étape $0 dans `docs/ROADMAP.md` : contenu et critères d'acceptation.
- Lis les exigences du `docs/CAHIER_DES_CHARGES.md` citées par l'étape, les ADR utiles de `docs/DECISIONS.md` et `docs/SECURITE.md`.
- Si l'étape a déjà commencé (une branche `etape/$0-*` existe), lis son `git log` depuis sa base et reprends là où le travail s'est arrêté au lieu de recommencer.

## 2. Préparer
- Si l'arbre de travail n'est pas propre, mets les modifications de côté avec `git stash push -m "avant etape $0"` et signale-le dans le rapport final.
- Choisis la base de la branche :
  - l'étape précédente est fusionnée dans `main` → pars de `main` à jour ;
  - sinon → pars de la branche de l'étape précédente, et note-le dans le rapport (la PR de l'étape $0 dépendra de celle de l'étape précédente).
- Si une étape précédente n'est pas ✅, continue quand même et signale-le dans le rapport.
- Crée la branche `etape/$0-<slug-court>`.
- Passe l'étape en 🟨 dans `docs/ROADMAP.md`.

## 3. Planifier (sans attendre de validation)
Écris `docs/plans/etape-$0.md` avec :
- Les sous-tâches, dans l'ordre, chacune livrable en un ou quelques commits.
- Les fichiers et crates touchés.
- Les **dépendances à ajouter**, avec leur rôle, leur licence et leur dernière version vérifiée.
- Les **risques de sécurité** propres à l'étape et la parade prévue.
- Les tests prévus (unitaires, intégration, interface).
- Les **choix tranchés en autonomie**, avec l'option retenue et la raison (règle « la plus sûre »).
Commite-le (`docs(plan): planifie l'étape $0`), puis enchaîne directement sur l'implémentation.

## 4. Implémenter
- Avance sous-tâche par sous-tâche, avec un commit Conventional Commits par unité cohérente.
- Ajoute les tests en même temps que le code.
- Ajoute toutes les chaînes d'interface en FR et en EN.
- Lance `npm run check` régulièrement, et obligatoirement avant le dernier commit.
- Avant d'ajouter une dépendance : vérifie licence, maintenance et avis RustSec, puis lance `cargo deny check` / `npm audit` après l'ajout.
- Si une décision structurante apparaît en cours de route, tranche-la (option la plus sûre), ajoute-la au plan et note-la pour `docs-keeper` (ADR).
- Si quelque chose bloque (test d'intégration sans serveur, API introuvable…), contourne proprement sans affaiblir la sécurité ni désactiver de vérification, et consigne-le pour le rapport.

## 5. Contrôler
1. Délègue à l'agent `security-reviewer` (diff depuis la base de la branche). Corrige les problèmes **Critique**, **Élevée** et **Moyenne** ; corrige ou justifie les **Faible**. Relance la revue tant que le verdict n'est pas `OK`.
2. Délègue à l'agent `qa-validator` avec le numéro d'étape $0. Si le verdict est `INCOMPLET`, complète puis relance.
3. Délègue à l'agent `docs-keeper`.
4. Commite les corrections et la documentation. Vérifie une dernière fois que `npm run check` passe.

## 6. Livrer au porteur, puis s'arrêter
- Pousse la branche (`git push -u origin etape/$0-<slug>`) et ouvre une PR vers `main` (ou vers la branche de base choisie) avec `gh pr create`. Si `gh` n'est pas installé, donne le lien de création de PR affiché par `git push` (`https://github.com/<dépôt>/pull/new/<branche>`). **Ne la fusionne pas.**
- Présente, en français et de façon concise :
  - Ce qui a été fait (5 à 8 puces au maximum) et les choix de sécurité notables.
  - **Décisions prises en autonomie** : chaque choix qui serait normalement revenu au porteur, l'option retenue et comment revenir dessus.
  - **Dépendances ajoutées** (licence, rôle) et modifications de capabilities / CSP / `SECURITE.md`, s'il y en a.
  - **Questions pour le porteur**, s'il en reste (sans bloquer).
  - Comment tester : `npm run tauri dev`, ou le workflow `build.yml` lancé sur la branche pour obtenir un installeur. Lien de la PR.
  - Le **plan de test manuel** de `qa-validator`, avec ses cases à cocher.
  - Les limites connues et ce qui est reporté.
Ensuite, arrête-toi. Les retours arriveront avec `/retour`.
